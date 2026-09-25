//! This computer's Wi-Fi, asked to put the session first for as long as
//! one is open.
//!
//! A Wi-Fi card connected to its network still leaves it now and then to
//! look at the others around, and while it listens elsewhere nothing of
//! the session reaches it: what it missed arrives all at once when it
//! comes back. On the twenty-fifth of September a laptop watching over
//! the Internet went deaf for 130 ms every 1.7 s, thirteen times in a
//! row, which is a window caught on something every other second while
//! it is being dragged.
//!
//! Windows has two switches for it, card by card: no looking around while
//! connected, and the streaming mode, which tells the driver to put
//! latency before everything else, saving power included. Moonlight
//! turned the second on for as long as it played. Both are turned on
//! here, on either side of a session: a computer sending over Wi-Fi goes
//! deaf in the same way.
//!
//! Both are votes. Windows withdraws them by itself when the handle that
//! cast them is closed, however this service comes to end, and forgets
//! them whenever a card disconnects: a card that connects again during a
//! session is asked again as soon as it has.
//!
//! The service asks rather than the player: Windows may refuse these
//! switches to a program that is not an administrator, and the service
//! runs as the system. The Wi-Fi library is looked for at run time, since
//! a Windows Server without its wireless service has none, and a service
//! linked to it would not even start there.

use zyr_proto::log::Log;

/// What this module's lines are filed under.
const TAG: &str = "wifi";

/// The Wi-Fi putting the session first for as long as this is held.
///
/// Handed to whatever owns a session, so that it ends exactly when the
/// session does, whichever way it ends.
pub struct Favouring;

impl Drop for Favouring {
    fn drop(&mut self) {
        mechanism::ended();
    }
}

/// A session is opening here: until it ends, every Wi-Fi card connected
/// puts it first.
pub fn favour_latency(log: &Log) -> Favouring {
    mechanism::opened(&log.about(TAG));
    Favouring
}

#[cfg(windows)]
mod mechanism {
    use std::ffi::{CStr, c_void};
    use std::io;
    use std::ptr;
    use std::sync::OnceLock;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::thread;
    use std::time::Instant;

    use windows_sys::Win32::Foundation::{ERROR_SUCCESS, FreeLibrary, HANDLE, HMODULE};
    use windows_sys::Win32::NetworkManagement::WiFi::{
        L2_NOTIFICATION_DATA, WLAN_API_VERSION_2_0, WLAN_INTERFACE_INFO, WLAN_INTERFACE_INFO_LIST,
        WLAN_INTF_OPCODE, WLAN_NOTIFICATION_CALLBACK, WLAN_NOTIFICATION_SOURCE_ACM,
        WLAN_NOTIFICATION_SOURCE_NONE, WLAN_NOTIFICATION_SOURCES, WLAN_OPCODE_VALUE_TYPE,
        wlan_interface_state_connected, wlan_intf_opcode_background_scan_enabled,
        wlan_intf_opcode_media_streaming_mode, wlan_notification_acm_connection_complete,
    };
    use windows_sys::Win32::System::LibraryLoader::{
        GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
    };
    use windows_sys::core::{BOOL, GUID, w};
    use zyr_proto::log::Log;

    use crate::gateway::with_its_code;

    /// What the thread that speaks to the Wi-Fi hears.
    enum Told {
        Opened,
        Ended,
        /// That card finished connecting, and has forgotten what it was
        /// asked.
        Connected(GUID),
    }

    /// Where that thread hears from. The first session starts it, and it
    /// then waits for the next one for as long as the service runs.
    static MAILBOX: OnceLock<Sender<Told>> = OnceLock::new();

    pub fn opened(log: &Log) {
        let mailbox = MAILBOX.get_or_init(|| {
            let (mailbox, heard) = mpsc::channel();
            let listening = log.clone();
            if let Err(e) = thread::Builder::new()
                .name("zyrdeskd-wifi".to_string())
                .spawn(move || keep(&heard, &listening))
            {
                log.write(&format!("nothing can speak to the Wi-Fi here: {e}"));
            }
            mailbox
        });
        let _ = mailbox.send(Told::Opened);
    }

    pub fn ended() {
        if let Some(mailbox) = MAILBOX.get() {
            let _ = mailbox.send(Told::Ended);
        }
    }

    /// Asks the cards when the first session opens, hands them back when
    /// the last one ends, and asks a card again whenever it connects in
    /// between.
    ///
    /// A thread of its own, because Windows takes about a second over
    /// each switch of each card: the driver is told, and answers.
    fn keep(heard: &Receiver<Told>, log: &Log) {
        let mut open = 0usize;
        let mut asking: Option<Asking> = None;
        for told in heard {
            match told {
                Told::Opened => {
                    open += 1;
                    if open == 1 {
                        asking = Asking::start(log);
                    }
                }
                Told::Ended => {
                    open = open.saturating_sub(1);
                    if open == 0 && asking.take().is_some() {
                        log.debug(|| "the Wi-Fi cards are handed back to Windows".to_string());
                    }
                }
                Told::Connected(card) => {
                    if let Some(asking) = &asking {
                        asking.ask(Some(card), log);
                    }
                }
            }
        }
    }

    /// What Windows calls, on a thread of its own, when a card's
    /// connection moves. It only passes the news on: calling back into
    /// the Wi-Fi service from here can hang.
    unsafe extern "system" fn heard(data: *mut L2_NOTIFICATION_DATA, _context: *mut c_void) {
        // SAFETY: a notification Windows keeps valid for the call.
        let Some(data) = (unsafe { data.as_ref() }) else {
            return;
        };
        if data.NotificationSource == WLAN_NOTIFICATION_SOURCE_ACM
            && data.NotificationCode == wlan_notification_acm_connection_complete as u32
            && let Some(mailbox) = MAILBOX.get()
        {
            let _ = mailbox.send(Told::Connected(data.InterfaceGuid));
        }
    }

    /// A handle on the Wi-Fi service, for as long as a session is open:
    /// what was asked through it holds while it stays open.
    struct Asking {
        wlan: Wlan,
        handle: HANDLE,
    }

    impl Asking {
        /// Opens the handle, listens for cards that connect, and asks
        /// every card connected now. None, and the journal says why, when
        /// nothing can be asked.
        fn start(log: &Log) -> Option<Self> {
            let wlan = Wlan::load()
                .inspect_err(|e| {
                    log.write(&format!(
                        "the Wi-Fi could not be asked to put the session first: {e}"
                    ));
                })
                .ok()?;
            let mut negotiated = 0;
            let mut handle = ptr::null_mut();
            // SAFETY: the function's own type; the version of Windows
            // Vista and after, and places for what the service answers.
            let opened = unsafe {
                (wlan.open_handle)(
                    WLAN_API_VERSION_2_0,
                    ptr::null(),
                    &mut negotiated,
                    &mut handle,
                )
            };
            if opened != ERROR_SUCCESS {
                log.write(&format!(
                    "the Wi-Fi could not be asked to put the session first: the Wi-Fi service \
                     did not answer ({})",
                    refused("WlanOpenHandle", opened)
                ));
                return None;
            }
            let asking = Self { wlan, handle };
            // SAFETY: the function's own type; the handle just opened, and
            // a function that lasts as long as the program.
            let listening = unsafe {
                (asking.wlan.register_notification)(
                    handle,
                    WLAN_NOTIFICATION_SOURCE_ACM,
                    1,
                    Some(heard),
                    ptr::null(),
                    ptr::null(),
                    ptr::null_mut(),
                )
            };
            if listening != ERROR_SUCCESS {
                log.write(&format!(
                    "a Wi-Fi card that connects again during the session will not be asked \
                     again ({})",
                    refused("WlanRegisterNotification", listening)
                ));
            }
            asking.ask(None, log);
            Some(asking)
        }

        /// Asks every card connected now, or only `only` if it is.
        fn ask(&self, only: Option<GUID>, log: &Log) {
            let cards = match self.cards() {
                Ok(cards) => cards,
                Err(e) => {
                    log.write(&format!("the Wi-Fi cards could not be listed ({e})"));
                    return;
                }
            };
            let mut connected = cards
                .iter()
                .filter(|card| card.isState == wlan_interface_state_connected)
                .filter(|card| only.is_none_or(|only| same(&only, &card.InterfaceGuid)))
                .peekable();
            if only.is_none() && connected.peek().is_none() {
                log.write("no Wi-Fi card is connected: the session goes through none");
            }
            for card in connected {
                let asked = Instant::now();
                let looking = self.switch(
                    &card.InterfaceGuid,
                    wlan_intf_opcode_background_scan_enabled,
                    false,
                );
                let streaming = self.switch(
                    &card.InterfaceGuid,
                    wlan_intf_opcode_media_streaming_mode,
                    true,
                );
                log.write(&format!(
                    "the Wi-Fi card {} {} and {} ({}, asked in {} ms)",
                    name(card),
                    match looking {
                        Ok(false) => "looks for no other network while connected".to_string(),
                        Ok(true) =>
                            "still looks for other networks, its driver keeps to it".to_string(),
                        Err(e) =>
                            format!("could not be kept from looking for other networks ({e})"),
                    },
                    match streaming {
                        Ok(true) => "is in streaming mode".to_string(),
                        Ok(false) =>
                            "is not in streaming mode, its driver keeps out of it".to_string(),
                        Err(e) => format!("could not be put in streaming mode ({e})"),
                    },
                    if only.is_some() {
                        "it connected again"
                    } else {
                        "for the session"
                    },
                    asked.elapsed().as_millis()
                ));
            }
        }

        /// The cards Windows knows, connected or not.
        fn cards(&self) -> Result<Vec<WLAN_INTERFACE_INFO>, String> {
            let mut list: *mut WLAN_INTERFACE_INFO_LIST = ptr::null_mut();
            // SAFETY: the function's own type; the open handle, and a place
            // for the list the service makes.
            let listed =
                unsafe { (self.wlan.enum_interfaces)(self.handle, ptr::null(), &mut list) };
            if listed != ERROR_SUCCESS {
                return Err(refused("WlanEnumInterfaces", listed));
            }
            // SAFETY: the list the service made, whose entries follow one
            // another from the one it declares; it is freed once, after
            // they have been copied.
            unsafe {
                let first = (&raw const (*list).InterfaceInfo).cast::<WLAN_INTERFACE_INFO>();
                let cards =
                    std::slice::from_raw_parts(first, (*list).dwNumberOfItems as usize).to_vec();
                (self.wlan.free_memory)(list.cast());
                Ok(cards)
            }
        }

        /// Asks a card to hold one switch `on` or off, then reads what it
        /// holds: a driver may take the question and keep to its own way.
        fn switch(&self, card: &GUID, switch: WLAN_INTF_OPCODE, on: bool) -> Result<bool, String> {
            let asked = BOOL::from(on);
            // SAFETY: the function's own type; the open handle, a card it
            // listed, and the BOOL these two switches take.
            let set = unsafe {
                (self.wlan.set_interface)(
                    self.handle,
                    card,
                    switch,
                    size_of::<BOOL>() as u32,
                    (&raw const asked).cast(),
                    ptr::null(),
                )
            };
            if set != ERROR_SUCCESS {
                return Err(refused("WlanSetInterface", set));
            }
            let mut size = 0u32;
            let mut held: *mut c_void = ptr::null_mut();
            let mut kind: WLAN_OPCODE_VALUE_TYPE = 0;
            // SAFETY: the function's own type; the same handle, card and
            // switch, and places for what the service makes.
            let read = unsafe {
                (self.wlan.query_interface)(
                    self.handle,
                    card,
                    switch,
                    ptr::null(),
                    &mut size,
                    &mut held,
                    &mut kind,
                )
            };
            if read != ERROR_SUCCESS {
                return Err(refused("WlanQueryInterface", read));
            }
            if held.is_null() {
                return Err("WlanQueryInterface answered nothing".to_string());
            }
            // SAFETY: the value the service made, read if it is as large as
            // the BOOL these switches hold, and freed once.
            unsafe {
                let value = (size as usize >= size_of::<BOOL>())
                    .then(|| held.cast::<BOOL>().read_unaligned() != 0);
                (self.wlan.free_memory)(held);
                value.ok_or_else(|| format!("WlanQueryInterface answered {size} bytes"))
            }
        }
    }

    impl Drop for Asking {
        fn drop(&mut self) {
            // SAFETY: the functions' own types, and the handle opened in
            // `start`, not used after. Unlistening waits for a notification
            // being delivered, so none comes once the handle is closed.
            unsafe {
                (self.wlan.register_notification)(
                    self.handle,
                    WLAN_NOTIFICATION_SOURCE_NONE,
                    0,
                    None,
                    ptr::null(),
                    ptr::null(),
                    ptr::null_mut(),
                );
                (self.wlan.close_handle)(self.handle, ptr::null());
            }
        }
    }

    type OpenHandle = unsafe extern "system" fn(u32, *const c_void, *mut u32, *mut HANDLE) -> u32;
    type CloseHandle = unsafe extern "system" fn(HANDLE, *const c_void) -> u32;
    type EnumInterfaces =
        unsafe extern "system" fn(HANDLE, *const c_void, *mut *mut WLAN_INTERFACE_INFO_LIST) -> u32;
    type FreeMemory = unsafe extern "system" fn(*const c_void);
    type SetInterface = unsafe extern "system" fn(
        HANDLE,
        *const GUID,
        WLAN_INTF_OPCODE,
        u32,
        *const c_void,
        *const c_void,
    ) -> u32;
    type QueryInterface = unsafe extern "system" fn(
        HANDLE,
        *const GUID,
        WLAN_INTF_OPCODE,
        *const c_void,
        *mut u32,
        *mut *mut c_void,
        *mut WLAN_OPCODE_VALUE_TYPE,
    ) -> u32;
    type RegisterNotification = unsafe extern "system" fn(
        HANDLE,
        WLAN_NOTIFICATION_SOURCES,
        BOOL,
        WLAN_NOTIFICATION_CALLBACK,
        *const c_void,
        *const c_void,
        *mut u32,
    ) -> u32;

    /// `wlanapi.dll`, loaded, and what this takes from it.
    struct Wlan {
        library: HMODULE,
        open_handle: OpenHandle,
        close_handle: CloseHandle,
        enum_interfaces: EnumInterfaces,
        free_memory: FreeMemory,
        set_interface: SetInterface,
        query_interface: QueryInterface,
        register_notification: RegisterNotification,
    }

    impl Wlan {
        fn load() -> Result<Self, String> {
            // SAFETY: a library of the system's own, looked for in System32
            // alone.
            let library = unsafe {
                LoadLibraryExW(
                    w!("wlanapi.dll"),
                    ptr::null_mut(),
                    LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            };
            if library.is_null() {
                return Err(format!(
                    "this computer has no Wi-Fi library ({})",
                    with_its_code(&io::Error::last_os_error())
                ));
            }
            // SAFETY: each function's own type, as wlanapi.h declares it.
            let found = unsafe {
                Ok::<_, String>(Self {
                    library,
                    open_handle: function(library, c"WlanOpenHandle")?,
                    close_handle: function(library, c"WlanCloseHandle")?,
                    enum_interfaces: function(library, c"WlanEnumInterfaces")?,
                    free_memory: function(library, c"WlanFreeMemory")?,
                    set_interface: function(library, c"WlanSetInterface")?,
                    query_interface: function(library, c"WlanQueryInterface")?,
                    register_notification: function(library, c"WlanRegisterNotification")?,
                })
            };
            if found.is_err() {
                // SAFETY: loaded above, and nothing of it is used.
                unsafe { FreeLibrary(library) };
            }
            found
        }
    }

    impl Drop for Wlan {
        fn drop(&mut self) {
            // SAFETY: loaded in `load`, and not used after.
            unsafe { FreeLibrary(self.library) };
        }
    }

    /// The function `library` exports under `name`, as `F`.
    ///
    /// # Safety
    ///
    /// `F` must be that function's own type.
    unsafe fn function<F: Copy>(library: HMODULE, name: &CStr) -> Result<F, String> {
        const { assert!(size_of::<F>() == size_of::<usize>()) };
        // SAFETY: a library loaded, and a terminated name.
        let found = unsafe { GetProcAddress(library, name.as_ptr().cast()) }
            .ok_or_else(|| format!("wlanapi.dll has no {}", name.to_string_lossy()))?;
        // SAFETY: an exported function's address, of the type the caller
        // vouches for, which is as large as an address.
        Ok(unsafe { std::mem::transmute_copy(&found) })
    }

    /// A card as its driver names it.
    fn name(card: &WLAN_INTERFACE_INFO) -> String {
        let named = &card.strInterfaceDescription;
        let end = named
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(named.len());
        String::from_utf16_lossy(&named[..end])
    }

    fn same(one: &GUID, other: &GUID) -> bool {
        (one.data1, one.data2, one.data3, one.data4)
            == (other.data1, other.data2, other.data3, other.data4)
    }

    /// What the Wi-Fi service refused, with its own number for it.
    fn refused(call: &str, code: u32) -> String {
        format!(
            "{call}: {}",
            with_its_code(&io::Error::from_raw_os_error(code as i32))
        )
    }
}

/// Outside Windows there is no service, and no card to ask.
#[cfg(not(windows))]
mod mechanism {
    use zyr_proto::log::Log;

    pub fn opened(_log: &Log) {}

    pub fn ended() {}
}
