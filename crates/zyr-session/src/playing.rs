//! A session played to its end: opened, played by whatever shows it,
//! and its picture brought back when it falls over on its own.
//!
//! A road between two homes goes quiet for a few seconds now and then,
//! and thirty seconds of it end a session by design (D138). What that
//! would cost is the whole session: the picture gone, the person back on
//! the home screen with something to click and a far computer to wait
//! for again. Making that half-minute unlosable is a race that cannot be
//! won: it takes one outage a little too long, or one unlucky cascade,
//! and the session is over.
//!
//! This is the other answer, and it is the one the products people
//! compare this to give: falling over stops costing the session. The
//! picture comes back by the road the person walked by hand, and it comes
//! back on its own.
//!
//! What is decided here is when a session that ended is worth opening
//! again, how many times in a row, how long to wait before, and what is
//! told once it is over. What plays the picture and says each moment
//! where the person looks is a [`Stage`]: the window is one.

use std::path::Path;
use std::time::{Duration, Instant};

use zyr_control::CHANNEL;
use zyr_player::Ending;
use zyr_proto::fact::Fact;
use zyr_proto::log::Log;
use zyr_proto::paths;

use crate::{Error, Opened, Step, Wanted, opened_on};

/// What this module files its journal lines under.
const TAG: &str = "session";

/// How many times in a row the picture is brought back before the person
/// is told instead.
///
/// A session that falls over, comes back and falls over again within the
/// minute is not a network that hiccups: it is one that cannot carry a
/// session at all just now, and bringing the picture back forever would
/// hide that behind a screen that never settles.
const COMES_BACK_IN_A_ROW: u32 = 5;

/// How many of those may fail to open at all, one after the other.
///
/// Its own count, and a much shorter one, because the two failures cost
/// wildly different amounts of time. A picture that came back and fell
/// over again was answered in seconds; an opening that finds nobody
/// takes fifteen seconds twice over, the service asking a second time on
/// its own (D171). The far computer being off is exactly what this looks
/// like, and telling the person that after a minute is honest where
/// telling them after three would be a product that hangs.
const OPENINGS_MISSED_IN_A_ROW: u32 = 2;

/// A session that stood this long before falling over is a fresh
/// accident and not the same one over again, so the count starts over.
const HELD_LONG_ENOUGH: Duration = Duration::from_secs(60);

/// The pause before the picture is asked for again.
///
/// The far computer has its own tidying to do once its client vanishes:
/// it puts the desk back the way it found it, and lets go of the engine
/// that served the session. It also learns that the client is gone by its
/// own patience running out, which can leave it half a minute behind this
/// end. Waiting a moment costs the person nothing they can feel and
/// spares one try landing on a computer that is still holding the session
/// that just fell over.
const BEFORE_COMING_BACK: Duration = Duration::from_secs(3);

/// How often the pause looks up to see whether it is still wanted.
const WHILE_WAITING: Duration = Duration::from_millis(50);

/// What plays a session, and says what happens around it.
pub trait Stage {
    /// A moment of the opening, as it happens.
    fn step(&self, step: &Step);

    /// Whether the session is still wanted.
    ///
    /// Asked all along the opening, once the picture has gone and all
    /// through the pause before it comes back: a session let go of is let
    /// go of where it stands, and one that ended because it was closed is
    /// never told as one that broke.
    fn still_wanted(&self) -> bool;

    /// Plays the opened session until it ends, and says how it ended, or
    /// why it could not be played at all.
    ///
    /// `shown` is called the moment the first picture is on screen: that
    /// is when the opening is over, and when a picture on its way back
    /// has made it.
    fn play(&mut self, opened: Opened, shown: &mut dyn FnMut()) -> Result<Ending, Fact>;

    /// Says the picture is on its way back, as try `attempt`.
    ///
    /// What is deliberately not put down meanwhile is whatever the
    /// session was shown in. It is one session as the person sees it, and
    /// handing the screen back to take it again a second later is exactly
    /// the flicker this road exists to spare them.
    fn coming_back(&self, attempt: u32);

    /// What the far computer is asked for when the way is opened again.
    ///
    /// Everything it was told went with the old way, so all of it is
    /// asked afresh, and with what is chosen now rather than what was
    /// chosen when the session opened.
    fn asked_afresh(&mut self, wanted: &mut Wanted);
}

/// Opens the session and plays it to its end, bringing its picture back
/// when it falls over on its own.
///
/// Answers what there is to tell once it is over: why it ended badly or
/// never opened, and nothing when it was closed or ended cleanly.
///
/// Waits for as long as the session lasts, on a plain thread: see
/// [`Driving`](crate::Driving) for why never inside an async task.
pub fn see_it_through(wanted: Wanted, stage: &mut impl Stage, log: &Log) -> Option<Fact> {
    seen_through_on(
        &paths::ffmpeg_dir(),
        CHANNEL,
        BEFORE_COMING_BACK,
        wanted,
        stage,
        log,
    )
}

/// The same, with FFmpeg looked for in `ffmpeg`, through the service
/// listening on `channel`, and pausing that long before the picture is
/// asked for again: the product's own, or ones a test stands in for.
fn seen_through_on(
    ffmpeg: &Path,
    channel: &str,
    pause: Duration,
    mut wanted: Wanted,
    stage: &mut impl Stage,
    log: &Log,
) -> Option<Fact> {
    let log = log.about(TAG);
    log.write(&format!("session asked for towards {}", wanted.host));
    let mut coming_back = ComingBack::none();
    loop {
        let mut opening = Opening::begins();
        let opened = {
            let stage = &*stage;
            opened_on(
                ffmpeg,
                channel,
                &wanted,
                &mut |step| {
                    log.write(&written(&step));
                    opening.reached(&step);
                    stage.step(&step);
                },
                &|| stage.still_wanted(),
            )
        };
        let played = match opened {
            Ok(opened) => {
                opening.opened();
                let mut shown_at: Option<Instant> = None;
                let ended = stage.play(opened, &mut || {
                    opening.shown();
                    shown_at = Some(Instant::now());
                    log.write(&opening.how_long_it_took());
                    if coming_back.tried() {
                        // Worth its own line, and worth reading tomorrow:
                        // this is the whole of what a session that used to
                        // die looks like now.
                        log.write(&format!(
                            "the picture came back after {} attempt(s), the session goes on",
                            coming_back.try_number()
                        ));
                    }
                    coming_back.opened();
                });
                ended.map(|ended| (ended, shown_at.map(|at| at.elapsed())))
            }
            // Let go of by whoever asked for it: there is nothing to tell
            // them about it, they are the one who asked.
            Err(Error::Abandoned) => {
                log.write("opening abandoned: the session was closed before the picture");
                return None;
            }
            Err(e) => Err(e.fact()),
        };
        let (ended, stood) = match played {
            Ok(played) => played,
            Err(reason) => match failed_to_open(stage, pause, &log, reason, &mut coming_back) {
                Then::Again => {
                    stage.asked_afresh(&mut wanted);
                    continue;
                }
                Then::Over(trouble) => return trouble,
            },
        };

        let on_purpose = !stage.still_wanted();
        log.write(&if on_purpose {
            format!("session closed on purpose, the player said {ended:?}")
        } else {
            format!("session over: {ended:?}")
        });
        if on_purpose {
            return None;
        }

        // A player that ended before its first picture is an opening that
        // failed, and is answered as one: it did not fall over, it never
        // stood.
        let Some(held) = stood else {
            let reason = before_the_picture(&ended)?;
            match failed_to_open(stage, pause, &log, reason, &mut coming_back) {
                Then::Again => {
                    stage.asked_afresh(&mut wanted);
                    continue;
                }
                Then::Over(trouble) => return trouble,
            }
        };

        // Nobody asked for this one, so the picture comes back rather than
        // the session ending under the person. Whatever showed it keeps
        // what it knows, so what they see is a picture that goes and
        // returns.
        if coming_back.after(&ended, held) {
            log.write(&format!(
                "the session fell over on its own, the picture is brought back ({} of {})",
                coming_back.in_a_row, COMES_BACK_IN_A_ROW
            ));
            if !held_on(stage, pause, &log, &coming_back) {
                return None;
            }
            stage.asked_afresh(&mut wanted);
            continue;
        }

        return why_it_is_over(ended, &coming_back);
    }
}

/// What comes after an opening that failed.
enum Then {
    /// The picture is on its way back: the way is asked for again.
    Again,
    /// The session is over, with what there is to tell about it.
    Over(Option<Fact>),
}

/// Answers an opening that failed.
///
/// A picture on its way back that did not open is one of its tries and
/// not the end of them: the far computer can still be holding the session
/// that just fell over, which it learns of by its own patience running
/// out. Any other opening that fails is told as it is.
fn failed_to_open(
    stage: &impl Stage,
    pause: Duration,
    log: &Log,
    reason: Fact,
    coming_back: &mut ComingBack,
) -> Then {
    log.write(&format!("session not opened: {reason}"));
    if coming_back.again() {
        if !held_on(stage, pause, log, coming_back) {
            return Then::Over(None);
        }
        return Then::Again;
    }
    Then::Over(Some(if coming_back.tried() {
        Fact::new("session.picture_not_back")
            .with("times", coming_back.in_a_row)
            .because(&reason)
    } else {
        reason
    }))
}

/// Says the picture is on its way back, and waits out the moment the far
/// computer needs.
///
/// Answers whether to go on: a session let go of during the pause is
/// heard at once, and there is nothing left to bring back.
fn held_on(stage: &impl Stage, pause: Duration, log: &Log, coming_back: &ComingBack) -> bool {
    stage.coming_back(coming_back.try_number());
    let waited = waited_out(stage, pause);
    if !waited {
        log.write("coming back abandoned: the session was closed during the wait");
    }
    waited
}

/// Waits that long, unless the session stops being wanted meanwhile.
///
/// Answers whether the wait ran its course. Looked up from rather than
/// slept through: a session closed during it would otherwise be answered
/// three seconds later, by a picture coming back that nobody wants.
fn waited_out(stage: &impl Stage, how_long: Duration) -> bool {
    let until = Instant::now() + how_long;
    loop {
        if !stage.still_wanted() {
            return false;
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return true;
        }
        std::thread::sleep(left.min(WHILE_WAITING));
    }
}

/// What a session that stood and then ended has to tell, or nothing when
/// it ended cleanly.
fn why_it_is_over(ended: Ending, coming_back: &ComingBack) -> Option<Fact> {
    match ended {
        Ending::Asked | Ending::HostLeft => None,
        _ if coming_back.tried() => {
            Some(Fact::new("session.did_not_hold").with("times", coming_back.in_a_row))
        }
        Ending::LinkLost => Some(Fact::new("session.link_lost")),
        Ending::EngineFailed(reason) => Some(reason),
    }
}

/// What a session that ended before its first picture has to say, or
/// nothing when it was closed from here.
fn before_the_picture(ended: &Ending) -> Option<Fact> {
    match ended {
        Ending::Asked => None,
        Ending::HostLeft => Some(Fact::new("session.host_left_early")),
        Ending::LinkLost => Some(Fact::new("session.link_lost_early")),
        Ending::EngineFailed(reason) => Some(reason.clone()),
    }
}

/// How many times the picture has been brought back, in a row.
struct ComingBack {
    /// How many times in a row, with nothing between them long enough to
    /// call the next one a fresh accident.
    in_a_row: u32,
    /// How many of those did not manage to open, in a row.
    missed: u32,
}

impl ComingBack {
    fn none() -> Self {
        Self {
            in_a_row: 0,
            missed: 0,
        }
    }

    /// The picture is up again: whatever it took to get here is spent,
    /// and the next opening that fails is the first of its own row.
    fn opened(&mut self) {
        self.missed = 0;
    }

    /// Which try the person is being shown, counting both roads: a
    /// picture that fell over again and an opening that found nobody are
    /// one wait as they see it.
    fn try_number(&self) -> u32 {
        self.in_a_row + self.missed
    }

    /// Whether the picture is worth bringing back, the session having
    /// ended without anybody asking for it.
    ///
    /// Only what a session falling over looks like: the link lost, or the
    /// player saying it could not go on. A far computer that ended it
    /// cleanly has decided so, and that is not an accident to undo.
    fn after(&mut self, ended: &Ending, held: Duration) -> bool {
        if !matches!(ended, Ending::LinkLost | Ending::EngineFailed(_)) {
            return false;
        }
        if held >= HELD_LONG_ENOUGH {
            self.in_a_row = 0;
        }
        self.once_more()
    }

    /// The same, an opening having failed rather than a picture having
    /// fallen over.
    ///
    /// Only while the picture was already coming back: a session that
    /// never opened in the first place is the person's own try, answered
    /// where they can see it, and the service has already asked twice by
    /// then (D171).
    fn again(&mut self) -> bool {
        if !self.tried() || self.missed >= OPENINGS_MISSED_IN_A_ROW {
            return false;
        }
        self.missed += 1;
        true
    }

    fn once_more(&mut self) -> bool {
        if self.in_a_row >= COMES_BACK_IN_A_ROW {
            return false;
        }
        self.in_a_row += 1;
        true
    }

    /// Whether the picture has been brought back at all.
    fn tried(&self) -> bool {
        self.in_a_row > 0
    }
}

/// How long an opening took, in its parts.
///
/// One line at the end rather than timestamps to subtract by hand.
/// Opening a session is the wait a person actually feels, and only some
/// of it is ours to shorten: guessing which part has already cost an
/// evening, twice. Written where the timestamps in the journal cannot
/// answer, since they are cut to the second and every part of this is
/// smaller than that.
struct Opening {
    asked: Instant,
    reached: Option<Duration>,
    opened: Option<Duration>,
    shown: Option<Duration>,
}

impl Opening {
    fn begins() -> Self {
        Self {
            asked: Instant::now(),
            reached: None,
            opened: None,
            shown: None,
        }
    }

    /// Notes when the way stood.
    ///
    /// The questions put to the far computer afterwards say nothing when
    /// they are answered, only when they are refused, so they cannot be
    /// timed one by one from here. They all sit between the tunnel
    /// standing and the way handed over, and that is how they are
    /// counted: together.
    fn reached(&mut self, step: &Step) {
        if *step == Step::Reached {
            self.reached = Some(self.asked.elapsed());
        }
    }

    /// Notes when the way was handed over, everything asked.
    fn opened(&mut self) {
        self.opened = Some(self.asked.elapsed());
    }

    /// Notes when the first picture was on screen, which is the only
    /// moment the person is waiting for.
    fn shown(&mut self) {
        self.shown = Some(self.asked.elapsed());
    }

    fn how_long_it_took(&self) -> String {
        let since = |from: Option<Duration>, to: Option<Duration>| match (from, to) {
            (Some(from), Some(to)) => format!("{} ms", to.saturating_sub(from).as_millis()),
            _ => "not measured".to_string(),
        };
        format!(
            "picture on screen {} after the request: {} to reach the far computer, {} to ask \
             it what is needed, {} from the player to the first picture",
            since(Some(Duration::ZERO), self.shown),
            since(Some(Duration::ZERO), self.reached),
            since(self.reached, self.opened),
            since(self.opened, self.shown),
        )
    }
}

/// A moment of the opening, in the journal.
///
/// Whatever shows it shows it for as long as it is on screen and then
/// draws something else over it. Opening a session is where most of what
/// can go wrong goes wrong, and every step of it is worth having in
/// writing afterwards, when there is nothing left on screen to look at.
fn written(step: &Step) -> String {
    match step {
        Step::Reached => "tunnel open".to_string(),
        Step::NoSoundCardHere => "this computer has no audio output".to_string(),
        Step::SpeakersLeftAlone { refused } => {
            format!("the far computer's speakers stay on: {refused}")
        }
        Step::ScreenLeftAlone { refused } => {
            format!("the far computer did not wake its virtual screen: {refused}")
        }
        Step::ScreenOverThere { wide, high } => {
            format!("the far computer shows {wide}x{high}, which is what the player is asked for")
        }
        Step::FarScreenLeftAlone { refused } => {
            format!("the far computer keeps the screen it films: {refused}")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    use zyr_control::{Answer, Request};

    use super::*;
    use crate::testing::{FfmpegHere, a_service, wanted, willing};

    /// A session that fell over the instant it opened.
    fn fell_over() -> Ending {
        Ending::LinkLost
    }

    #[test]
    fn a_picture_that_falls_over_comes_back_a_bounded_number_of_times() {
        let mut coming_back = ComingBack::none();
        assert!(!coming_back.tried());
        // It comes back, as long as the session does not stand long
        // enough in between for it to be a new accident.
        for attempt in 1..=COMES_BACK_IN_A_ROW {
            assert!(
                coming_back.after(&fell_over(), Duration::from_secs(2)),
                "coming back {attempt}"
            );
            assert_eq!(coming_back.in_a_row, attempt);
        }
        // Past that count, the person is told rather than left
        // watching a screen that never settles.
        assert!(!coming_back.after(&fell_over(), Duration::from_secs(2)));
        assert!(coming_back.tried());
    }

    #[test]
    fn a_session_that_stood_long_enough_starts_the_count_over() {
        let mut coming_back = ComingBack::none();
        for _ in 0..COMES_BACK_IN_A_ROW {
            assert!(coming_back.after(&fell_over(), Duration::from_secs(2)));
        }
        assert!(!coming_back.after(&fell_over(), Duration::from_secs(2)));
        // A session that stood for a minute and then falls over is a
        // new failure, and not the same one starting again.
        assert!(coming_back.after(&fell_over(), HELD_LONG_ENOUGH));
        assert_eq!(coming_back.in_a_row, 1);
    }

    #[test]
    fn a_session_the_far_computer_ended_is_not_brought_back() {
        let mut coming_back = ComingBack::none();
        // Hanging up is a decision of the far computer, not an accident
        // to undo, and a session closed here was closed on purpose.
        assert!(!coming_back.after(&Ending::HostLeft, Duration::from_secs(2)));
        assert!(!coming_back.after(&Ending::Asked, Duration::from_secs(2)));
        assert!(!coming_back.tried());
        // A player that could not go on, yes: from where the person
        // sits, it is the same thing as a session falling over.
        assert!(coming_back.after(
            &Ending::EngineFailed(
                Fact::new("player.graphics_gone").with("detail", "the card went away")
            ),
            Duration::from_secs(2)
        ));
    }

    #[test]
    fn an_opening_that_finds_nobody_is_only_retried_while_coming_back() {
        let mut coming_back = ComingBack::none();
        // The first attempt is the person's: they clicked, and the
        // failure is told where they see it.
        assert!(!coming_back.again());

        assert!(coming_back.after(&fell_over(), Duration::from_secs(2)));
        // A comeback that does not open is one of its tries, and they are
        // counted apart because they cost half a minute each.
        for attempt in 1..=OPENINGS_MISSED_IN_A_ROW {
            assert!(coming_back.again(), "missed attempt {attempt}");
        }
        assert!(!coming_back.again());

        // With the picture back, what it took to get there has been
        // spent.
        coming_back.opened();
        assert!(coming_back.again());
    }

    #[test]
    fn a_session_that_never_showed_a_picture_says_why_unless_closed_here() {
        // Closed from here is nothing to tell; everything else is.
        assert_eq!(before_the_picture(&Ending::Asked), None);
        for ended in [
            Ending::HostLeft,
            Ending::LinkLost,
            Ending::EngineFailed(Fact::new("engine.no_encoder")),
        ] {
            assert!(before_the_picture(&ended).is_some(), "{ended:?}");
        }
        let failed = Fact::new("player.graphics_gone").with("detail", "the card went away");
        assert_eq!(
            before_the_picture(&Ending::EngineFailed(failed.clone())),
            Some(failed)
        );
    }

    #[test]
    fn an_opening_says_how_long_each_part_took() {
        let mut opening = Opening::begins();
        // Nothing reached: every part says it was not measured rather
        // than a figure of nought.
        assert!(opening.how_long_it_took().contains("not measured"));
        opening.reached(&Step::Reached);
        opening.opened();
        opening.shown();
        let said = opening.how_long_it_took();
        assert!(!said.contains("not measured"), "{said}");
        assert!(said.contains("to the first picture"), "{said}");
    }

    /// A stage that plays what it is told to, and writes down what it is
    /// asked.
    struct Rehearsal {
        /// What each session played does, in order: whether its first
        /// picture shows, and how it ends.
        plays: VecDeque<(bool, Ending)>,
        /// Whether the session is still wanted.
        wanted: Cell<bool>,
        /// Let go of while its picture is on its way back.
        let_go_while_coming_back: bool,
        /// The tries the person was shown.
        comebacks: RefCell<Vec<u32>>,
        /// How many times the far computer was asked afresh.
        afresh: u32,
    }

    impl Rehearsal {
        fn playing(plays: impl IntoIterator<Item = (bool, Ending)>) -> Self {
            Self {
                plays: plays.into_iter().collect(),
                wanted: Cell::new(true),
                let_go_while_coming_back: false,
                comebacks: RefCell::new(Vec::new()),
                afresh: 0,
            }
        }
    }

    impl Stage for Rehearsal {
        fn step(&self, _step: &Step) {}

        fn still_wanted(&self) -> bool {
            self.wanted.get()
        }

        fn play(&mut self, opened: Opened, shown: &mut dyn FnMut()) -> Result<Ending, Fact> {
            drop(opened);
            let (shows, ended) = self.plays.pop_front().expect("a session to play");
            if shows {
                shown();
            }
            if ended == Ending::Asked {
                self.wanted.set(false);
            }
            Ok(ended)
        }

        fn coming_back(&self, attempt: u32) {
            self.comebacks.borrow_mut().push(attempt);
            if self.let_go_while_coming_back {
                self.wanted.set(false);
            }
        }

        fn asked_afresh(&mut self, _wanted: &mut Wanted) {
            self.afresh += 1;
        }
    }

    /// The whole road, through a service a test stands in for.
    fn seen_through(what: &str, service: &str, stage: &mut Rehearsal) -> Option<Fact> {
        let ffmpeg = FfmpegHere::new(what);
        let log = Log::open(&ffmpeg.0.join("session.log")).expect("the test journal opens");
        seen_through_on(&ffmpeg.0, service, Duration::ZERO, wanted(), stage, &log)
    }

    #[test]
    fn a_picture_that_fell_over_comes_back_and_the_session_goes_on() {
        let (channel, asked) = a_service("comes-back", willing);
        let mut stage = Rehearsal::playing([(true, Ending::LinkLost), (true, Ending::HostLeft)]);
        assert_eq!(seen_through("comes-back", &channel, &mut stage), None);
        assert_eq!(*stage.comebacks.borrow(), [1]);
        assert_eq!(stage.afresh, 1);
        let reached = asked
            .lock()
            .unwrap()
            .iter()
            .filter(|request| matches!(request, Request::Reach { .. }))
            .count();
        assert_eq!(reached, 2);
    }

    #[test]
    fn a_session_that_never_holds_is_told_after_its_last_comeback() {
        let (channel, _) = a_service("never-holds", willing);
        let fell = (0..=COMES_BACK_IN_A_ROW).map(|_| (true, Ending::LinkLost));
        let mut stage = Rehearsal::playing(fell);
        let told = seen_through("never-holds", &channel, &mut stage).expect("something to tell");
        assert_eq!(told.code(), "session.did_not_hold");
        assert_eq!(told.value("times"), Some("5"));
        assert_eq!(*stage.comebacks.borrow(), [1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_session_closed_on_purpose_is_neither_told_nor_brought_back() {
        let (channel, _) = a_service("closed", willing);
        let mut stage = Rehearsal::playing([(true, Ending::Asked)]);
        assert_eq!(seen_through("closed", &channel, &mut stage), None);
        assert!(stage.comebacks.borrow().is_empty());
    }

    #[test]
    fn a_first_opening_that_fails_is_told_as_it_is() {
        // The person's own try: they clicked, and the refusal is theirs
        // to read, with no comeback to wait through first.
        let (channel, _) = a_service("first-refused", |_| {
            Some(Answer::Refused(
                Fact::new("reach.nobody_answered")
                    .with("host", "192.168.1.20")
                    .with("waited", 15),
            ))
        });
        let mut stage = Rehearsal::playing([]);
        let told = seen_through("first-refused", &channel, &mut stage).expect("something to tell");
        assert_eq!(told.code(), "reach.nobody_answered");
        assert!(stage.comebacks.borrow().is_empty());
    }

    #[test]
    fn a_comeback_that_cannot_open_is_tried_again_then_told() {
        // The far computer answers the first time, and never again: it
        // went away with the session.
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let (channel, _) = a_service("not-back", move |request| match request {
            Request::Reach { .. }
                if calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 0 =>
            {
                Some(Answer::Refused(Fact::new("reach.nobody_answered")))
            }
            _ => willing(request),
        });
        let mut stage = Rehearsal::playing([(true, Ending::LinkLost)]);
        let told = seen_through("not-back", &channel, &mut stage).expect("something to tell");
        assert_eq!(told.code(), "session.picture_not_back");
        assert_eq!(told.value("times"), Some("1"));
        // The comeback itself, then one wait per opening that missed.
        let tries = 1 + OPENINGS_MISSED_IN_A_ROW;
        assert_eq!(*stage.comebacks.borrow(), (1..=tries).collect::<Vec<_>>());
    }

    #[test]
    fn a_session_let_go_of_while_coming_back_is_over_without_a_word() {
        let (channel, _) = a_service("let-go-coming-back", willing);
        let mut stage = Rehearsal {
            let_go_while_coming_back: true,
            ..Rehearsal::playing([(true, Ending::LinkLost)])
        };
        assert_eq!(
            seen_through("let-go-coming-back", &channel, &mut stage),
            None
        );
        assert_eq!(*stage.comebacks.borrow(), [1]);
        assert_eq!(stage.afresh, 0);
    }
}
