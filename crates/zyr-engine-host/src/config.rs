//! Rendering of the `sunshine.conf` and `apps.json` files.

use std::path::{Path, PathBuf};

use zyr_proto::net::EnginePorts;
use zyr_proto::session::Serving;

/// Internal encryption mode of the GameStream protocol over loopback.
///
/// End-to-end encryption is carried by the ZyrDesk tunnel, and the
/// engine's traffic only ever exists on loopback. The mandatory mode
/// stays available for the advanced "paranoid" setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InnerEncryption {
    #[default]
    Off,
    Mandatory,
}

impl InnerEncryption {
    fn value(self) -> &'static str {
        match self {
            InnerEncryption::Off => "0",
            InnerEncryption::Mandatory => "2",
        }
    }
}

/// Interfaces the engine accepts connections on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Listening {
    /// Local machine only.
    ///
    /// This is the target: the ZyrDesk tunnel is then the one and only
    /// way to the engine, which exposes nothing to the network.
    #[default]
    Local,
    /// Every network interface.
    ///
    /// Needed as long as no tunnel carries the traffic, without which no
    /// other computer could reach the engine at all.
    Network,
}

/// Settings of one host engine instance.
#[derive(Debug, Clone)]
pub struct SunshineConfig {
    ports: EnginePorts,
    data_dir: PathBuf,
    logs_dir: PathBuf,
    listening: Listening,
    encryption: InnerEncryption,
    serving: Serving,
    output_name: Option<String>,
    alone_on_the_screen: bool,
    adapter_name: Option<String>,
}

/// Frame rate the engine guarantees even on a frozen screen, when it is
/// asked to guarantee one at all.
///
/// The engine only encodes a frame when the screen changes, and only
/// resends on its own after a delay. Its default is half the requested
/// rate: a still desktop then reaches thirty frames per second, and both
/// mouse motion and window animations turn choppy.
///
/// For a remote desktop, smoothness beats the few redundant frames it
/// saves: a frozen screen re-encodes for almost nothing.
const DESKTOP_MINIMUM_FPS: f64 = 60.0;

impl SunshineConfig {
    /// `logs_dir` is the log folder shared by every component: the
    /// engine writes its own there instead of hiding it inside its
    /// private state.
    pub fn new(
        ports: EnginePorts,
        data_dir: impl Into<PathBuf>,
        logs_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            ports,
            data_dir: data_dir.into(),
            logs_dir: logs_dir.into(),
            listening: Listening::default(),
            encryption: InnerEncryption::default(),
            serving: Serving::default(),
            output_name: None,
            alone_on_the_screen: false,
            adapter_name: None,
        }
    }

    /// How this computer makes the pictures it serves.
    pub fn with_serving(mut self, serving: Serving) -> Self {
        self.serving = serving;
        self
    }

    /// Opens the engine to the network. Only ever without a tunnel.
    pub fn with_listening(mut self, listening: Listening) -> Self {
        self.listening = listening;
        self
    }

    pub fn with_encryption(mut self, mode: InnerEncryption) -> Self {
        self.encryption = mode;
        self
    }

    /// Screen to capture, by the name the engine knows it under.
    pub fn with_screen(mut self, output_name: impl Into<String>) -> Self {
        self.output_name = Some(output_name.into());
        self
    }

    /// The screen this computer grew for a session, which is to be the
    /// only one there is while that session lasts.
    ///
    /// The only one, and not merely the one being captured. A screen
    /// nobody is sitting in front of is an empty desktop: the taskbar,
    /// the windows and the icons are all on the screen the person here
    /// has, and a session shown the empty one would show nothing at all.
    /// Putting the others out for the length of the session moves the
    /// whole desktop onto it, which is what makes the far end see this
    /// computer rather than a blank copy of it.
    ///
    /// The screen here goes dark meanwhile, which is the same thing the
    /// engine already does to its size and puts back afterwards.
    pub fn with_screen_of_its_own(mut self, output_name: impl Into<String>) -> Self {
        self.output_name = Some(output_name.into());
        self.alone_on_the_screen = true;
        self
    }

    /// Encoding GPU, which matters on hybrid machines.
    pub fn with_gpu(mut self, adapter_name: impl Into<String>) -> Self {
        self.adapter_name = Some(adapter_name.into());
        self
    }

    pub fn ports(&self) -> EnginePorts {
        self.ports
    }

    pub fn conf_path(&self) -> PathBuf {
        self.data_dir.join("engine.conf")
    }

    pub fn apps_path(&self) -> PathBuf {
        self.data_dir.join("apps.json")
    }

    /// State the engine keeps for itself, paired computers above all.
    pub fn state_path(&self) -> PathBuf {
        self.data_dir.join("engine_state.json")
    }

    /// Identifiers of the engine's local API, apart from its state.
    ///
    /// The engine puts both in one file unless told otherwise, and that
    /// default costs every pairing on the machine. We write fresh
    /// identifiers at every start of the service, and the engine writes
    /// them by reading the whole file and writing it back out through a
    /// library that cannot keep a JSON list: the list of paired
    /// computers comes back as something nothing can read. The far
    /// computer was therefore forgotten at every start, and every session
    /// after one ended on « l'ordinateur distant n'a pas répondu » until
    /// the two had been introduced again.
    ///
    /// Two files, and the state is never rewritten by anything but the
    /// engine itself.
    pub fn credentials_path(&self) -> PathBuf {
        self.data_dir.join("engine_credentials.json")
    }

    /// Log the engine writes itself.
    pub fn log_path(&self) -> PathBuf {
        self.logs_dir.join("engine.log")
    }

    /// Folders the engine assumes exist when it starts.
    pub fn required_dirs(&self) -> [&Path; 2] {
        [&self.data_dir, &self.logs_dir]
    }

    /// Contents of the `sunshine.conf` file.
    pub fn render_conf(&self) -> String {
        let shown = |path: &Path| path.display().to_string();
        let mut lines = Vec::new();
        // A missing address means "every interface" to the engine.
        if self.listening == Listening::Local {
            lines.push("bind_address = 127.0.0.1".to_string());
        }
        lines.extend([
            format!("port = {}", self.ports.base()),
            "address_family = ipv4".to_string(),
            "origin_web_ui_allowed = pc".to_string(),
            "system_tray = disabled".to_string(),
            format!("capture = {}", self.serving.capture),
            format!("lan_encryption_mode = {}", self.encryption.value()),
            format!("wan_encryption_mode = {}", self.encryption.value()),
            "upnp = disabled".to_string(),
            format!("file_apps = {}", shown(&self.apps_path())),
            format!("file_state = {}", shown(&self.state_path())),
            format!("credentials_file = {}", shown(&self.credentials_path())),
            // The far desktop is put at the size and the rate the session
            // asks for, and put back afterwards.
            //
            // Without this the engine leaves the desktop as it is and
            // squeezes it into the stream, keeping its shape by burning
            // black bars into every frame it sends. A sixteen by ten
            // laptop watched on a sixteen by nine screen loses ninety-six
            // pixels of picture down each side, for good, before anything
            // is even encoded, and no amount of care at this end can put
            // them back.
            //
            // Four lines and not one, because each does a different half
            // of it. The first turns the whole thing on: left alone the
            // engine touches nothing, whatever the three others say. The
            // next two say what may be changed. The last says when to put
            // it back, and it is not the obvious answer: the engine
            // otherwise waits for the application it is showing to stop,
            // and the one we show is the far desktop itself, which never
            // stops. Leaving a session without closing it would hand back
            // a laptop still at the size we gave it.
            format!(
                "dd_configuration_option = {}",
                if self.alone_on_the_screen {
                    "ensure_only_display"
                } else {
                    "ensure_active"
                }
            ),
            "dd_resolution_option = auto".to_string(),
            "dd_refresh_rate_option = auto".to_string(),
            "dd_config_revert_on_disconnect = enabled".to_string(),
            format!("log_path = {}", shown(&self.log_path())),
            "min_log_level = info".to_string(),
        ]);
        // Left out entirely rather than turned down, when it is off: the
        // engine's own answer is half the rate that was asked for, and
        // half is what « off » means here. Writing a number would be
        // choosing a third thing nobody asked for.
        if self.serving.steady_rate {
            lines.push(format!("minimum_fps_target = {DESKTOP_MINIMUM_FPS}"));
        }
        if let Some(output) = &self.output_name {
            lines.push(format!("output_name = {output}"));
        }
        if let Some(adapter) = &self.adapter_name {
            lines.push(format!("adapter_name = {adapter}"));
        }
        let mut rendered = lines.join("\n");
        rendered.push('\n');
        rendered
    }

    /// Contents of the `apps.json` file: the desktop and nothing else.
    ///
    /// No thumbnail is declared: it would only serve the client engine's
    /// application list, which the product never shows.
    pub fn render_apps(&self) -> String {
        concat!(
            "{\n",
            "  \"env\": {},\n",
            "  \"apps\": [\n",
            "    {\n",
            "      \"name\": \"Desktop\"\n",
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
    use zyr_proto::session::Capture;

    fn test_config() -> SunshineConfig {
        SunshineConfig::new(
            EnginePorts::new(42100).unwrap(),
            "/data/zyrdesk/host",
            "/data/zyrdesk/logs",
        )
    }

    #[test]
    fn the_engine_is_closed_to_the_network_by_default() {
        let rendered = test_config().render_conf();
        assert!(rendered.contains("bind_address = 127.0.0.1"));
    }

    #[test]
    fn opening_to_the_network_drops_the_address_restriction() {
        let rendered = test_config()
            .with_listening(Listening::Network)
            .render_conf();
        assert!(!rendered.contains("bind_address"), "{rendered}");
        // The rest of the policy stays put all the same.
        assert!(rendered.contains("origin_web_ui_allowed = pc"));
        assert!(rendered.contains("upnp = disabled"));
    }

    #[test]
    fn the_conf_applies_the_isolation_policy() {
        let rendered = test_config().render_conf();
        assert!(rendered.contains("port = 42100"));
        assert!(rendered.contains("origin_web_ui_allowed = pc"));
        assert!(rendered.contains("system_tray = disabled"));
        assert!(rendered.contains("capture = ddx"));
        assert!(rendered.contains("upnp = disabled"));
        assert!(rendered.contains("lan_encryption_mode = 0"));
    }

    #[test]
    fn the_far_desktop_is_resized_for_the_session_and_put_back_after() {
        // Sans ces quatre lignes, le moteur garde le bureau tel quel et
        // grave des bandes noires dans chaque image pour lui garder sa
        // forme. Et sans la dernière, il ne remet jamais rien : le
        // bureau que nous montrons ne s'arrête pas, et c'est à son arrêt
        // qu'il rendrait la main.
        let rendered = test_config().render_conf();
        for line in [
            "dd_configuration_option = ensure_active",
            "dd_resolution_option = auto",
            "dd_refresh_rate_option = auto",
            "dd_config_revert_on_disconnect = enabled",
        ] {
            assert!(rendered.contains(line), "{line} manque dans la conf");
        }
    }

    #[test]
    fn the_state_and_the_identifiers_are_two_files() {
        // Un seul fichier pour les deux coûtait tous les appairages de
        // la machine à chaque démarrage du service : le moteur relit et
        // réécrit ce fichier pour y poser ses identifiants, à travers
        // une bibliothèque qui ne sait pas rendre une liste JSON telle
        // qu'elle l'a lue.
        let config = test_config();
        assert_ne!(config.state_path(), config.credentials_path());
    }

    #[test]
    fn the_conf_keeps_the_state_in_the_data_folder() {
        // The expected paths come from the configuration itself: writing
        // them out by hand would only test the separator of whichever
        // system ran the test.
        let config = test_config();
        let rendered = config.render_conf();
        for (key, path) in [
            ("file_apps", config.apps_path()),
            ("file_state", config.state_path()),
            ("credentials_file", config.credentials_path()),
            ("log_path", config.log_path()),
        ] {
            let line = format!("{key} = {}", path.display());
            assert!(rendered.contains(&line), "{line} missing from the conf");
            assert!(
                config
                    .required_dirs()
                    .iter()
                    .any(|dir| path.starts_with(dir)),
                "{key} sits outside the folders we create"
            );
        }
    }

    #[test]
    fn the_required_folders_cover_both_state_and_logs() {
        let config = test_config();
        let required = config.required_dirs();
        for path in [config.conf_path(), config.apps_path(), config.log_path()] {
            let parent = path.parent().unwrap();
            assert!(
                required.contains(&parent),
                "{} is covered by no folder we create",
                path.display()
            );
        }
    }

    #[test]
    fn the_optional_settings_are_absent_by_default() {
        let rendered = test_config().render_conf();
        assert!(!rendered.contains("output_name"));
        assert!(!rendered.contains("adapter_name"));

        let with_both = test_config()
            .with_screen(r"\\.\DISPLAY1")
            .with_gpu("NVIDIA GeForce RTX 4070")
            .render_conf();
        assert!(with_both.contains(r"output_name = \\.\DISPLAY1"));
        assert!(with_both.contains("adapter_name = NVIDIA GeForce RTX 4070"));
    }

    #[test]
    fn a_screen_grown_for_the_session_becomes_the_only_one() {
        // Sinon la session montre un bureau vide : la barre des tâches,
        // les fenêtres et les icônes sont sur l'écran de la personne
        // assise devant, pas sur celui qu'on vient de faire pousser.
        let ordinary = test_config().render_conf();
        assert!(ordinary.contains("dd_configuration_option = ensure_active"));

        let grown = test_config()
            .with_screen_of_its_own("{64243705-4020-5895-b923-adc862c3457e}")
            .render_conf();
        assert!(grown.contains("dd_configuration_option = ensure_only_display"));
        assert!(grown.contains("output_name = {64243705-4020-5895-b923-adc862c3457e}"));
        // Et tout est remis en place à la fin, sans quoi l'écran de la
        // personne assise devant resterait éteint.
        assert!(grown.contains("dd_config_revert_on_disconnect = enabled"));
    }

    #[test]
    fn the_minimum_frame_rate_aims_at_a_smooth_desktop() {
        let rendered = test_config().render_conf();
        // Without this setting, the engine falls back to half the
        // requested rate as soon as the screen stops changing.
        assert!(rendered.contains("minimum_fps_target = 60"), "{rendered}");
    }

    #[test]
    fn a_computer_that_cannot_keep_up_stops_resending_a_still_screen() {
        // Absente et non baissée : la réponse propre du moteur est la
        // moitié de la cadence demandée, et c'est ce que « éteint » veut
        // dire. Écrire un nombre serait en choisir un troisième que
        // personne n'a demandé.
        let rendered = test_config()
            .with_serving(Serving {
                steady_rate: false,
                ..Serving::default()
            })
            .render_conf();
        assert!(!rendered.contains("minimum_fps_target"), "{rendered}");
    }

    #[test]
    fn the_way_the_screen_is_taken_is_the_one_that_was_asked_for() {
        // Le défaut voit les invites administrateur et l'écran de
        // connexion ; l'autre est plus rapide sur certaines machines et
        // ne les voit pas. Les deux doivent pouvoir sortir d'ici.
        assert!(test_config().render_conf().contains("capture = ddx"));
        let rendered = test_config()
            .with_serving(Serving {
                capture: Capture::Windows,
                ..Serving::default()
            })
            .render_conf();
        assert!(rendered.contains("capture = wgc"), "{rendered}");
    }

    #[test]
    fn the_paranoid_mode_turns_the_inner_encryption_on() {
        let rendered = test_config()
            .with_encryption(InnerEncryption::Mandatory)
            .render_conf();
        assert!(rendered.contains("lan_encryption_mode = 2"));
        assert!(rendered.contains("wan_encryption_mode = 2"));
    }

    #[test]
    fn the_apps_file_holds_the_desktop_and_nothing_else() {
        let apps = test_config().render_apps();
        assert!(apps.contains("\"name\": \"Desktop\""));
        assert_eq!(apps.matches("\"name\"").count(), 1);
    }
}
