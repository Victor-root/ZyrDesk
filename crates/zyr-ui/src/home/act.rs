//! What a click, or a key, does.
//!
//! Nothing waits here: whatever asks the service goes off on a task of
//! its own and has the window drawn again on its way back. The thread
//! that draws never waits for an answer that goes through a pipe.
//!
//! What a dialog's own buttons do is written with the dialog; what the
//! pages ask for is here.

use std::time::Instant;

use zyr_i18n::say;
use zyr_proto::fact::Fact;
use zyr_session::presence::Narrowed;

use crate::home::dialogs;
use crate::home::pages::settings::{CODECS, DISPLAYS};
use crate::home::seen;
use crate::home::state::{Copyable, Dialog, FarJournal, Offer, Page, Pick, Target, Toggle};
use crate::home::ui::fields::{self, Field};
use crate::shell::app::App;
use crate::shell::settings::Chosen;

/// Does what that target does.
pub fn act(app: &App, target: Target) {
    match target {
        Target::Nav(page) => go_to(app, page),
        Target::Drawer => {
            let mut state = crate::home::state();
            state.drawer = !state.drawer;
            drop(state);
            crate::home::redraw(app);
        }
        Target::Fold => {
            crate::home::state().drawer = false;
            crate::home::redraw(app);
        }
        Target::Person => {
            let linked = seen::with(|seen| {
                seen.account
                    .as_ref()
                    .is_some_and(|account| account.link.is_some())
            });
            if linked {
                go_to(app, Page::Settings);
            } else {
                sign_in(app);
            }
        }
        Target::Switch(toggle) => push(app, toggle),
        Target::Remedy => remedy(app),
        Target::SendBackAll => {
            let watching: Vec<String> = seen::with(|seen| {
                seen.watching
                    .iter()
                    .map(|watcher| watcher.fingerprint.clone())
                    .collect()
            });
            for fingerprint in watching {
                send_back(app, fingerprint);
            }
        }
        Target::RenameThis => {
            if let Some((device, name)) = seen::with(|seen| {
                seen.this_device()
                    .map(|device| (device.id.clone(), device.name.clone()))
            }) {
                rename(app, device, name);
            }
        }
        Target::Details => dialogs::open(app, Dialog::Details),
        Target::Connect(fingerprint) => connect(app, &fingerprint, false),
        Target::Wake(fingerprint) => wake(app, fingerprint),
        Target::Menu(menu) => {
            let mut state = crate::home::state();
            let open = state.menu.as_ref().is_some_and(|(open, _)| *open == menu);
            state.menu = if open {
                None
            } else {
                Some((menu, zyr_draw::Rect::at(0.0, 0.0, 0.0, 0.0)))
            };
            drop(state);
            crate::home::redraw(app);
        }
        Target::Field(field) => fields::focus(field),
        // Emptied, the field says so, and the search follows.
        Target::ClearSearch => fields::write(Field::Search, ""),
        Target::Narrow(narrowed) => {
            let mut state = crate::home::state();
            state.narrowed = narrowed;
            state.menu = None;
            state.page_scroll.reset();
            drop(state);
            crate::home::redraw(app);
        }
        Target::ShowAll => {
            crate::home::state().narrowed = Narrowed::All;
            fields::write(Field::Search, "");
            crate::home::redraw(app);
        }
        Target::View(view) => {
            let order = {
                let mut state = crate::home::state();
                state.view = view;
                state.order.clone()
            };
            crate::home::arranged::keep(view, order);
            crate::home::redraw(app);
        }
        Target::Arrange => {
            let mut state = crate::home::state();
            state.arranging = !state.arranging;
            drop(state);
            crate::home::redraw(app);
        }
        Target::Offer(fingerprint, offer) => {
            // Ending a session and forgetting a computer sit right on its
            // card: one click there must not be enough.
            let undone = matches!(offer, Offer::SendBack | Offer::Forget);
            if undone && !second_click(app, Target::Offer(fingerprint.clone(), offer)) {
                return;
            }
            crate::home::redraw(app);
            match offer {
                Offer::HereOnly => connect(app, &fingerprint, true),
                Offer::Journal => {
                    let far = seen::with(|seen| {
                        seen.peer(&fingerprint).map(|peer| FarJournal {
                            name: peer.name.clone(),
                            address: peer.host.clone(),
                            fingerprint: fingerprint.clone(),
                        })
                    });
                    if let Some(far) = far {
                        dialogs::open_the_journal(app, Some(far));
                    }
                }
                Offer::SendBack => send_back(app, fingerprint),
                Offer::Forget => forget(app, fingerprint),
            }
        }
        Target::AddComputer => dialogs::open(app, Dialog::Adding),
        Target::Theme(choice) => {
            crate::shell::theme::choose(choice);
            crate::home::theme_changed(app);
        }
        Target::Side(which, rank) => pick(app, which, rank),
        Target::Shortcut(doing) => {
            let mut state = crate::home::state();
            state.listening = if state.listening == Some(doing) {
                None
            } else {
                Some(doing)
            };
            drop(state);
            crate::home::redraw(app);
        }
        Target::SettingsTab(tab) => {
            let mut state = crate::home::state();
            if state.settings_tab != tab {
                state.page_scroll.reset();
            }
            state.settings_tab = tab;
            state.listening = None;
            drop(state);
            crate::home::redraw(app);
        }
        Target::SignIn => sign_in(app),
        Target::Detach => detach(app),
        Target::RenameDevice(device) => {
            let name = seen::with(|seen| {
                seen.account
                    .as_ref()
                    .and_then(|account| account.devices.iter().find(|known| known.id == device))
                    .map(|known| known.name.clone())
            });
            if let Some(name) = name {
                rename(app, device, name);
            }
        }
        Target::Revoke(device) => revoke(app, device),
        Target::OpenJournal => dialogs::open_the_journal(app, None),
        Target::OpenLogs => open_a_folder(app, "logs"),
        Target::Link(link) => {
            if crate::home::licences::is_ours(link)
                && let Err(reason) = crate::shell::win32::open_in_the_browser(link)
            {
                crate::home::state().tell_trouble(&reason);
                crate::home::redraw(app);
            }
        }
        Target::Licence(rank) => dialogs::open(app, Dialog::Licence(rank)),
        Target::CloseDialog => dialogs::close(app),
        Target::Confirm => dialogs::confirm(app),
        Target::Copy(Copyable::Fingerprint) => {
            let own =
                seen::with(|seen| seen.standing.as_ref().map(|said| said.fingerprint.clone()));
            if let Some(own) = own.filter(|own| !own.is_empty()) {
                copy(app, &own, Copyable::Fingerprint);
            }
        }
        Target::Copy(Copyable::Journal) => dialogs::copy_the_journal(app),
        Target::Forget(fingerprint) => {
            if second_click(app, Target::Forget(fingerprint.clone())) {
                forget(app, fingerprint);
            }
        }
        Target::JournalRefresh => dialogs::reread_the_journal(app, dialogs::After::Show),
        Target::JournalEmpty => dialogs::empty_the_journal(app),
        Target::JournalTag(name) => dialogs::toggle_tag(app, &name),
        // Held and dragged, never clicked.
        Target::Scrollbar(_) | Target::Move(_) => {}
        Target::DismissNotice => {
            crate::home::state().notice = None;
            crate::home::redraw(app);
        }
    }
}

/// Shows a page, from its top.
fn go_to(app: &App, page: Page) {
    {
        let mut state = crate::home::state();
        if state.page != page {
            state.page_scroll.reset();
        }
        state.page = page;
        state.drawer = false;
        state.menu = None;
        state.listening = None;
    }
    crate::home::redraw(app);
}

/// Narrows the computers to what the search holds now. Typed from a page
/// that shows none, it leads to the page of computers.
pub fn searched(app: &App, words: String) {
    let away = {
        let mut state = crate::home::state();
        if state.search == words {
            return;
        }
        state.search = words;
        state.page_scroll.reset();
        !matches!(state.page, Page::Home | Page::Computers) && !state.search.is_empty()
    };
    if away {
        go_to(app, Page::Computers);
    } else {
        crate::home::redraw(app);
    }
}

/// What Enter and Escape do in the search: Enter shows every computer
/// found, on their page; Escape empties it and lets go of the keyboard.
pub fn search_key(app: &App, enter: bool) {
    if enter {
        go_to(app, Page::Computers);
    } else {
        fields::write(Field::Search, "");
        fields::leave();
    }
}

/// Opens the account's dialog, this computer's name already written.
fn sign_in(app: &App) {
    let name = seen::with(|seen| seen.this_name());
    dialogs::open(app, Dialog::SignIn);
    fields::write(Field::DeviceName, &name);
}

/// Opens the renaming of a device of the account, its name already
/// written.
fn rename(app: &App, device: String, name: String) {
    dialogs::open(
        app,
        Dialog::Renaming {
            device,
            name: name.clone(),
        },
    );
    fields::write(Field::NewName, &name);
}

/// Whether this is the second click a button asked for. The first only
/// arms it, and the wait lapses by itself.
pub fn second_click(app: &App, target: Target) -> bool {
    let mut state = crate::home::state();
    if state.is_armed(&target) {
        state.armed = None;
        return true;
    }
    state.armed = Some((target, Instant::now()));
    drop(state);
    crate::home::redraw(app);
    false
}

/// What repairs what keeps this computer from working: the service, set
/// up and started, or FFmpeg's folder, opened for its files.
fn remedy(app: &App) {
    let stopped = seen::with(|seen| {
        seen.standing
            .as_ref()
            .is_some_and(|said| said.unreachable.is_some())
    });
    if !stopped {
        open_a_folder(app, "ffmpeg");
        return;
    }
    let app = app.clone();
    crate::shell::app::spawn(async move {
        if let Err(reason) = crate::shell::desk::start_service().await {
            crate::home::state().tell_trouble(&reason);
        }
        seen::again(&app).await;
        crate::home::redraw(&app);
    });
}

fn open_a_folder(app: &App, which: &'static str) {
    if let Err(reason) = crate::shell::folders::open_folder(which.to_string()) {
        crate::home::state().tell_trouble(&reason);
        crate::home::redraw(app);
    }
}

/// Pushes a switch, which stays where the hand put it until the service
/// says it has done it.
fn push(app: &App, toggle: Toggle) {
    let wanted = !seen::with(|seen| {
        let state = crate::home::state();
        crate::home::pages::home::toggle_is_on(seen, &state, toggle)
    });
    {
        let mut state = crate::home::state();
        state.pushed.retain(|(pushed, _)| *pushed != toggle);
        state.pushed.push((toggle, wanted));
        state.notice = None;
    }
    crate::home::redraw(app);

    let app = app.clone();
    crate::shell::app::spawn(async move {
        let done = match toggle {
            Toggle::Access => crate::shell::desk::set_hosting(wanted).await,
            Toggle::Trust => crate::shell::desk::set_trust(wanted).await,
            Toggle::AtBoot => crate::shell::desk::set_at_boot(wanted).await,
            Toggle::Marking => crate::shell::desk::set_ecn(wanted).await,
            Toggle::FixedPort => crate::shell::desk::set_fixed_port(wanted).await,
            Toggle::FarSpeakers => {
                write_the_settings(|chosen| chosen.mute_far_speakers = wanted).await
            }
            Toggle::Statistics => write_the_settings(|chosen| chosen.stats_overlay = wanted).await,
        };
        if let Err(reason) = done {
            crate::home::state().tell_trouble(&reason);
        }
        seen::again(&app).await;
        crate::home::state()
            .pushed
            .retain(|(pushed, _)| *pushed != toggle);
        crate::home::redraw(&app);
    });
}

/// Picks one of the sides of a row.
fn pick(app: &App, which: Pick, rank: usize) {
    // This one goes nowhere: it changes the shape of the account's
    // dialog, and nothing else.
    if which == Pick::SignUp {
        crate::home::state().sign_up = rank == 1;
        crate::home::redraw(app);
        return;
    }
    let app = app.clone();
    crate::shell::app::spawn(async move {
        let done = write_the_settings(|chosen| match which {
            Pick::Codec => {
                if let Some(codec) = CODECS.get(rank).and_then(|codec| codec.parse().ok()) {
                    chosen.codec = codec;
                }
            }
            Pick::Display => {
                if let Some(display) = DISPLAYS.get(rank).and_then(|display| display.parse().ok()) {
                    chosen.display = display;
                }
            }
            Pick::Mouse => chosen.absolute_mouse = rank == 0,
            Pick::SignUp => {}
        })
        .await;
        if let Err(reason) = done {
            crate::home::state().tell_trouble(&reason);
        }
        seen::again(&app).await;
        crate::home::redraw(&app);
    });
}

/// Changes one choice of every session, the others as they stand: the
/// whole set goes to the service, so that it never has to guess what
/// stayed.
async fn write_the_settings(change: impl FnOnce(&mut Chosen)) -> Result<(), Fact> {
    let mut chosen = Chosen::of(crate::shell::settings::preferred().await);
    change(&mut chosen);
    crate::shell::settings::choose(chosen).await
}

/// Opens a session towards the computer of that fingerprint, by the best
/// road or through this network and nothing else.
fn connect(app: &App, fingerprint: &str, only_here: bool) {
    let aimed = seen::with(|seen| {
        seen.peer(fingerprint)
            .map(|peer| (peer.host.clone(), peer.name.clone()))
    });
    if let Some((host, name)) = aimed {
        launch(app, &host, fingerprint, &name, only_here);
    }
}

/// Opens a session towards this computer. One at a time: two pictures
/// in one window cannot both be driven.
pub fn launch(app: &App, host: &str, fingerprint: &str, name: &str, only_here: bool) {
    let busy = seen::with(|seen| seen.busy(crate::home::state().opening.is_some()));
    if busy {
        return;
    }
    crate::home::opening::begin(app, name);
    let (app, host, fingerprint) = (app.clone(), host.to_string(), fingerprint.to_string());
    crate::shell::app::spawn(async move {
        if let Err(reason) =
            crate::session::connect(app.clone(), host, fingerprint, only_here).await
        {
            crate::home::opening::failed(&app, &reason);
        }
    });
}

/// Forgets a computer written down by hand, from the screen and from
/// those let in at once.
fn forget(app: &App, fingerprint: String) {
    let name = seen::with(|seen| seen.peer(&fingerprint).map(|peer| peer.name.clone()));
    let app = app.clone();
    crate::shell::app::spawn(async move {
        match crate::shell::desk::forget(fingerprint).await {
            Ok(()) => {
                if let Some(name) = name {
                    crate::home::state().tell_news(say!("computer.forgotten", name = name));
                }
            }
            Err(reason) => crate::home::state().tell_trouble(&reason),
        }
        seen::again(&app).await;
        crate::home::redraw(&app);
    });
}

/// Wakes a computer of this network that is switched off, and says the
/// signal went: it takes the computer a moment to come up.
fn wake(app: &App, fingerprint: String) {
    let name = seen::with(|seen| seen.peer(&fingerprint).map(|peer| peer.name.clone()));
    let app = app.clone();
    crate::shell::app::spawn(async move {
        match crate::shell::desk::wake(fingerprint).await {
            Ok(()) => crate::home::state()
                .tell_news(say!("computer.wake_sent", name = name.unwrap_or_default())),
            Err(reason) => crate::home::state().tell_trouble(&reason),
        }
        crate::home::redraw(&app);
    });
}

/// Sends back the computer controlling this one right now.
fn send_back(app: &App, fingerprint: String) {
    let app = app.clone();
    crate::shell::app::spawn(async move {
        if let Err(reason) = crate::shell::desk::kick(fingerprint).await {
            crate::home::state().tell_trouble(&reason);
        }
        seen::again(&app).await;
        crate::home::redraw(&app);
    });
}

/// Detaches this computer from its account, after a second click.
fn detach(app: &App) {
    if !second_click(app, Target::Detach) {
        return;
    }
    crate::home::redraw(app);
    let app = app.clone();
    crate::shell::app::spawn(async move {
        if let Err(reason) = crate::shell::desk::detach().await {
            crate::home::state().tell_trouble(&reason);
        }
        seen::again(&app).await;
        crate::home::redraw(&app);
    });
}

/// Takes a device off the account, after a second click on the same one.
fn revoke(app: &App, device: String) {
    if !second_click(app, Target::Revoke(device.clone())) {
        return;
    }
    crate::home::redraw(app);
    let app = app.clone();
    crate::shell::app::spawn(async move {
        if let Err(reason) = crate::shell::desk::revoke_device(device).await {
            crate::home::state().tell_trouble(&reason);
        }
        seen::again(&app).await;
        crate::home::redraw(&app);
    });
}

/// Copies this text, and has its button say so.
///
/// Windows can refuse, and a button saying « Copied » after a refusal
/// would send someone to paste nothing on the other computer.
pub fn copy(app: &App, text: &str, what: Copyable) {
    if let Err(e) = zyr_clipboard::hold_this(&zyr_proto::clipboard::Clip::text(text)) {
        crate::home::note(&format!("copy refused: {e}"));
        crate::home::state().tell_trouble(&Fact::new("window.copy_refused"));
        crate::home::redraw(app);
        return;
    }
    crate::home::state().copied = Some((what, Instant::now()));
    crate::home::redraw(app);
}
