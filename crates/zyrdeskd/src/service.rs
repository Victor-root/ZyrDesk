//! The Windows service: installation, lifecycle, clean stop.
//!
//! Windows starts this program, talks to it through the service control
//! manager, and expects an answer. Answering is compulsory: a service
//! that takes too long to confirm a stop is killed, and its engine with
//! it, with nothing put away.
//!
//! Everything touching the service control manager lives here. The real
//! work is in the supervisor, which does not know it is a service.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use windows_service::service::{
    Service, ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl,
    ServiceExitCode, ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{Result as ServiceResult, define_windows_service, service_dispatcher};

use zyr_proto::log::Log;
use zyr_proto::paths;

use crate::supervisor::{self, End, StopOrder};

/// Internal service name, the one Windows uses.
pub const NAME: &str = "ZyrDesk";

/// Name shown in the services console.
const DISPLAY_NAME: &str = "ZyrDesk";

const DESCRIPTION: &str =
    "Rend cet ordinateur accessible à distance, y compris avant l'ouverture de session.";

/// Argument Windows uses to start this program as a service.
///
/// Without it, the same executable serves as a command-line installer.
/// Telling them apart explicitly beats guessing where the launch came
/// from.
pub const SERVICE_ARGUMENT: &str = "--run-as-service";

/// Time given to the service to stop before it is removed anyway.
const STOP_DELAY: Duration = Duration::from_secs(30);

/// How often its state is asked for while waiting.
const STOP_STEP: Duration = Duration::from_millis(250);

/// How long Windows is told a stop may take.
///
/// It has to cover the whole of it: the engine is asked to go and given
/// time to put the far computer's screen back before it is taken. A
/// service that is still tidying up when this runs out is killed, and
/// the engine with it, which is exactly the tidying up that matters.
const STOPPING_TAKES: Duration = Duration::from_secs(45);

/// A service runs in exactly one copy, so what it shares with its
/// handler is legitimately global.
static STOP_ORDER: std::sync::OnceLock<StopOrder> = std::sync::OnceLock::new();

/// The handle a state is announced on, kept where the control handler
/// can reach it: Windows calls that handler before anything else is
/// handed to it.
static ANNOUNCING: std::sync::OnceLock<service_control_handler::ServiceStatusHandle> =
    std::sync::OnceLock::new();

/// Hands control to Windows, which will call the service entry point.
pub fn hand_over_to_windows() -> ServiceResult<()> {
    service_dispatcher::start(NAME, ffi_service_main)
}

define_windows_service!(ffi_service_main, service_main);

fn service_main(_arguments: Vec<OsString>) {
    let log = match Log::open(&log_path()) {
        Ok(log) => log,
        // Without a log there is nothing left to tell anyone: better not
        // to start at all than to run mute and invisible.
        Err(_) => return,
    };

    if let Err(e) = hold_the_service(&log) {
        log.write(&format!("the service stopped on an error: {e}"));
    }
}

fn hold_the_service(log: &Log) -> ServiceResult<()> {
    let order = STOP_ORDER.get_or_init(StopOrder::new).clone();

    let on_request = {
        let order = order.clone();
        move |control| match control {
            // Three ways of asking for the same thing: a person stopping
            // the service, and the two Windows uses on its way down. The
            // first of those two is the one that matters, and it is the
            // one this service is registered for: it arrives early, with
            // minutes rather than seconds behind it, which is what makes
            // it possible to hand the far computer's screen back before
            // the machine goes.
            ServiceControl::Stop | ServiceControl::Preshutdown | ServiceControl::Shutdown => {
                // Said before anything is stopped, so Windows waits for
                // the tidying up instead of killing it half done.
                if let Some(handle) = ANNOUNCING.get() {
                    let _ = handle.set_service_status(stopping());
                }
                order.ask_for_a_stop();
                ServiceControlHandlerResult::NoError
            }
            // Windows sometimes asks for the current state: answering is
            // what tells a live service from a stuck one.
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let handle = service_control_handler::register(NAME, on_request)?;
    let _ = ANNOUNCING.set(handle);
    handle.set_service_status(announcement(
        ServiceState::Running,
        ServiceExitCode::Win32(0),
    ))?;
    // The build opens the log: a fault read against the wrong version of
    // the product is a fault chased for nothing.
    log.write(&format!("service started, {}", zyr_proto::version_line()));

    // The rules are laid here rather than only when the service is
    // registered. A machine where the service was installed before this
    // existed would otherwise never get its rules, and would show,
    // forever, a network on which nobody else appears.
    if let Ok(program) = std::env::current_exe() {
        lay_the_firewall(&program, Some(log));
    }
    // Same reason: a machine whose service was registered before this
    // existed would ask for administrator rights every time ZyrDesk was
    // opened, and nothing would ever put that right on its own.
    let_the_person_start_and_stop_it(Some(log));
    // And the same reason a third time, which is the one that cost a
    // machine its virtual screen for good: laid only where the service is
    // registered, it never arrived on a computer registered before it
    // existed, and nothing said so. Asked for here as well, where it does
    // nothing at all when the screen is already there.
    crate::screen::put_in_place(Some(log));
    // Here and only here: no session can be running at the start of the
    // service, so an arrangement of screens the engine still owes back is
    // one a run that never finished left behind. The one kind it can
    // never honour is dropped now, before the engine is started and tries
    // it again.
    crate::screen::forget_what_cannot_be_put_back(log);
    // And a fourth time, for the same reason again, which this one has
    // already cost: laid only where the service is registered, the
    // policy that lets Ctrl+Alt+Suppr be pressed never reached a computer
    // registered before it existed, and Windows says nothing at all when
    // it refuses a press.
    crate::attention::let_it_be_pressed(Some(log));
    // A silence left behind by a run of this service that never got to
    // finish: the machine was switched off, or the service fell over,
    // with a session in progress. Only remembered here; the watch gives
    // the sound back on its first turn, since at this moment there may
    // still be nobody signed in.
    crate::speakers::pick_up_where_it_was_left(log);
    say_how_the_networks_are_classed(log);

    let end = supervisor::run(&order, log);
    // Whatever took the service down, including Windows on its way
    // out: the speakers were only ever quiet for a session, and there
    // is no longer one.
    crate::speakers::keep_in_step(false, false, log);
    log.write(&format!("service stopped: {}", reason(end)));

    // A service that gives up has to tell Windows so, rather than
    // leaving without a word: the services console would otherwise show
    // it stopped for no reason.
    let exit = match end {
        End::Asked | End::WindowsShutdown => ServiceExitCode::Win32(0),
        End::NoRuntime => ServiceExitCode::ServiceSpecific(1),
    };
    handle.set_service_status(announcement(ServiceState::Stopped, exit))?;
    Ok(())
}

fn reason(end: End) -> &'static str {
    match end {
        End::Asked => "stop asked for",
        End::WindowsShutdown => "Windows shutting down",
        End::NoRuntime => "nothing to run the service on",
    }
}

fn announcement(state: ServiceState, exit: ServiceExitCode) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: ServiceControlAccept::STOP
            | ServiceControlAccept::PRESHUTDOWN
            | ServiceControlAccept::SHUTDOWN,
        exit_code: exit,
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}

/// What is announced the moment a stop is asked for.
///
/// Nothing is accepted any more, the stop having begun, and the delay is
/// said out loud: Windows waits for what it was told to wait for and
/// kills whatever is still going after it.
fn stopping() -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StopPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: STOPPING_TAKES,
        process_id: None,
    }
}

/// Where the service writes what it does.
///
/// Reachable from outside this file because the short errands the service
/// starts in the session on screen write into it too: they are the same
/// program and what they have to say belongs in the same journal.
pub fn log_path() -> PathBuf {
    paths::logs_dir().join("service.log")
}

/// Code Windows returns for a service it does not know.
const UNKNOWN_SERVICE: i32 = 1060;

/// What installing actually did.
pub enum Installed {
    /// Windows did not know the service and now does.
    Registered,
    /// It already knew it, and now points at this program.
    Updated,
}

/// How the service is described to Windows.
///
/// `at_boot` is the one thing that ever changes: whether Windows starts
/// it on its own, or waits to be asked. Everything else is fixed.
fn described(at_boot: bool) -> Result<ServiceInfo, std::io::Error> {
    Ok(ServiceInfo {
        name: OsString::from(NAME),
        display_name: OsString::from(DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: if at_boot {
            ServiceStartType::AutoStart
        } else {
            ServiceStartType::OnDemand
        },
        error_control: ServiceErrorControl::Normal,
        executable_path: std::env::current_exe()?,
        launch_arguments: vec![OsString::from(SERVICE_ARGUMENT)],
        dependencies: vec![],
        // Default account: LocalSystem, the only one able to reach the
        // secure desktop and to survive session changes.
        account_name: None,
        account_password: None,
    })
}

/// Registers the service with Windows.
///
/// Run again on a machine that already knows the service, it updates
/// where the registration points instead of failing: the program moves
/// when the project folder does, and Windows has to follow.
///
/// It is registered waiting to be asked, not starting on its own. The
/// interface starts it when it opens and stops it when it is quit, so
/// that nothing of this product runs while nobody is using it. Being
/// reachable before anybody has signed in is worth having and is a
/// deliberate choice, made from the settings screen, which is what turns
/// this the other way round.
pub fn install() -> Result<Installed, Box<dyn std::error::Error>> {
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    let description = described(false)?;
    let program = description.executable_path.clone();
    let installed = match manager.open_service(NAME, ServiceAccess::CHANGE_CONFIG) {
        Ok(service) => {
            // What was already decided about starting with Windows is
            // kept: installing again is an update, not a change of mind.
            let carried = described(starts_with_windows().unwrap_or(false))?;
            service.change_config(&carried)?;
            service.set_description(DESCRIPTION)?;
            Installed::Updated
        }
        Err(e) if reported(&e) == Some(UNKNOWN_SERVICE) => {
            let service = manager.create_service(&description, ServiceAccess::CHANGE_CONFIG)?;
            service.set_description(DESCRIPTION)?;
            Installed::Registered
        }
        Err(e) => return Err(e.into()),
    };

    let log = Log::open(&log_path()).ok();
    lay_the_firewall(&program, log.as_ref());
    let_the_person_start_and_stop_it(log.as_ref());
    // Here and nowhere else: laying a driver down needs administrator
    // rights, which are already in hand at this one moment, and needs
    // nobody to be watching a session, which is true of this one moment
    // too. It never fails the installation, since a computer without a
    // virtual screen is a computer that works, only less sharply.
    crate::screen::put_in_place(log.as_ref());
    crate::attention::let_it_be_pressed(log.as_ref());
    Ok(installed)
}

/// Lets whoever is signed in start and stop this service.
///
/// Without it, quitting ZyrDesk and opening it again would ask Windows
/// for administrator rights every single time, which is not a product
/// anybody wants to use. Starting and stopping a service is not a way
/// into anything: what would be one, changing where the service points,
/// is deliberately left out of this and stays with the administrators.
///
/// Said once, when the service is registered, since that is the one
/// moment those rights are already in hand.
fn let_the_person_start_and_stop_it(log: Option<&Log>) {
    use std::os::windows::process::CommandExt;

    /// Windows' own wording for it. The three entries are the system,
    /// the administrators, and whoever is signed in; the last one is the
    /// only one this adds anything to, `RP` for starting and `WP` for
    /// stopping.
    const WHO_MAY: &str = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)\
        (A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)\
        (A;;CCLCSWRPWPDTLOCRRC;;;IU)";

    let said = std::process::Command::new("sc")
        .args(["sdset", NAME, WHO_MAY])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let Some(log) = log else {
        return;
    };
    log.write(&match said {
        Ok(said) if said.status.success() => {
            "the signed-in person may start and stop the service".to_string()
        }
        Ok(said) => format!(
            "the signed-in person may not start or stop the service: {}",
            String::from_utf8_lossy(&said.stdout)
                .trim()
                .replace('\n', " ")
        ),
        Err(e) => format!("service rights untouched: {e}"),
    });
}

/// Whether Windows starts the service on its own.
pub fn starts_with_windows() -> Result<bool, windows_service::Error> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(NAME, ServiceAccess::QUERY_CONFIG)?;
    Ok(service.query_config()?.start_type == ServiceStartType::AutoStart)
}

/// Decides whether it does.
///
/// Asked of the service and not of the interface: the service runs as
/// the system, which is the one identity allowed to change this without
/// anybody being asked for administrator rights.
pub fn start_with_windows(on: bool) -> Result<(), Box<dyn std::error::Error>> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(NAME, ServiceAccess::CHANGE_CONFIG)?;
    service.change_config(&described(on)?)?;
    Ok(())
}

/// Registers the service, points it at this program, and starts it.
///
/// One move rather than three because it is asked for from the
/// interface, and each move on its own would mean another elevation
/// prompt. A service already installed is stopped first: it may point at
/// another copy of this program, and starting it again is the only way
/// it picks this one up.
pub fn set_up() -> Result<Installed, Box<dyn std::error::Error>> {
    let installed = install()?;
    if matches!(installed, Installed::Updated) {
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
        let service =
            manager.open_service(NAME, ServiceAccess::STOP | ServiceAccess::QUERY_STATUS)?;
        let _ = service.stop();
        wait_until_stopped(&service);
    }
    start()?;
    Ok(installed)
}

/// Keeps a console window from flashing up behind the interface.
///
/// Everything this file asks of Windows goes through a program of its
/// own, and every one of them would otherwise show a black window on
/// somebody's screen for a fraction of a second.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// What the service listens on, and what Windows calls each rule.
///
/// Three ports, not one. The tunnel carries everything a session needs.
/// The other two are how two ZyrDesk find each other without anybody
/// reading an address out loud: mDNS asks a whole network at once, and
/// the third is where this computer answers a call made to it directly,
/// for the many networks that quietly drop a multicast between a wired
/// card and a wireless one. Leaving either of the last two closed does
/// not break a session, it stops the other computer from ever appearing,
/// which looks exactly like a product that does not work.
const OPENINGS: [(&str, u16); 3] = [
    ("ZyrDesk (tunnel)", zyr_proto::net::TUNNEL_PORT),
    ("ZyrDesk (réseau local)", zyr_lan::PORT),
    ("ZyrDesk (voisinage)", zyr_lan::CALLING_PORT),
];

/// Lets the outside reach the service, through the Windows firewall.
///
/// Each rule is bound to this program alone, so nothing else on the
/// machine gains anything by it.
///
/// Written again at every start, and not merely put back when missing. A
/// rule that exists is not a rule that lets this program through: it may
/// name a copy of the service that has since been moved, or a port from
/// a version that is no longer this one. Either way it fails without a
/// word, the machine shows nobody on its network, and nothing anywhere
/// says why. Removing it and writing it again costs two calls at start
/// and leaves no room for that.
///
/// A failure here is written down and not raised: a machine whose
/// firewall is managed by someone else is not a machine that should
/// refuse to run. What it costs, if it comes to that, is said in the
/// journal.
fn lay_the_firewall(program: &std::path::Path, log: Option<&Log>) {
    if let Some(log) = log {
        log.write(&format!("firewall rules laid for {}", program.display()));
    }
    for (rule, port) in OPENINGS {
        let _ = netsh(&["delete", "rule", &format!("name={rule}")]);
        told(log, rule, port, add_rule(rule, port, program));
    }
}

/// Says how Windows classes each network this computer is on.
///
/// The one thing that decides whether two ZyrDesk ever find each other
/// and that nothing in the product could see: on a network classed
/// public, Windows cuts discovery whatever the firewall rules say, and a
/// laptop on Wi-Fi inherits that classing without being asked. Written
/// down here so a journal answers the question instead of costing
/// another evening and another command.
///
/// On a thread of its own, and never waited for: this asks Windows
/// through a program of its own, which is slow to start and has no
/// business holding up a service.
fn say_how_the_networks_are_classed(log: &Log) {
    let log = log.clone();
    std::thread::spawn(move || {
        let said = asked_of_windows(
            "Get-NetConnectionProfile | ForEach-Object { $_.InterfaceAlias + ' : ' + $_.NetworkCategory }",
        )
        .unwrap_or_default();
        let classed: Vec<&str> = said
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        // Une absence de réponse est elle-même une réponse : sans cette
        // ligne, on ne saurait pas distinguer « Windows n'a rien dit »
        // de « le service est trop vieux pour le demander ».
        if classed.is_empty() {
            log.write("Windows did not say how it classes these networks");
            return;
        }
        for line in classed {
            log.write(&format!("network {line}"));
        }
    });
}

/// Runs one question through Windows' own shell, quietly.
fn asked_of_windows(question: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;

    let said = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", question])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !said.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&said.stdout).into_owned())
}

/// Adds one rule, bound to this program alone.
fn add_rule(rule: &str, port: u16, program: &std::path::Path) -> std::io::Result<bool> {
    netsh(&[
        "add",
        "rule",
        &format!("name={rule}"),
        "dir=in",
        "action=allow",
        "protocol=UDP",
        &format!("localport={port}"),
        &format!("program={}", program.display()),
        &format!("description={DESCRIPTION}"),
    ])
}

/// Writes down what became of one rule, when there is anywhere to write.
fn told(log: Option<&Log>, rule: &str, port: u16, outcome: std::io::Result<bool>) {
    let Some(log) = log else {
        return;
    };
    log.write(&match outcome {
        Ok(true) => format!("firewall opened for {rule} on UDP {port}"),
        Ok(false) => format!("firewall rule {rule} refused, UDP {port} stays closed"),
        Err(e) => format!("firewall untouched for {rule}: {e}"),
    });
}

/// Runs one netsh command, quietly. `false` when netsh said no.
fn netsh(arguments: &[&str]) -> std::io::Result<bool> {
    use std::os::windows::process::CommandExt;

    std::process::Command::new("netsh")
        .args(["advfirewall", "firewall"])
        .args(arguments)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|said| said.status.success())
}

/// Code the system itself gave, when it gave one.
fn reported(error: &windows_service::Error) -> Option<i32> {
    match error {
        windows_service::Error::Winapi(e) => e.raw_os_error(),
        _ => None,
    }
}

/// Removes the service. It disappears once stopped.
pub fn uninstall() -> ServiceResult<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(
        NAME,
        ServiceAccess::STOP | ServiceAccess::QUERY_STATUS | ServiceAccess::DELETE,
    )?;
    // A running service is only removed once stopped; a failure here
    // means it was already stopped, which suits us.
    let _ = service.stop();
    wait_until_stopped(&service);
    service.delete()?;

    // What was opened is closed again: a rule left behind would point at
    // a program that no longer runs.
    for (rule, _) in OPENINGS {
        let _ = netsh(&["delete", "rule", &format!("name={rule}")]);
    }

    // And the door this product opened on Ctrl+Alt+Suppr is closed with
    // them. A machine that no longer runs ZyrDesk has no reason to go on
    // letting a service press what Windows keeps for itself.
    crate::attention::forget_it();

    // After the service is gone and not before: the driver cannot leave
    // Windows' store while anything is still using its device, and the
    // engine is what uses it.
    crate::screen::take_away(Log::open(&log_path()).ok().as_ref());
    Ok(())
}

/// Waits for the service to have really stopped.
///
/// Asking for a stop only starts one. As long as a copy is still
/// running, Windows merely notes the removal and keeps the program file
/// locked: an uninstaller would then leave both of them behind.
fn wait_until_stopped(service: &Service) {
    let deadline = Instant::now() + STOP_DELAY;
    while Instant::now() < deadline {
        match service.query_status() {
            Ok(status) if status.current_state == ServiceState::Stopped => return,
            // An unreadable state is not worth waiting on: the removal
            // that follows will say what it could not do.
            Err(_) => return,
            _ => std::thread::sleep(STOP_STEP),
        }
    }
}

pub fn start() -> ServiceResult<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(NAME, ServiceAccess::START)?;
    service.start::<&str>(&[])
}

/// Code Windows returns when asked to stop a service that is not running.
const NOT_ACTIVE: i32 = 1062;

/// What stopping actually did.
pub enum Stopped {
    /// It was running, and now it is not.
    WasRunning,
    /// It already was not: nothing to do.
    AlreadyStopped,
}

/// Stops the service.
///
/// Asked of a service that already is not running, this is not a
/// failure: that is the state being asked for, already reached.
pub fn stop() -> ServiceResult<Stopped> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(NAME, ServiceAccess::STOP)?;
    match service.stop() {
        Ok(_) => Ok(Stopped::WasRunning),
        Err(e) if reported(&e) == Some(NOT_ACTIVE) => Ok(Stopped::AlreadyStopped),
        Err(e) => Err(e),
    }
}

/// State of the service, as Windows reports it.
pub fn state() -> ServiceResult<ServiceState> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;
    let service = manager.open_service(NAME, ServiceAccess::QUERY_STATUS)?;
    Ok(service.query_status()?.current_state)
}
