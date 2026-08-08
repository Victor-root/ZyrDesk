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

use zyr_proto::paths;

use crate::log::Log;
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

/// A service runs in exactly one copy, so what it shares with its
/// handler is legitimately global.
static STOP_ORDER: std::sync::OnceLock<StopOrder> = std::sync::OnceLock::new();

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
            ServiceControl::Stop | ServiceControl::Shutdown => {
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
    handle.set_service_status(announcement(
        ServiceState::Running,
        ServiceExitCode::Win32(0),
    ))?;
    log.write("service started");

    let end = supervisor::run(&order, log);
    log.write(&format!("service stopped: {}", reason(end)));

    // A service that gives up has to tell Windows so, rather than
    // leaving without a word: the services console would otherwise show
    // it stopped for no reason.
    let exit = match end {
        End::Asked | End::WindowsShutdown => ServiceExitCode::Win32(0),
        End::EngineWontStand | End::NothingToStart => ServiceExitCode::ServiceSpecific(1),
    };
    handle.set_service_status(announcement(ServiceState::Stopped, exit))?;
    Ok(())
}

fn reason(end: End) -> &'static str {
    match end {
        End::Asked => "stop asked for",
        End::WindowsShutdown => "Windows shutting down",
        End::EngineWontStand => "the host engine will not stand",
        End::NothingToStart => "no host engine to start",
    }
}

fn announcement(state: ServiceState, exit: ServiceExitCode) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: exit,
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }
}

fn log_path() -> PathBuf {
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

/// Registers the service with Windows, starting automatically.
///
/// Run again on a machine that already knows the service, it updates
/// where the registration points instead of failing: the program moves
/// when the project folder does, and Windows has to follow.
pub fn install() -> Result<Installed, Box<dyn std::error::Error>> {
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )?;

    let description = ServiceInfo {
        name: OsString::from(NAME),
        display_name: OsString::from(DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        // The computer has to be reachable from the moment it powers on,
        // without anyone logging in: that is the whole point.
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: std::env::current_exe()?,
        launch_arguments: vec![OsString::from(SERVICE_ARGUMENT)],
        dependencies: vec![],
        // Default account: LocalSystem, the only one able to reach the
        // secure desktop and to survive session changes.
        account_name: None,
        account_password: None,
    };

    match manager.open_service(NAME, ServiceAccess::CHANGE_CONFIG) {
        Ok(service) => {
            service.change_config(&description)?;
            service.set_description(DESCRIPTION)?;
            Ok(Installed::Updated)
        }
        Err(e) if reported(&e) == Some(UNKNOWN_SERVICE) => {
            let service = manager.create_service(&description, ServiceAccess::CHANGE_CONFIG)?;
            service.set_description(DESCRIPTION)?;
            Ok(Installed::Registered)
        }
        Err(e) => Err(e.into()),
    }
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
