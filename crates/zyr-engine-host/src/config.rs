//! Rendu du fichier `sunshine.conf` et du fichier `apps.json`.

use std::path::{Path, PathBuf};

use zyr_proto::net::EnginePorts;

/// Mode de chiffrement interne du protocole GameStream sur loopback.
///
/// Le chiffrement de bout en bout est porté par le tunnel ZyrDesk ; le
/// trafic du moteur n'existe qu'en loopback. Le mode obligatoire reste
/// disponible pour le réglage avancé « paranoïaque ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChiffrementInterne {
    #[default]
    Desactive,
    Obligatoire,
}

impl ChiffrementInterne {
    fn valeur(self) -> &'static str {
        match self {
            ChiffrementInterne::Desactive => "0",
            ChiffrementInterne::Obligatoire => "2",
        }
    }
}

/// Paramètres d'une instance du moteur hôte.
#[derive(Debug, Clone)]
pub struct SunshineConfig {
    ports: EnginePorts,
    data_dir: PathBuf,
    chiffrement: ChiffrementInterne,
    output_name: Option<String>,
    adapter_name: Option<String>,
}

impl SunshineConfig {
    pub fn new(ports: EnginePorts, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            ports,
            data_dir: data_dir.into(),
            chiffrement: ChiffrementInterne::default(),
            output_name: None,
            adapter_name: None,
        }
    }

    pub fn avec_chiffrement(mut self, mode: ChiffrementInterne) -> Self {
        self.chiffrement = mode;
        self
    }

    /// Écran à capturer (identifiant Windows du périphérique d'affichage).
    pub fn avec_ecran(mut self, output_name: impl Into<String>) -> Self {
        self.output_name = Some(output_name.into());
        self
    }

    /// GPU d'encodage (utile sur les machines hybrides).
    pub fn avec_gpu(mut self, adapter_name: impl Into<String>) -> Self {
        self.adapter_name = Some(adapter_name.into());
        self
    }

    pub fn ports(&self) -> EnginePorts {
        self.ports
    }

    pub fn chemin_conf(&self) -> PathBuf {
        self.data_dir.join("engine.conf")
    }

    pub fn chemin_apps(&self) -> PathBuf {
        self.data_dir.join("apps.json")
    }

    /// Contenu du fichier `sunshine.conf`.
    pub fn rendu_conf(&self) -> String {
        let d = |p: &Path| p.display().to_string();
        let mut lignes = vec![
            "bind_address = 127.0.0.1".to_string(),
            format!("port = {}", self.ports.base()),
            "address_family = ipv4".to_string(),
            "origin_web_ui_allowed = pc".to_string(),
            "system_tray = disabled".to_string(),
            "capture = ddx".to_string(),
            format!("lan_encryption_mode = {}", self.chiffrement.valeur()),
            format!("wan_encryption_mode = {}", self.chiffrement.valeur()),
            "upnp = disabled".to_string(),
            format!("file_apps = {}", d(&self.chemin_apps())),
            format!(
                "file_state = {}",
                d(&self.data_dir.join("engine_state.json"))
            ),
            format!(
                "credentials_file = {}",
                d(&self.data_dir.join("engine_state.json"))
            ),
            format!("log_path = {}", d(&self.data_dir.join("logs/engine.log"))),
            "min_log_level = info".to_string(),
        ];
        if let Some(output) = &self.output_name {
            lignes.push(format!("output_name = {output}"));
        }
        if let Some(adapter) = &self.adapter_name {
            lignes.push(format!("adapter_name = {adapter}"));
        }
        let mut rendu = lignes.join("\n");
        rendu.push('\n');
        rendu
    }

    /// Contenu du fichier `apps.json` : le bureau uniquement.
    pub fn rendu_apps(&self) -> String {
        concat!(
            "{\n",
            "  \"env\": {},\n",
            "  \"apps\": [\n",
            "    {\n",
            "      \"name\": \"Desktop\",\n",
            "      \"image-path\": \"desktop.png\"\n",
            "    }\n",
            "  ]\n",
            "}\n"
        )
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_test() -> SunshineConfig {
        SunshineConfig::new(EnginePorts::new(42100).unwrap(), "/data/zyrdesk/host")
    }

    #[test]
    fn conf_verrouille_le_moteur_en_loopback() {
        let rendu = config_test().rendu_conf();
        assert!(rendu.contains("bind_address = 127.0.0.1"));
        assert!(rendu.contains("port = 42100"));
        assert!(rendu.contains("origin_web_ui_allowed = pc"));
        assert!(rendu.contains("system_tray = disabled"));
        assert!(rendu.contains("capture = ddx"));
        assert!(rendu.contains("upnp = disabled"));
        assert!(rendu.contains("lan_encryption_mode = 0"));
    }

    #[test]
    fn conf_place_l_etat_dans_le_repertoire_de_donnees() {
        let rendu = config_test().rendu_conf();
        assert!(rendu.contains("file_apps = /data/zyrdesk/host/apps.json"));
        assert!(rendu.contains("file_state = /data/zyrdesk/host/engine_state.json"));
        assert!(rendu.contains("credentials_file = /data/zyrdesk/host/engine_state.json"));
        assert!(rendu.contains("log_path = /data/zyrdesk/host/logs/engine.log"));
    }

    #[test]
    fn options_facultatives_absentes_par_defaut() {
        let rendu = config_test().rendu_conf();
        assert!(!rendu.contains("output_name"));
        assert!(!rendu.contains("adapter_name"));
        let avec = config_test()
            .avec_ecran(r"\\.\DISPLAY1")
            .avec_gpu("NVIDIA GeForce RTX 4070")
            .rendu_conf();
        assert!(avec.contains(r"output_name = \\.\DISPLAY1"));
        assert!(avec.contains("adapter_name = NVIDIA GeForce RTX 4070"));
    }

    #[test]
    fn mode_paranoiaque_active_le_chiffrement_interne() {
        let rendu = config_test()
            .avec_chiffrement(ChiffrementInterne::Obligatoire)
            .rendu_conf();
        assert!(rendu.contains("lan_encryption_mode = 2"));
        assert!(rendu.contains("wan_encryption_mode = 2"));
    }

    #[test]
    fn apps_json_ne_contient_que_le_bureau() {
        let apps = config_test().rendu_apps();
        assert!(apps.contains("\"name\": \"Desktop\""));
        assert_eq!(apps.matches("\"name\"").count(), 1);
    }
}
