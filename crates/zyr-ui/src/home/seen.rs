//! What the home window shows, gathered for it and kept up to date.
//!
//! Everything comes from somewhere else: the service says what this
//! computer is doing and which computers it knows, Windows says what this
//! computer is and who sits at it. The window draws what is here and
//! goes looking for nothing itself; a session can be opened from the
//! other end, the service can stop, a wallpaper can change, and none of
//! that goes through a click, so this is asked again every few seconds.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use zyr_control::{Account, Contact, Device, Lent, Past, Peer};
use zyr_draw::Picture;
use zyr_system::{Connectivity, Machine};

use crate::session::Ongoing;
use crate::shell::app::App;
use crate::shell::desk::{Standing, Watcher};
use crate::shell::settings::Settings;
use crate::shell::shortcuts::Doing;

/// What is asked for again this often.
const REFRESH: Duration = Duration::from_secs(3);

/// What the home window shows.
#[derive(Default, PartialEq)]
pub struct Seen {
    /// How many times it was changed: what was drawn from an older one
    /// is out of date.
    pub generation: u64,
    /// This computer as the service says it, or why it cannot.
    pub standing: Option<Standing>,
    /// This computer as Windows says it, read once.
    pub machine: Option<Machine>,
    /// Its wallpaper, read at the size its card shows it.
    pub wallpaper: Option<Wallpaper>,
    /// Who sits at it, read once.
    pub person: Option<String>,
    pub connectivity: Option<Connectivity>,
    /// Where it answers on its networks.
    pub addresses: Vec<Ipv4Addr>,
    /// Whether FFmpeg is where the player loads it from.
    pub ffmpeg_here: Option<bool>,
    pub peers: Vec<Peer>,
    /// The other computers' wallpapers, by fingerprint, as their last
    /// session with this one brought them.
    pub far_wallpapers: HashMap<String, FarWallpaper>,
    pub sessions: Vec<Ongoing>,
    /// The computers controlling this one right now.
    pub watching: Vec<Watcher>,
    /// The sessions that took place, the latest first.
    pub history: Vec<Past>,
    pub settings: Option<Settings>,
    /// The account, when the service answers: the link if there is one,
    /// and the devices on it.
    pub account: Option<AccountState>,
    /// The shortcuts, as they are engraved on the keyboard plugged in.
    pub shortcuts: Vec<(Doing, Option<String>)>,
    /// This window's build, and the folder of the journals.
    pub version: String,
    pub folder: String,
}

/// This computer's account, as the service holds it.
#[derive(Default, PartialEq)]
pub struct AccountState {
    pub link: Option<Account>,
    /// The devices of the account, this computer among them, as the
    /// server last said them.
    pub devices: Vec<Device>,
    /// Its contacts and the requests either way, and what it lends them.
    pub contacts: Vec<Contact>,
    pub lent: Vec<Lent>,
}

/// This computer's wallpaper, as its card shows it.
pub struct Wallpaper {
    /// The picture, brought down to the card, when there is one that
    /// could be read.
    pub picture: Option<Arc<Picture>>,
    /// The colour of the desktop, shown when there is no picture.
    pub colour: [u8; 3],
    /// What it was read from, and at what size: read again only when one
    /// of them changes.
    from: Option<(PathBuf, SystemTime)>,
    size: (u32, u32),
}

/// Another computer's wallpaper, as its card shows it.
#[derive(Clone)]
pub struct FarWallpaper {
    /// When it came, as the service says: read again when that changes.
    stamp: u64,
    pub picture: Arc<Picture>,
}

impl PartialEq for FarWallpaper {
    fn eq(&self, other: &Self) -> bool {
        self.stamp == other.stamp && Arc::ptr_eq(&self.picture, &other.picture)
    }
}

impl PartialEq for Wallpaper {
    fn eq(&self, other: &Self) -> bool {
        self.colour == other.colour
            && self.from == other.from
            && self.size == other.size
            && self.picture.as_ref().map(Arc::as_ptr) == other.picture.as_ref().map(Arc::as_ptr)
    }
}

impl Seen {
    /// One session at a time from this computer: two pictures in one
    /// window cannot both be driven.
    pub fn busy(&self, opening: bool) -> bool {
        opening || !self.sessions.is_empty()
    }

    /// This computer's own device on the account, when there is one.
    pub fn this_device(&self) -> Option<&Device> {
        self.account
            .as_ref()
            .and_then(|account| account.devices.iter().find(|device| device.this))
    }

    /// The name this computer goes by: the account's when it has one,
    /// which is the one renaming changes, the machine's otherwise.
    pub fn this_name(&self) -> String {
        self.this_device()
            .map(|device| device.name.clone())
            .or_else(|| self.standing.as_ref().map(|said| said.name.clone()))
            .unwrap_or_else(zyr_proto::machine::name)
    }

    /// The person at this computer, and the server of their account: its
    /// name and server with an account, the name Windows knows them by
    /// and nothing more without one.
    pub fn with_person(&self) -> (String, Option<String>) {
        if let Some(link) = self
            .account
            .as_ref()
            .and_then(|account| account.link.as_ref())
        {
            let server = if link.name.is_empty() {
                link.server.clone()
            } else {
                link.name.clone()
            };
            return (link.username.clone(), Some(server));
        }
        (
            self.person.clone().unwrap_or_else(zyr_proto::machine::name),
            None,
        )
    }

    /// The computer of this fingerprint, if it is still on the list.
    pub fn peer(&self, fingerprint: &str) -> Option<&Peer> {
        self.peers
            .iter()
            .find(|peer| peer.fingerprint.to_string() == fingerprint)
    }

    /// What a computer is called today, by its fingerprint: on the
    /// screen, or on the account, or, for one known by neither any more,
    /// where it was.
    pub fn name_of(&self, peer: &zyr_proto::fingerprint::Fingerprint, address: &str) -> String {
        self.peers
            .iter()
            .find(|known| known.fingerprint == *peer)
            .map(|known| known.name.clone())
            .or_else(|| {
                self.account.as_ref().and_then(|account| {
                    account
                        .devices
                        .iter()
                        .find(|device| device.fingerprint == *peer)
                        .map(|device| device.name.clone())
                })
            })
            .unwrap_or_else(|| address.to_string())
    }

    /// What the account's server knows of a computer: when it was last
    /// seen, for one that is gone.
    pub fn device_of(&self, peer: &Peer) -> Option<&Device> {
        self.account.as_ref().and_then(|account| {
            account
                .devices
                .iter()
                .find(|device| device.fingerprint == peer.fingerprint)
        })
    }
}

/// What the window shows, shared between the thread that draws and the
/// one that asks.
static SEEN: Mutex<Option<Seen>> = Mutex::new(None);

/// Reads what is shown, for the length of `read`.
pub fn with<R>(read: impl FnOnce(&Seen) -> R) -> R {
    let nothing = Seen::default();
    let guard = SEEN.lock().expect("what the home window shows");
    read(guard.as_ref().unwrap_or(&nothing))
}

/// Changes what is shown, for the length of `change`.
fn change<R>(change: impl FnOnce(&mut Seen) -> R) -> R {
    let mut guard = SEEN.lock().expect("what the home window shows");
    let seen = guard.get_or_insert_with(Seen::default);
    seen.generation += 1;
    change(seen)
}

/// Keeps asking for what is shown, for as long as the program runs, and
/// has the window drawn again when something changed.
pub fn watch(app: App) {
    crate::shell::app::spawn(async move {
        // What does not change for the life of the program, asked once,
        // away from the thread that draws: the registry is quick but not
        // free, the person's name may ask the network of a company, and
        // the scenery of the theme takes a moment to read at its size.
        let (machine, person) = crate::shell::app::spawn_blocking(|| {
            zyr_draw::scenery::read_ahead(crate::shell::theme::light());
            (
                zyr_system::this_computer(),
                zyr_system::name_of_the_person(),
            )
        })
        .await
        .unwrap_or_default();
        change(|seen| {
            seen.machine = Some(machine);
            seen.person = person;
            seen.version = crate::shell::desk::build();
            seen.folder = crate::shell::folders::logs_folder();
            seen.shortcuts = crate::shell::shortcuts::engraved();
        });
        loop {
            if again(&app).await {
                crate::home::redraw(&app);
            }
            tokio::time::sleep(REFRESH).await;
        }
    });
}

/// Asks everything again, and says whether anything changed.
///
/// What has not changed is not drawn again: the window often stays open
/// behind a session, and a frame drawn for nothing every three seconds is
/// time taken from the session's picture.
pub async fn again(app: &App) -> bool {
    let standing = crate::shell::desk::standing().await;
    let mut peers = crate::shell::desk::peers().await;
    zyr_session::presence::reachable_first(&mut peers);
    let sessions = crate::session::sessions().await;
    let watching = crate::shell::desk::watching().await;
    let history = crate::shell::desk::history().await;
    let settings = crate::shell::settings::settings(app.clone()).await;
    let account = match crate::shell::desk::account().await {
        Ok(None) => Some(AccountState::default()),
        Ok(link) => Some(AccountState {
            devices: crate::shell::desk::devices().await,
            contacts: crate::shell::desk::contacts().await,
            lent: crate::shell::desk::lent().await,
            link,
        }),
        Err(_) => None,
    };
    let (ffmpeg_here, connectivity, addresses) = crate::shell::app::spawn_blocking(|| {
        (
            crate::shell::folders::ffmpeg_here(),
            zyr_system::connectivity(),
            zyr_proto::machine::addresses()
                .into_iter()
                .map(|card| card.address)
                .collect::<Vec<_>>(),
        )
    })
    .await
    .unwrap_or((false, None, Vec::new()));
    let kept = with(|seen| seen.wallpaper.as_ref().map(Wallpaper::read_from));
    let wallpaper = read_the_wallpaper(kept).await;
    let far_wallpapers = far_wallpapers(&peers).await;

    change(|seen| {
        let mut changed = replace(&mut seen.standing, Some(standing));
        changed |= replace(&mut seen.peers, peers);
        changed |= replace(&mut seen.far_wallpapers, far_wallpapers);
        changed |= replace(&mut seen.sessions, sessions);
        changed |= replace(&mut seen.watching, watching);
        changed |= replace(&mut seen.history, history);
        changed |= replace(&mut seen.settings, Some(settings));
        changed |= replace(&mut seen.account, account);
        changed |= replace(&mut seen.ffmpeg_here, Some(ffmpeg_here));
        changed |= replace(&mut seen.connectivity, connectivity);
        changed |= replace(&mut seen.addresses, addresses);
        if let Some(wallpaper) = wallpaper {
            seen.wallpaper = Some(wallpaper);
            changed = true;
        }
        changed
    })
}

/// The size the other computers' thumbnails come at, which is the size
/// they are read at.
const THUMBNAIL: (u32, u32) = (400, 225);

/// The other computers' wallpapers: those kept already, and those whose
/// computer brought a new one since, read anew. A computer that has none
/// any more loses its own.
async fn far_wallpapers(peers: &[Peer]) -> HashMap<String, FarWallpaper> {
    let mut kept = with(|seen| seen.far_wallpapers.clone());
    let mut gathered = HashMap::new();
    for peer in peers {
        let Some(stamp) = peer.wallpaper else {
            continue;
        };
        let fingerprint = peer.fingerprint.to_string();
        match kept.remove(&fingerprint) {
            Some(far) if far.stamp == stamp => {
                gathered.insert(fingerprint, far);
            }
            _ => {
                let Some(bytes) = crate::shell::desk::far_wallpaper(&fingerprint).await else {
                    continue;
                };
                let picture =
                    crate::shell::app::spawn_blocking(move || Picture::decode(&bytes, THUMBNAIL))
                        .await
                        .ok()
                        .flatten();
                if let Some(picture) = picture {
                    gathered.insert(
                        fingerprint,
                        FarWallpaper {
                            stamp,
                            picture: Arc::new(picture),
                        },
                    );
                }
            }
        }
    }
    gathered
}

/// Puts a new value in place, and says whether it was a different one.
fn replace<T: PartialEq>(field: &mut T, new: T) -> bool {
    if *field == new {
        return false;
    }
    *field = new;
    true
}

/// What a wallpaper was read from: the file and when it last changed, the
/// size it was read at, and the colour of the desktop.
type ReadFrom = (Option<(PathBuf, SystemTime)>, (u32, u32), [u8; 3]);

impl Wallpaper {
    fn read_from(&self) -> ReadFrom {
        (self.from.clone(), self.size, self.colour)
    }
}

/// The room the card shows the wallpaper in, in real pixels, as the
/// drawing last asked for it: the picture is read at that size and no
/// larger.
static ROOM: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Says how large the wallpaper is shown, so that it is read at that
/// size. A different room has it read again at once, rather than at the
/// next round: the card would otherwise show a stand-in for a moment
/// every time the window opens or changes screen.
pub fn wallpaper_shown_at(app: &App, width: u32, height: u32) {
    let room = (u64::from(width) << 32) | u64::from(height);
    if ROOM.swap(room, std::sync::atomic::Ordering::Relaxed) == room {
        return;
    }
    let app = app.clone();
    crate::shell::app::spawn(async move {
        let kept = with(|seen| seen.wallpaper.as_ref().map(Wallpaper::read_from));
        if let Some(wallpaper) = read_the_wallpaper(kept).await {
            change(|seen| seen.wallpaper = Some(wallpaper));
            crate::home::redraw(&app);
        }
    });
}

/// The wallpaper read again when its file, its size or its colour has
/// changed since `kept`, and nothing when it has not.
async fn read_the_wallpaper(kept: Option<ReadFrom>) -> Option<Wallpaper> {
    let room = ROOM.load(std::sync::atomic::Ordering::Relaxed);
    let size = ((room >> 32) as u32, room as u32);
    if size.0 == 0 || size.1 == 0 {
        return None;
    }
    crate::shell::app::spawn_blocking(move || {
        let desktop = zyr_system::wallpaper();
        let from = desktop.picture.and_then(|path| {
            let changed = std::fs::metadata(&path)
                .and_then(|file| file.modified())
                .ok()?;
            Some((path, changed))
        });
        let now = (from.clone(), size, desktop.colour);
        if kept.as_ref() == Some(&now) {
            return None;
        }
        let picture = from
            .as_ref()
            .and_then(|(path, _)| Picture::read(path, size))
            .map(Arc::new);
        Some(Wallpaper {
            picture,
            colour: desktop.colour,
            from,
            size,
        })
    })
    .await
    .ok()
    .flatten()
}

/// Has the shortcuts read again, after one was changed.
pub fn shortcuts_changed() {
    change(|seen| seen.shortcuts = crate::shell::shortcuts::engraved());
}
