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

/// Where the engine writes its own log, in a folder of logs.
///
/// Named here and read from here, so the two never part company. It is
/// not one of the four files the journal gathers and empties, and that
/// matters: everything this product reads back out of the engine is read
/// from this file, and a file the person can empty is a file that stops
/// answering the moment they do.
pub fn engine_log_in(logs_dir: &Path) -> PathBuf {
    logs_dir.join("engine.log")
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
///
/// The fastest a session is ever opened at, and not a number of its own,
/// because this file is written before anybody knows what rate the
/// session will ask for. The engine never resends above that rate, so
/// asking for the ceiling is how this comes out as « the rate of the
/// session », whatever screen it turns out to be shown on.
const DESKTOP_MINIMUM_FPS: u32 = zyr_proto::session::FASTEST_RATE;

/// What the engine is told to keep up on a still screen, on and off.
///
/// The ceiling when it is on, for the reason just above, and nought when
/// it is off, which is the engine's own answer of half the rate a session
/// asks for. Said here once and read twice: written into the file the
/// engine starts on, and asked of the engine that runs when a session
/// changes its mind.
pub fn minimum_fps_target(steady_rate: bool) -> u32 {
    if steady_rate { DESKTOP_MINIMUM_FPS } else { 0 }
}

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
    ///
    /// Its own name for it and no other: the engine looks this up in the
    /// list it publishes, and anything that is not in that list names
    /// nothing at all, on which it silently films what it finds.
    ///
    /// Said on every computer. The main screen where there are screens,
    /// the one this computer grew for itself where there are none. Left
    /// unsaid, the engine films whichever screen the graphics card
    /// enumerates first, and picks again every time it has to start
    /// filming over.
    pub fn with_screen(mut self, output_name: impl Into<String>) -> Self {
        self.output_name = Some(output_name.into());
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
        engine_log_in(&self.logs_dir)
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
            // ZyrDesk depends on nobody, and this line is where that
            // stopped being true by omission.
            //
            // Left unwritten, the engine looks for Steam's own sound card
            // on this machine and installs it if it finds it, then makes
            // it the computer's default output for the length of every
            // session. That is how the engines empty a room while keeping
            // the sound in the stream, and it means a product that only
            // works properly on machines where somebody happened to
            // install Steam. This product empties the room itself, on the
            // machine's real sound card, with nothing installed and
            // nothing borrowed.
            "install_steam_audio_drivers = disabled".to_string(),
            // And no card of anybody else's is looked for either. Left
            // empty, this very field is what sends the engine hunting for
            // Steam's: empty does not mean « none » to it, it means
            // « Steam's », and a machine that has Steam would have its
            // sound quietly moved onto it at every session. A name no
            // card will ever carry does mean none. The engine says once a
            // session that it could not find it, which is the answer that
            // was wanted, spelled out in its own log.
            "virtual_sink = aucune-carte-son-virtuelle".to_string(),
            format!("file_apps = {}", shown(&self.apps_path())),
            format!("file_state = {}", shown(&self.state_path())),
            format!("credentials_file = {}", shown(&self.credentials_path())),
            // Nothing here tells the engine to touch this computer's
            // screens, and that is the whole of what is said about them:
            // left alone, it touches none of them.
            //
            // It offers to, and it was taken up on the offer, and that is
            // exactly what had to be undone. Asked to put a screen at the
            // size a session wants, it also puts every other screen out
            // for the length of that session, then puts back an
            // arrangement it noted at its own start. It gives up on that
            // as soon as anything else has moved a screen since, and what
            // it does when it gives up is switch every screen it can find
            // back on. Somebody with three screens and a television they
            // switch on twice a month got the television back at every
            // start of the service, and one of the three stayed dark.
            //
            // So the product does it: it notes the desk before a session,
            // puts the one screen the session watches at the size it
            // asked for, and puts the whole desk back afterwards, off
            // screens included. The engine is left with the one job it is
            // good at, which is filming what is in front of it.
            format!("log_path = {}", shown(&self.log_path())),
            "min_log_level = info".to_string(),
            // The engine gives a session up after this long without one
            // packet from the computer watching, and its own answer is
            // ten seconds, which is shorter than the tunnel carrying it.
            // A road between two homes that goes quiet for a dozen
            // seconds and comes back is a road the tunnel rides through,
            // and the engine was ending the session under it (D138).
            format!(
                "ping_timeout = {}",
                zyr_proto::net::UNHEARD_LIMIT.as_millis()
            ),
        ]);
        // Left out entirely rather than turned down, when it is off: the
        // engine's own answer is half the rate that was asked for, and
        // half is what « off » means here. Writing a number other than
        // the key's own default would be choosing a third thing nobody
        // asked for.
        if self.serving.steady_rate {
            lines.push(format!(
                "minimum_fps_target = {}",
                minimum_fps_target(self.serving.steady_rate)
            ));
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
    fn the_engine_waits_as_long_as_the_tunnel_before_giving_a_session_up() {
        // Le 4 septembre, deux sessions sont mortes à la dixième seconde
        // d'un silence du réseau, deux fois : le tunnel en porte trente,
        // le moteur en portait dix, et c'est le plus court qui décide.
        // La patience est écrite une fois pour tout le produit, et le
        // moteur en est informé.
        let rendered = test_config().render_conf();
        assert!(
            rendered.contains(&format!(
                "ping_timeout = {}",
                zyr_proto::net::UNHEARD_LIMIT.as_millis()
            )),
            "{rendered}"
        );
    }

    #[test]
    fn the_engine_is_told_nothing_at_all_about_this_computers_screens() {
        // Le relevé de Victor : « il m'a coupé mes écrans physiques ».
        // Le moteur sait arranger les écrans et le faisait, y compris
        // éteindre ceux qu'il ne filme pas et rallumer une télé que son
        // propriétaire garde éteinte. C'est le produit qui relève le
        // bureau et le remet maintenant ; le moteur filme, et rien
        // d'autre. Aucune de ces lignes ne doit reparaître.
        let rendered = test_config().render_conf();
        for line in [
            "dd_configuration_option",
            "dd_resolution_option",
            "dd_refresh_rate_option",
            "dd_config_revert_on_disconnect",
            "dd_config_revert_delay",
        ] {
            assert!(!rendered.contains(line), "{line} est revenu dans la conf");
        }
    }

    #[test]
    fn ce_produit_ne_depend_de_personne_pour_le_son() {
        // Les deux lignes se tiennent. Sans la première, le moteur
        // installe la carte son de Steam quand il en trouve les fichiers
        // sur la machine. Sans la seconde, il cherche cette même carte et
        // fait passer le son de l'ordinateur dessus le temps de chaque
        // session : vide ne veut pas dire « aucune » pour lui, ça veut
        // dire « celle de Steam ».
        let rendered = test_config().render_conf();
        assert!(rendered.contains("install_steam_audio_drivers = disabled"));
        assert!(rendered.contains("virtual_sink = aucune-carte-son-virtuelle"));
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

        // L'écran se nomme du nom que le moteur lui donne, jamais de
        // celui que Windows numérote : ce dernier ne figure pas dans la
        // liste qu'il consulte, donc il ne nomme rien et le moteur filme
        // ce qu'il trouve, sans le dire.
        let with_both = test_config()
            .with_screen("{aed131a5-3850-5dc6-89be-4967cca4ef04}")
            .with_gpu("NVIDIA GeForce RTX 4070")
            .render_conf();
        assert!(with_both.contains("output_name = {aed131a5-3850-5dc6-89be-4967cca4ef04}"));
        assert!(with_both.contains("adapter_name = NVIDIA GeForce RTX 4070"));
    }

    #[test]
    fn the_screen_a_computer_grew_is_named_without_a_word_about_the_others() {
        // Nommer l'écran à filmer et arranger les écrans sont deux choses
        // différentes, et elles étaient devenues une seule. Un ordinateur
        // sans écran branché fait pousser le sien et le nomme ici ; ce
        // qu'il ne fait plus, c'est demander au moteur d'éteindre le reste.
        let grown = test_config()
            .with_screen("{64243705-4020-5895-b923-adc862c3457e}")
            .render_conf();
        assert!(grown.contains("output_name = {64243705-4020-5895-b923-adc862c3457e}"));
        assert!(!grown.contains("dd_configuration_option"));
    }

    #[test]
    fn the_minimum_frame_rate_aims_at_a_smooth_desktop() {
        let rendered = test_config().render_conf();
        // Without this setting, the engine falls back to half the
        // requested rate as soon as the screen stops changing.
        //
        // Le plafond des sessions et non soixante : ce fichier est écrit
        // avant qu'on sache à quelle cadence la session s'ouvrira, et le
        // moteur ne réémet jamais au-dessus de ce qu'elle demande.
        assert!(
            rendered.contains(&format!(
                "minimum_fps_target = {}",
                zyr_proto::session::FASTEST_RATE
            )),
            "{rendered}"
        );
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
    fn the_floor_asked_of_a_running_engine_is_the_one_its_file_carries() {
        // Le même nombre par les deux chemins : le fichier que le moteur
        // lit à son démarrage, et la porte par laquelle on le lui demande
        // pendant qu'il tourne. Zéro quand c'est éteint, qui est la
        // valeur propre de cette clé chez le moteur : la moitié de la
        // cadence demandée.
        assert_eq!(minimum_fps_target(true), zyr_proto::session::FASTEST_RATE);
        assert_eq!(minimum_fps_target(false), 0);
        assert!(test_config().render_conf().contains(&format!(
            "minimum_fps_target = {}",
            minimum_fps_target(true)
        )));
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
