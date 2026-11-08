//! Rip Packs: the tab's options, its stage, and the keys that drive them.
//!
//! The ITGmania browser's `25 rip packs.lua`, less everything its relay did.
//! Finding the songs, numbering the pack and filling it is
//! `deadsync_online::rip_packs`, on its own threads. What is left here is what
//! the player sees and presses: nine option rows, a picker for eras and for
//! charters, a prompt for typed names and the meter's ends, and a stage where
//! a booster pack shakes while it is searched for, tears open, and deals its
//! fifteen cards.
//!
//! Every motion is worked out from the clock and what the rip says -- which
//! phase it is in and when it got there, when its cards arrived, when each
//! song landed -- and nothing is tweened. A stage left mid-deal and come back
//! to is drawn where it should be, which is the original's own rule.
//!
//! The options live for the session, as the original's do: leaving the
//! browser keeps them, and only RESET OPTIONS puts them back.

use super::layout as lo;
use super::state::{
    NAV_INITIAL_HOLD_DELAY, NAV_REPEAT_INTERVAL, State, Zone, banner_key, rip_showing, song_art_key,
};
use deadsync_online::rip_packs::{
    AuthorsPhase, AuthorsSnapshot, BOOSTER_SONGS, DEFAULT_HI, DEFAULT_LO, Era, KeepPhase,
    KeepState, METER_CEILING, METER_FLOOR, Pace, RipOptions, RipPhase, RipSnapshot, RipSong,
    SongState, merge_names, split_names,
};
use deadsync_theme::AudioRequest;
use std::borrow::Cow;
use std::cell::RefCell;
use std::ffi::OsString;
use std::sync::Arc;

/// How long after the cards land a START is taken as a choice, so the press
/// that ripped the pack cannot also keep it.
pub(super) const DECIDE_GUARD: f64 = 1.0;
/// The tear is heard only when the stage sees the pack open this soon after
/// it did: coming back to a pack opened while the player was elsewhere is
/// silent.
pub(super) const TEAR_HEARD_WITHIN: f64 = 0.5;
/// The longest a typed name list can be, and a meter's end.
const NAMES_MAX_CHARS: usize = 160;
const METER_MAX_CHARS: usize = 2;
/// A time far enough back that anything timed from it has long finished.
const LONG_AGO: f64 = -1.0e6;

// The deal and the flip, in seconds from the moment the pack opened.
pub(super) const DEAL_AT: f32 = 0.5;
pub(super) const DEAL_EACH: f32 = 0.07;
pub(super) const DEAL_TIME: f32 = 0.35;
/// A card turns over this soon after it lands, and takes this long.
pub(super) const FLIP_AFTER: f32 = 0.08;
pub(super) const FLIP_TIME: f32 = 0.32;
/// A song that has just come in bumps its card, over this long.
const BUMP_TIME: f32 = 0.3;

const SFX_CHANGE: &str = "assets/sounds/change.ogg";
const SFX_START: &str = "assets/sounds/start.ogg";
const SFX_INVALID: &str = "assets/sounds/common_invalid.ogg";

/// The option rows, top to bottom, which is also the cursor's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Row {
    Era,
    Author,
    Typed,
    Avoid,
    Lo,
    Hi,
    Full,
    /// RIP A PACK, or KEEP and RE-RIP while a pack is on the table.
    Button,
    Reset,
}

impl Row {
    pub(super) const ALL: [Self; 9] = [
        Self::Era,
        Self::Author,
        Self::Typed,
        Self::Avoid,
        Self::Lo,
        Self::Hi,
        Self::Full,
        Self::Button,
        Self::Reset,
    ];

    /// The small grey word over a row's value. The button and RESET have none.
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Era => "ERA",
            Self::Author => "AUTHOR",
            Self::Typed => "OR TYPE AUTHORS",
            Self::Avoid => "AVOID CHARTS BY",
            Self::Lo => "EXPERT FROM",
            Self::Hi => "EXPERT TO",
            Self::Full => "FULL DIFFICULTIES",
            Self::Button | Self::Reset => "",
        }
    }

    /// The rows LEFT and RIGHT change, which show the arrows that say so.
    pub(super) const fn cycles(self) -> bool {
        matches!(self, Self::Lo | Self::Hi | Self::Full)
    }
}

/// Which of the pair the button row is on while a pack is on the table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Choice {
    #[default]
    Keep,
    ReRip,
}

/// Which list the picker has open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pick {
    Eras,
    Authors,
}

/// What a text prompt is asking for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PromptKind {
    Typed,
    Avoid,
    Lo,
    Hi,
}

impl PromptKind {
    /// The question, as the prompt's title.
    pub(super) const fn question(self) -> &'static str {
        match self {
            Self::Typed => "Chart authors, separated by commas",
            Self::Avoid => "Chart authors to avoid, separated by commas",
            Self::Lo => "Lowest Expert meter (1 to 30)",
            Self::Hi => "Highest Expert meter (1 to 30)",
        }
    }

    const fn max_chars(self) -> usize {
        match self {
            Self::Typed | Self::Avoid => NAMES_MAX_CHARS,
            Self::Lo | Self::Hi => METER_MAX_CHARS,
        }
    }

    const fn numeric(self) -> bool {
        matches!(self, Self::Lo | Self::Hi)
    }
}

/// The text prompt: DeadSync has no `ScreenTextEntry`, so the browser asks
/// itself, over the tab, with the keyboard's typing and the pad's START and
/// BACK.
pub(super) struct Prompt {
    pub(super) kind: PromptKind,
    pub(super) text: String,
    /// Seconds since the last edit, which blinks the caret.
    pub(super) caret: f32,
}

/// Where the rip is, as the stage draws it: the original's six phases, read
/// off the service's snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Stage {
    /// No pack on the table: the unopened one idles.
    #[default]
    Idle,
    /// The pack shakes while songs are looked for.
    Searching,
    /// Its cards are on the table, face up, to keep or throw back.
    Choosing,
    /// Kept, and its songs coming in, a few at a time.
    Filling,
    /// Kept, and in the library.
    Done,
    /// No pack this time: the search found nothing, or no song came in.
    Failed,
}

/// The keys the tab answers to, whichever button sent them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Key {
    Up,
    Down,
    Left,
    Right,
    Start,
    Select,
    Back,
}

/// What a key did, as a sound: chosen once here, played by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Heard {
    Nothing,
    Change,
    Start,
    Invalid,
}

impl Heard {
    /// DeadSync has no cancel sound; going back plays the change, as
    /// everywhere else in the browser.
    pub(super) const fn sfx(self) -> Option<&'static str> {
        match self {
            Self::Nothing => None,
            Self::Change => Some(SFX_CHANGE),
            Self::Start => Some(SFX_START),
            Self::Invalid => Some(SFX_INVALID),
        }
    }
}

/// A direction held down: the rows, the meter's ends and the picker repeat
/// the way the pack list does.
#[derive(Clone, Copy, Debug)]
struct Hold {
    key: Key,
    held_for: f32,
    since: f32,
}

/// A line of news over the stage, as the original's toast: two seconds, then
/// a fade.
pub(super) struct Toast {
    pub(super) text: String,
    pub(super) at: f64,
}

/// Work for the shell, which owns the paths and calls the service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RipRequest {
    /// Rip a pack, leaving out these songs -- a re-rip's thrown-back cards.
    Rip {
        options: RipOptions,
        exclude: Vec<String>,
    },
    /// Keep the pack on the table.
    Keep,
    /// Throw the pack on the table away.
    Reset,
    /// Try the author list again after it failed.
    RetryAuthors,
}

/// One card's words, made once per rip rather than once a frame.
pub(super) struct CardText {
    pub(super) meter: u32,
    pub(super) meter_text: Arc<str>,
    pub(super) year: Option<Arc<str>>,
    /// `150 BPM`, and how fast the song plays, for its colour; nothing when
    /// SMO gives the song no tempo.
    pub(super) bpm: Option<(Arc<str>, Pace)>,
    pub(super) title: Arc<str>,
    /// `by <credit>`, or nothing when SMO credits nobody.
    pub(super) credit: Option<Arc<str>>,
    pub(super) pack: Arc<str>,
    /// The picture's key in the banner fetcher, and its texture's name.
    pub(super) art: Option<(u64, Arc<str>)>,
}

/// One row of the author picker, made once per published list.
pub(super) struct AuthorRow {
    pub(super) name: Arc<str>,
    /// `N popular`, beside a charter with packs on the most-played list.
    pub(super) note: Option<Arc<str>>,
}

/// The cards' titles, wrapped to two lines and cut with "..." past that, and
/// how wide the year and the BPM over each title are, so the two sit
/// centred as one pair.
///
/// Fitting needs the font, which only drawing has, so it is worked out the
/// first time a rip's cards are drawn and kept for as long as they are on
/// the table.
#[derive(Default)]
pub(super) struct TitleFit {
    songs: Option<Arc<[RipSong]>>,
    titles: Vec<(Arc<str>, usize)>,
    /// Each card's year and BPM widths in the font's own units; 0 for one
    /// the card does not show.
    year_lines: Vec<(f32, f32)>,
}

impl TitleFit {
    fn fits(&self, songs: &Arc<[RipSong]>) -> bool {
        self.songs
            .as_ref()
            .is_some_and(|fitted| Arc::ptr_eq(fitted, songs))
    }

    /// Card `card`'s fitted title and how many lines it takes, once fitted
    /// for the cards on the table.
    pub(super) fn get(&self, songs: &Arc<[RipSong]>, card: usize) -> Option<(&Arc<str>, usize)> {
        if !self.fits(songs) {
            return None;
        }
        self.titles.get(card).map(|(title, lines)| (title, *lines))
    }

    /// Card `card`'s year and BPM widths, once fitted for the cards on the
    /// table.
    pub(super) fn year_line(&self, songs: &Arc<[RipSong]>, card: usize) -> Option<(f32, f32)> {
        if !self.fits(songs) {
            return None;
        }
        self.year_lines.get(card).copied()
    }
}

/// Everything the tab remembers.
pub struct Rip {
    // --- the options ---------------------------------------------------------
    /// Ticked eras, in the order they were ticked. Shown and sent in the
    /// list's order.
    pub(super) eras: Vec<Era>,
    /// Charters chosen from the list, in the order they were picked.
    pub(super) chosen: Vec<String>,
    pub(super) typed: String,
    pub(super) avoid: String,
    pub(super) lo: u32,
    pub(super) hi: u32,
    pub(super) full: bool,

    // --- the cursor ----------------------------------------------------------
    /// Index into `Row::ALL`. Kept across leaving the tab and RESET.
    pub(super) row: usize,
    pub(super) choice: Choice,
    pub(super) picking: Option<Pick>,
    pub(super) pick_at: usize,
    pub(super) pick_top: usize,
    pub(super) prompt: Option<Prompt>,
    hold: Option<Hold>,

    // --- what the service says ----------------------------------------------
    pub(super) snapshot: Arc<RipSnapshot>,
    pub(super) authors: Arc<AuthorsSnapshot>,
    pub(super) author_rows: Vec<AuthorRow>,
    authors_revision: Option<u64>,
    pub(super) cards: Vec<CardText>,
    cards_of: Option<Arc<[RipSong]>>,
    pub(super) title_fit: RefCell<TitleFit>,

    // --- the stage's timeline ------------------------------------------------
    /// Seconds on the stage's clock, which runs while the browser is open.
    pub(super) now: f64,
    pub(super) stage: Stage,
    /// When the stage reached its phase.
    pub(super) phase_at: f64,
    seen_set: u64,
    seen_revision: Option<u64>,
    /// The rip whose cards are on the table, and when they arrived.
    opened_set: Option<u64>,
    pub(super) opened_at: f64,
    /// The rip the tear has been heard for.
    torn_for: Option<u64>,
    /// When each card's song came in, once the pack is kept.
    pub(super) landed: Vec<f64>,
    seen_states: Option<Arc<[SongState]>>,
    /// The browser has just been opened: what changed while it was shut
    /// happened long ago, as far as the stage is concerned.
    fresh: bool,

    // --- for the shell -------------------------------------------------------
    requests: Vec<RipRequest>,
    /// A request has gone out that the snapshot does not show yet.
    requested: bool,
    pub(super) toast: Option<Toast>,
    /// Boosters kept this session, by folder name, and how many songs each
    /// brought: the reload question counts them as songs, not packs.
    kept: Vec<(OsString, usize)>,
}

impl Default for Rip {
    fn default() -> Self {
        Self {
            eras: Vec::new(),
            chosen: Vec::new(),
            typed: String::new(),
            avoid: String::new(),
            lo: DEFAULT_LO,
            hi: DEFAULT_HI,
            full: false,
            row: 0,
            choice: Choice::Keep,
            picking: None,
            pick_at: 0,
            pick_top: 0,
            prompt: None,
            hold: None,
            snapshot: Arc::new(RipSnapshot::default()),
            authors: Arc::new(AuthorsSnapshot::default()),
            author_rows: Vec::new(),
            authors_revision: None,
            cards: Vec::new(),
            cards_of: None,
            title_fit: RefCell::new(TitleFit::default()),
            now: 0.0,
            stage: Stage::Idle,
            phase_at: LONG_AGO,
            seen_set: 0,
            seen_revision: None,
            opened_set: None,
            opened_at: LONG_AGO,
            torn_for: None,
            landed: Vec::new(),
            seen_states: None,
            fresh: false,
            requests: Vec::new(),
            requested: false,
            toast: None,
            kept: Vec::new(),
        }
    }
}

// --- the shell's side ---------------------------------------------------------

/// The rips, keeps and retries asked for since the last frame.
pub fn take_rip_requests(state: &mut State) -> Vec<RipRequest> {
    std::mem::take(&mut state.rip.requests)
}

/// A request the service refused, in its own words, as a toast.
pub fn rip_refused(state: &mut State, reason: String) {
    toast(&mut state.rip, reason);
}

/// The browser is open again: the options and the table stay, a picker or a
/// prompt left open is not still being asked, and whatever moved on the
/// stage while it was shut moved long ago.
pub(super) fn on_enter(rip: &mut Rip) {
    rip.picking = None;
    rip.prompt = None;
    rip.hold = None;
    rip.toast = None;
    // A request the shell never took was for a visit that is over.
    rip.requests.clear();
    rip.requested = false;
    settle(rip);
    rip.fresh = true;
}

/// Push every timeline far enough back that nothing on the stage is still
/// moving, and the tear is not heard again.
fn settle(rip: &mut Rip) {
    let past = rip.now - 10.0;
    rip.phase_at = rip.phase_at.min(past);
    rip.opened_at = rip.opened_at.min(past);
    for landed in &mut rip.landed {
        *landed = landed.min(past);
    }
    if rip.opened_set.is_some() {
        rip.torn_for = rip.opened_set;
    }
}

/// The frame's tick: the stage's clock, the caret, held keys, and the tear.
pub(super) fn update(state: &mut State, dt: f32) {
    let rip = &mut state.rip;
    rip.now += f64::from(dt);
    if let Some(prompt) = rip.prompt.as_mut() {
        prompt.caret += dt;
    }
    if rip
        .toast
        .as_ref()
        .is_some_and(|toast| toast_alpha(rip.now - toast.at).is_none())
    {
        rip.toast = None;
    }
    repeat_hold(state, dt);

    // The tear, out loud the moment the pack opens -- but only while the
    // stage is there to be looked at, as the original's pump only runs then.
    if !rip_showing(state) || state.rip.prompt.is_some() {
        return;
    }
    let rip = &mut state.rip;
    if let Some(elapsed) = opened_elapsed(rip)
        && rip.torn_for != rip.opened_set
    {
        rip.torn_for = rip.opened_set;
        if elapsed < TEAR_HEARD_WITHIN {
            state.pending_audio.push(AudioRequest::PlaySfx(
                crate::resources::CONTENT_BROWSER_TEAR_SFX,
            ));
        }
    }
}

/// The service's latest, read into what the stage draws from. Rebuilt only
/// when a revision moves.
pub(super) fn sync(rip: &mut Rip, snapshot: Arc<RipSnapshot>, authors: Arc<AuthorsSnapshot>) {
    rip.requested = false;
    if rip.authors_revision != Some(authors.revision) {
        rip.authors_revision = Some(authors.revision);
        rip.author_rows = authors
            .authors
            .iter()
            .map(|author| AuthorRow {
                name: Arc::from(author.name.as_str()),
                note: (author.popular > 0)
                    .then(|| Arc::from(crate::i18n::tr_fmt("ContentBrowser", "MessagePopular", &[("arg0", &(author.popular).to_string())]).to_string())),
            })
            .collect();
        rip.authors = authors;
        if rip.picking == Some(Pick::Authors) {
            clamp_pick(rip);
        }
    } else {
        rip.authors = authors;
    }
    if rip.seen_revision != Some(snapshot.revision) {
        rip.seen_revision = Some(snapshot.revision);
        observe(rip, &snapshot);
    }
    rip.snapshot = snapshot;
    rip.fresh = false;
}

/// What changed on the table, timed.
fn observe(rip: &mut Rip, snapshot: &RipSnapshot) {
    let at = if rip.fresh { LONG_AGO } else { rip.now };
    let stage = stage_of(snapshot);
    if stage != rip.stage || snapshot.set != rip.seen_set {
        rip.stage = stage;
        rip.seen_set = snapshot.set;
        rip.phase_at = at;
    }

    let current = rip
        .cards_of
        .as_ref()
        .is_some_and(|songs| Arc::ptr_eq(songs, &snapshot.songs));
    if !current {
        rip.cards = card_texts(&snapshot.songs);
        rip.cards_of = Some(Arc::clone(&snapshot.songs));
    }

    // The cards arrive: the pack opens, once per rip, with KEEP offered
    // first.
    if snapshot.phase == RipPhase::Ready && rip.opened_set != Some(snapshot.set) {
        rip.opened_set = Some(snapshot.set);
        rip.opened_at = at;
        rip.choice = Choice::Keep;
        rip.landed = vec![LONG_AGO; snapshot.songs.len()];
        rip.seen_states = None;
    }

    // Each kept song landing lights its card up, with a bump.
    if let Some(keep) = snapshot.keep_of_set() {
        for (card, state) in keep.songs.iter().enumerate() {
            let before = rip
                .seen_states
                .as_ref()
                .and_then(|states| states.get(card).copied())
                .unwrap_or(SongState::Waiting);
            if *state != before
                && matches!(state, SongState::Done | SongState::Failed)
                && let Some(landed) = rip.landed.get_mut(card)
            {
                *landed = at;
            }
        }
        rip.seen_states = Some(Arc::clone(&keep.songs));
    } else {
        rip.seen_states = None;
    }

    // A finished booster, remembered by folder so the way out counts it.
    if let Some(keep) = snapshot.keep.as_deref()
        && keep.phase == KeepPhase::Done
        && let Some(name) = keep
            .group_dir
            .as_deref()
            .and_then(std::path::Path::file_name)
        && !rip.kept.iter().any(|(kept, _)| kept == name)
    {
        rip.kept.push((name.to_owned(), keep.got));
    }
}

/// The stage's phase, from the service's.
pub(super) fn stage_of(snapshot: &RipSnapshot) -> Stage {
    match snapshot.phase {
        RipPhase::Idle => Stage::Idle,
        RipPhase::Searching => Stage::Searching,
        RipPhase::Failed => Stage::Failed,
        RipPhase::Ready => match snapshot.keep_of_set().map(|keep| keep.phase) {
            None => Stage::Choosing,
            Some(KeepPhase::Filling) => Stage::Filling,
            Some(KeepPhase::Done) => Stage::Done,
            Some(KeepPhase::Failed) => Stage::Failed,
        },
    }
}

fn card_texts(songs: &[RipSong]) -> Vec<CardText> {
    songs
        .iter()
        .take(BOOSTER_SONGS)
        .map(|song| CardText {
            meter: song.meter,
            meter_text: Arc::from(song.meter.to_string()),
            year: year_text(song).map(Arc::from),
            bpm: bpm_text(song).map(|text| (Arc::from(text), song.bpm.pace())),
            title: Arc::from(song.title.as_str()),
            credit: (!song.credit.is_empty()).then(|| Arc::from(crate::i18n::tr_fmt("ContentBrowser", "MessageBy", &[("arg0", &(song.credit).to_string())]).to_string())),
            pack: Arc::from(song.pack_name.as_str()),
            art: song.art_url.as_deref().map(|url| {
                let key = song_art_key(url);
                (key, Arc::from(banner_key(key)))
            }),
        })
        .collect()
}

/// The year a card shows, when something dates the song.
fn year_text(song: &RipSong) -> Option<String> {
    song.year
        .filter(|year| *year > 0)
        .map(|year| year.to_string())
}

/// The BPM a card shows -- `150 BPM`, `150-190 BPM`, `??? BPM` -- when SMO
/// gives the song a tempo.
fn bpm_text(song: &RipSong) -> Option<String> {
    (!song.bpm.text.is_empty()).then(|| format!("{} BPM", song.bpm.text))
}

/// The cards' pictures, for the banner fetcher.
pub(super) fn wanted_art(rip: &Rip, out: &mut Vec<(u64, String)>) {
    for (song, card) in rip.snapshot.songs.iter().zip(&rip.cards) {
        if let (Some(url), Some((key, _))) = (song.art_url.as_deref(), card.art.as_ref()) {
            out.push((*key, url.to_owned()));
        }
    }
}

/// Songs a booster kept this session brought into this folder, if it is one.
pub(super) fn kept_songs(rip: &Rip, folder: &std::ffi::OsStr) -> Option<usize> {
    rip.kept
        .iter()
        .find(|(name, _)| name == folder)
        .map(|(_, got)| *got)
}

/// Whether a kept rip is still coming in, which holds the reload question
/// back as any other install does.
pub(super) fn keeping(rip: &Rip) -> bool {
    rip.snapshot.keeping()
}

// --- what the stage is --------------------------------------------------------

/// Searching or filling: the button says so, and will not rip again.
pub(super) fn busy(rip: &Rip) -> bool {
    matches!(rip.stage, Stage::Searching | Stage::Filling)
        || rip.snapshot.keeping()
        || rip.requested
}

/// Seconds since the cards on the table arrived, while they are on it.
pub(super) fn opened_elapsed(rip: &Rip) -> Option<f64> {
    (matches!(rip.stage, Stage::Choosing | Stage::Filling | Stage::Done)
        && rip.opened_set == Some(rip.snapshot.set))
    .then_some(rip.now - rip.opened_at)
}

/// The kept rip on the table, if it was kept.
pub(super) fn keep(rip: &Rip) -> Option<&KeepState> {
    rip.snapshot.keep_of_set()
}

/// A card's song, once the pack is kept.
pub(super) fn song_state(rip: &Rip, card: usize) -> Option<SongState> {
    keep(rip).and_then(|keep| keep.songs.get(card).copied())
}

/// The options a rip is made with.
pub(super) fn options(rip: &Rip) -> RipOptions {
    RipOptions {
        eras: eras_in_order(rip).collect(),
        authors: merge_names(&rip.chosen, &rip.typed),
        avoid: split_names(&rip.avoid),
        lo: rip.lo,
        hi: rip.hi,
        full: rip.full,
    }
}

/// The ticked eras in the list's order, rather than the order ticked.
pub(super) fn eras_in_order(rip: &Rip) -> impl Iterator<Item = Era> + '_ {
    Era::ALL.into_iter().filter(|era| rip.eras.contains(era))
}

/// The pack's gold cards: the one whose Expert is hardest, or each of those
/// tied for it while there are three or fewer. More than that and the top
/// meter is simply what the pack is made of, so no card is marked. Failed
/// songs count toward the top, and are never gold.
pub(super) fn gold_cards(meters: &[u32], failed: &[bool]) -> [bool; BOOSTER_SONGS] {
    let mut gold = [false; BOOSTER_SONGS];
    let Some(top) = meters.iter().copied().max() else {
        return gold;
    };
    let count = meters.iter().filter(|meter| **meter == top).count();
    if count > 3 {
        return gold;
    }
    for (card, meter) in meters.iter().enumerate().take(BOOSTER_SONGS) {
        gold[card] = *meter == top && !failed.get(card).copied().unwrap_or(false);
    }
    gold
}

/// The gold cards of the rip on the table.
pub(super) fn table_gold(rip: &Rip) -> [bool; BOOSTER_SONGS] {
    let mut meters = [0u32; BOOSTER_SONGS];
    let mut failed = [false; BOOSTER_SONGS];
    let count = rip.cards.len().min(BOOSTER_SONGS);
    for (card, text) in rip.cards.iter().take(count).enumerate() {
        meters[card] = text.meter;
        failed[card] = song_state(rip, card) == Some(SongState::Failed);
    }
    gold_cards(&meters[..count], &failed[..count])
}

// --- the words ----------------------------------------------------------------

/// What a row says its value is, verbatim from the original.
pub(super) fn row_value(rip: &Rip, row: Row) -> Cow<'static, str> {
    match row {
        Row::Era => {
            let mut names = eras_in_order(rip).map(Era::short);
            let Some(first) = names.next() else {
                return Cow::Borrowed("Any era  (START to choose)");
            };
            let mut text = first.to_owned();
            for name in names {
                text.push_str(", ");
                text.push_str(name);
            }
            Cow::Owned(text)
        }
        Row::Author => match rip.chosen.as_slice() {
            [one] => Cow::Owned(one.clone()),
            [a, b] => Cow::Owned(format!("{a}, {b}")),
            [a, b, rest @ ..] => Cow::Owned(crate::i18n::tr_fmt("ContentBrowser", "MessageABMore", &[("a", &(a).to_string()), ("b", &(b).to_string()), ("arg0", &(rest.len()).to_string())]).to_string()),
            [] if rip.author_rows.is_empty() && rip.authors.phase == AuthorsPhase::Loading => {
                Cow::Borrowed("loading the list...")
            }
            [] if rip.author_rows.is_empty() && rip.authors.phase == AuthorsPhase::Error => {
                Cow::Borrowed("list unavailable  (START to retry)")
            }
            [] => Cow::Borrowed("Anyone  (START to choose)"),
        },
        Row::Typed if rip.typed.is_empty() => Cow::Borrowed("none  (START to type exact names)"),
        Row::Typed => Cow::Owned(rip.typed.clone()),
        Row::Avoid if rip.avoid.is_empty() => Cow::Borrowed("nobody  (START to type exact names)"),
        Row::Avoid => Cow::Owned(rip.avoid.clone()),
        Row::Lo => Cow::Owned(rip.lo.to_string()),
        Row::Hi => Cow::Owned(rip.hi.to_string()),
        Row::Full if rip.full => Cow::Borrowed("Yes: Beginner to Challenge"),
        Row::Full => Cow::Borrowed("No"),
        Row::Button | Row::Reset => Cow::Borrowed(""),
    }
}

/// A row whose value is "anyone": drawn grey rather than white.
pub(super) fn row_muted(rip: &Rip, row: Row) -> bool {
    match row {
        Row::Author => rip.chosen.is_empty(),
        Row::Era => rip.eras.is_empty(),
        Row::Typed => rip.typed.is_empty(),
        Row::Avoid => rip.avoid.is_empty(),
        _ => false,
    }
}

/// What the pack says it will be made of, along its foot. It reads the live
/// options, as the original's does.
pub(super) fn recipe(rip: &Rip) -> String {
    let mut text = String::with_capacity(64);
    let ticked = eras_in_order(rip).count();
    if ticked == 0 || ticked == Era::ALL.len() {
        text.push_str("ANY ERA");
    } else {
        for (index, era) in eras_in_order(rip).enumerate() {
            if index > 0 {
                text.push_str(" + ");
            }
            text.push_str(&era.short().to_uppercase());
        }
    }
    text.push_str("  -  EXPERT ");
    text.push_str(&rip.lo.to_string());
    if rip.hi != rip.lo {
        text.push('-');
        text.push_str(&rip.hi.to_string());
    }
    let who = merge_names(&rip.chosen, &rip.typed);
    match who.as_slice() {
        [] => {}
        [one] => {
            text.push_str("  -  ");
            text.push_str(&one.to_uppercase());
        }
        many => {
            text.push_str("  -  ");
            text.push_str(&crate::i18n::tr_fmt("ContentBrowser", "MessageAUTHORS", &[("arg0", &(many.len()).to_string())]).to_string());
        }
    }
    if rip.full {
        text.push_str("  -  FULL");
    }
    text
}

/// The line over the stage.
pub(super) fn heading(rip: &Rip) -> Cow<'static, str> {
    match rip.stage {
        Stage::Searching => Cow::Borrowed("LOOKING FOR SONGS..."),
        Stage::Choosing => Cow::Borrowed("KEEP THIS PACK?"),
        Stage::Filling | Stage::Done => match keep(rip) {
            Some(keep) if !keep.folder.is_empty() => Cow::Owned(keep.folder.to_uppercase()),
            _ => Cow::Borrowed("RIP A BOOSTER PACK"),
        },
        Stage::Failed => Cow::Borrowed("NO PACK THIS TIME"),
        Stage::Idle => Cow::Borrowed("RIP A BOOSTER PACK"),
    }
}

/// The line under the stage, and whether it is the red of a failure.
pub(super) fn under_line(rip: &Rip) -> (Cow<'static, str>, bool) {
    let songs = rip.snapshot.songs.len();
    let short = rip.snapshot.short;
    let text = match rip.stage {
        Stage::Searching => Cow::Owned(crate::i18n::tr_fmt("ContentBrowser", "MessageSearchingStepmaniaonlineNetForSongsThatFitS", &[("arg0", &((rip.now - rip.phase_at).max(0.0).floor()).to_string())]).to_string()),
        Stage::Choosing => {
            let total: u64 = rip.snapshot.songs.iter().map(|song| song.bytes).sum();
            let only = if short {
                crate::i18n::tr_fmt("ContentBrowser", "MessageOnlySongsMatched", &[("songs", &(songs).to_string())]).to_string()
            } else {
                String::new()
            };
            Cow::Owned(crate::i18n::tr_fmt("ContentBrowser", "MessageSongsSongsOnlyKEEPDownloadsThemRERIPBrings", &[("songs", &(songs).to_string()), ("arg0", &(rip_bytes(total)).to_string()), ("only", &(only).to_string())]).to_string())
        }
        Stage::Filling => match keep(rip) {
            Some(keep) => {
                // Nothing in yet reads as the whole to fetch: there are no
                // words for none.
                let fetched = if keep.done_bytes > 0 {
                    crate::i18n::tr_fmt("ContentBrowser", "MessageOf", &[("arg0", &(rip_bytes(keep.done_bytes)).to_string()), ("arg1", &(rip_bytes(keep.total_bytes)).to_string())]).to_string()
                } else {
                    crate::i18n::tr_fmt("ContentBrowser", "MessageToFetch", &[("arg0", &(rip_bytes(keep.total_bytes)).to_string())]).to_string()
                };
                Cow::Owned(crate::i18n::tr_fmt("ContentBrowser", "MessageOfSongsSongsInFetched", &[("arg0", &(keep.got + keep.failed).to_string()), ("songs", &(songs).to_string()), ("fetched", &(fetched).to_string())]).to_string())
            }
            None => Cow::Borrowed(IDLE_LINE),
        },
        Stage::Done => {
            let got = keep(rip).map_or(0, |keep| keep.got);
            let only = if short {
                crate::i18n::tr_fmt("ContentBrowser", "MessageOnlySongsMatchedeffe01", &[("songs", &(songs).to_string())]).to_string()
            } else {
                String::new()
            };
            // Every booster is NULL-synced: DeadSync reads no Pack.ini sync
            // unless the machine says to, and its single songs work this way.
            Cow::Owned(crate::i18n::tr_fmt("ContentBrowser", "MessageGotSongsAddedSyncedNULLOnlyReloadSongsWhen", &[("got", &(got).to_string()), ("only", &(only).to_string())]).to_string())
        }
        Stage::Failed => {
            let message = match keep(rip) {
                Some(keep) => Some(
                    keep.message
                        .clone()
                        .unwrap_or_else(|| crate::i18n::tr("ContentBrowser", "MessageNoneOfTheSongsCouldBeDownloaded").to_string()),
                ),
                None => rip.snapshot.message.clone(),
            };
            Cow::Owned(message.unwrap_or_else(|| crate::i18n::tr("ContentBrowser", "MessageTheRipDidNotWork").to_string()))
        }
        Stage::Idle => Cow::Borrowed(IDLE_LINE),
    };
    (text, rip.stage == Stage::Failed)
}

const IDLE_LINE: &str =
    "15 random songs within the limits on the left, made into a pack of their own";

/// The original's `FormatBytes`, which has no words for nothing.
fn rip_bytes(bytes: u64) -> String {
    if bytes == 0 {
        String::new()
    } else {
        super::render::format_bytes(bytes)
    }
}

/// What the button says.
pub(super) fn button_label(rip: &Rip) -> Cow<'static, str> {
    match (rip.stage, keep(rip)) {
        (Stage::Filling, Some(keep)) => Cow::Owned(crate::i18n::tr_fmt("ContentBrowser", "MessageDOWNLOADING", &[("arg0", &(keep.got + keep.failed).to_string()), ("arg1", &(rip.snapshot.songs.len()).to_string())]).to_string()),
        (Stage::Searching, _) => Cow::Borrowed("RIPPING..."),
        _ => Cow::Borrowed("RIP A PACK"),
    }
}

/// How full the button is while a kept pack comes in.
pub(super) fn fill_fraction(rip: &Rip) -> Option<f32> {
    let keep = keep(rip).filter(|keep| keep.filling())?;
    if keep.total_bytes == 0 {
        return Some(0.0);
    }
    Some((keep.done_bytes as f64 / keep.total_bytes as f64).clamp(0.0, 1.0) as f32)
}

/// Under the button: what a booster is, and how many have been ripped.
pub(super) fn info_text(rip: &Rip) -> Cow<'static, str> {
    const WHAT: &str = "15 random songs, each from a different pack on stepmaniaonline.net, as a new pack in your library.";
    match rip.snapshot.kept_count {
        Some(count) if count > 0 => Cow::Owned(crate::i18n::tr_fmt("ContentBrowser", "MessageWHATPacksRippedSoFarCount", &[("WHAT", &super::localization::text(&(WHAT).to_string()).to_string()), ("count", &(count).to_string())]).to_string()),
        _ => Cow::Borrowed(WHAT),
    }
}

/// The toast's alpha this long after it was raised, or `None` once it has
/// gone: two seconds held, then four tenths fading.
pub(super) fn toast_alpha(age: f64) -> Option<f32> {
    if !(0.0..2.4).contains(&age) {
        return None;
    }
    Some(if age < 2.0 {
        1.0
    } else {
        (1.0 - (age - 2.0) / 0.4) as f32
    })
}

fn toast(rip: &mut Rip, text: impl Into<String>) {
    rip.toast = Some(Toast {
        text: text.into(),
        at: rip.now,
    });
}

// --- the keys -----------------------------------------------------------------

/// Every key the tab's body answers to, below the tab row. `first` is false
/// for a held key's repeat.
pub(super) fn press(state: &mut State, key: Key, first: bool) -> Heard {
    if state.rip.picking.is_some() {
        return press_picker(&mut state.rip, key, first);
    }
    let row = Row::ALL[state.rip.row.min(Row::ALL.len() - 1)];
    match key {
        Key::Back => {
            if !first {
                return Heard::Nothing;
            }
            state.zone = Zone::Tabs;
            state.rip.hold = None;
            Heard::Change
        }
        Key::Up => {
            if state.rip.row > 0 {
                state.rip.row -= 1;
            } else if first {
                state.zone = Zone::Tabs;
                state.rip.hold = None;
            }
            // Always the change, even held against the top row.
            Heard::Change
        }
        Key::Down => {
            if state.rip.row + 1 < Row::ALL.len() {
                state.rip.row += 1;
                Heard::Change
            } else if first {
                Heard::Invalid
            } else {
                Heard::Nothing
            }
        }
        Key::Left | Key::Right => step(&mut state.rip, row, key == Key::Right, first),
        Key::Start if first => start(&mut state.rip, row),
        Key::Select if first => select(&mut state.rip, row),
        Key::Start | Key::Select => Heard::Nothing,
    }
}

/// LEFT and RIGHT on a row. The meter's ends repeat while held, and say so
/// even at their limits; a yes-or-no does not repeat.
fn step(rip: &mut Rip, row: Row, right: bool, first: bool) -> Heard {
    match row {
        Row::Lo => {
            let lo = if right {
                rip.lo + 1
            } else {
                rip.lo.saturating_sub(1)
            };
            rip.lo = lo.clamp(METER_FLOOR, rip.hi.max(METER_FLOOR));
            Heard::Change
        }
        Row::Hi => {
            let hi = if right {
                rip.hi + 1
            } else {
                rip.hi.saturating_sub(1)
            };
            rip.hi = hi.clamp(rip.lo.min(METER_CEILING), METER_CEILING);
            Heard::Change
        }
        Row::Full if first => {
            rip.full = !rip.full;
            Heard::Change
        }
        Row::Button if rip.stage == Stage::Choosing && first => {
            rip.choice = match rip.choice {
                Choice::Keep => Choice::ReRip,
                Choice::ReRip => Choice::Keep,
            };
            Heard::Change
        }
        Row::Full | Row::Button if !first => Heard::Nothing,
        _ if first => Heard::Invalid,
        _ => Heard::Nothing,
    }
}

fn start(rip: &mut Rip, row: Row) -> Heard {
    match row {
        Row::Era => {
            open_picker(rip, Pick::Eras);
            Heard::Start
        }
        Row::Author => {
            if !rip.author_rows.is_empty() {
                open_picker(rip, Pick::Authors);
                Heard::Start
            } else if rip.authors.phase == AuthorsPhase::Error {
                rip.requests.push(RipRequest::RetryAuthors);
                Heard::Start
            } else {
                Heard::Invalid
            }
        }
        Row::Typed => open_prompt(rip, PromptKind::Typed),
        Row::Avoid => open_prompt(rip, PromptKind::Avoid),
        Row::Lo => open_prompt(rip, PromptKind::Lo),
        Row::Hi => open_prompt(rip, PromptKind::Hi),
        Row::Full => {
            rip.full = !rip.full;
            Heard::Change
        }
        Row::Reset => {
            if rip.snapshot.keeping() {
                toast(rip, "a pack is still downloading");
                return Heard::Invalid;
            }
            reset(rip);
            Heard::Change
        }
        Row::Button => {
            let choosing = rip.stage == Stage::Choosing;
            if choosing && guarded(rip) {
                return Heard::Nothing;
            }
            if choosing && rip.choice == Choice::Keep {
                rip.requests.push(RipRequest::Keep);
                rip.requested = true;
                return Heard::Start;
            }
            if busy(rip) {
                toast(rip, "a pack is still being ripped");
                return Heard::Invalid;
            }
            // A re-rip leaves out the songs it throws back; a rip from a
            // finished or failed pack leaves out nothing.
            let exclude = if choosing {
                rip.snapshot
                    .songs
                    .iter()
                    .map(|song| song.key.clone())
                    .collect()
            } else {
                Vec::new()
            };
            rip.requests.push(RipRequest::Rip {
                options: options(rip),
                exclude,
            });
            rip.requested = true;
            rip.phase_at = rip.now;
            Heard::Start
        }
    }
}

/// The second after the cards land, when START is not yet a choice.
pub(super) fn guarded(rip: &Rip) -> bool {
    rip.now - rip.phase_at < DECIDE_GUARD
}

/// Every option as the tab first opens, and the pack unopened again. The
/// cursor, the author list and the history stay.
fn reset(rip: &mut Rip) {
    rip.typed.clear();
    rip.avoid.clear();
    rip.lo = DEFAULT_LO;
    rip.hi = DEFAULT_HI;
    rip.full = false;
    rip.chosen.clear();
    rip.eras.clear();
    rip.picking = None;
    rip.choice = Choice::Keep;
    rip.phase_at = rip.now;
    rip.requests.push(RipRequest::Reset);
    rip.requested = true;
}

/// SELECT clears the row the cursor is on back to "anyone".
fn select(rip: &mut Rip, row: Row) -> Heard {
    match row {
        Row::Era if !rip.eras.is_empty() => rip.eras.clear(),
        Row::Typed if !rip.typed.is_empty() => rip.typed.clear(),
        Row::Avoid if !rip.avoid.is_empty() => rip.avoid.clear(),
        Row::Author if !rip.chosen.is_empty() => rip.chosen.clear(),
        Row::Author if rip.authors.phase == AuthorsPhase::Error => {
            rip.requests.push(RipRequest::RetryAuthors);
            return Heard::Start;
        }
        _ => return Heard::Invalid,
    }
    Heard::Change
}

/// Note a direction going down, for the repeat.
pub(super) fn hold(state: &mut State, key: Key) {
    if matches!(key, Key::Up | Key::Down | Key::Left | Key::Right) {
        state.rip.hold = Some(Hold {
            key,
            held_for: 0.0,
            since: 0.0,
        });
    }
}

/// A direction let go.
pub(super) fn release(state: &mut State, key: Key) {
    if state.rip.hold.is_some_and(|hold| hold.key == key) {
        state.rip.hold = None;
    }
}

/// A held direction repeats, as the pack list's does. The sound goes out with
/// the shell's audio, since a repeat has no key press to carry it.
fn repeat_hold(state: &mut State, dt: f32) {
    if state.zone != Zone::Rip || state.rip.prompt.is_some() || !rip_showing(state) {
        state.rip.hold = None;
        return;
    }
    let Some(hold) = state.rip.hold.as_mut() else {
        return;
    };
    hold.held_for += dt;
    hold.since += dt;
    if hold.held_for < NAV_INITIAL_HOLD_DELAY.as_secs_f32()
        || hold.since < NAV_REPEAT_INTERVAL.as_secs_f32()
    {
        return;
    }
    hold.since = 0.0;
    let key = hold.key;
    if let Some(path) = press(state, key, false).sfx() {
        state.pending_audio.push(AudioRequest::PlaySfx(path));
    }
}

// --- the picker ---------------------------------------------------------------

/// How many rows the open picker has.
pub(super) fn pick_count(rip: &Rip) -> usize {
    match rip.picking {
        Some(Pick::Eras) => Era::ALL.len(),
        Some(Pick::Authors) => rip.author_rows.len(),
        None => 0,
    }
}

/// Whether the picker's row `at` is ticked.
pub(super) fn pick_ticked(rip: &Rip, at: usize) -> bool {
    match rip.picking {
        Some(Pick::Eras) => Era::ALL.get(at).is_some_and(|era| rip.eras.contains(era)),
        Some(Pick::Authors) => rip
            .author_rows
            .get(at)
            .is_some_and(|row| rip.chosen.iter().any(|name| **name == *row.name)),
        None => false,
    }
}

/// How many are ticked in the open picker.
pub(super) fn pick_chosen(rip: &Rip) -> usize {
    match rip.picking {
        Some(Pick::Eras) => rip.eras.len(),
        Some(Pick::Authors) | None => rip.chosen.len(),
    }
}

/// Open a list over the tab on its first ticked row, or its first row.
fn open_picker(rip: &mut Rip, pick: Pick) {
    rip.picking = Some(pick);
    rip.pick_at = match pick {
        Pick::Eras => rip
            .eras
            .first()
            .and_then(|first| Era::ALL.iter().position(|era| era == first)),
        Pick::Authors => rip
            .chosen
            .first()
            .and_then(|first| rip.author_rows.iter().position(|row| *row.name == **first)),
    }
    .unwrap_or(0);
    clamp_pick(rip);
    let count = pick_count(rip);
    rip.pick_top = rip
        .pick_at
        .saturating_sub(lo::RIP_PICK_ROWS / 2)
        .min(count.saturating_sub(lo::RIP_PICK_ROWS));
}

/// Keep the picker's cursor on a row and its window on the cursor, however
/// the list has changed under it.
fn clamp_pick(rip: &mut Rip) {
    let count = pick_count(rip);
    rip.pick_at = rip.pick_at.min(count.saturating_sub(1));
    if rip.pick_at < rip.pick_top {
        rip.pick_top = rip.pick_at;
    }
    if rip.pick_at >= rip.pick_top + lo::RIP_PICK_ROWS {
        rip.pick_top = rip.pick_at + 1 - lo::RIP_PICK_ROWS;
    }
    rip.pick_top = rip.pick_top.min(count.saturating_sub(lo::RIP_PICK_ROWS));
}

/// The open list: UP and DOWN move through it, LEFT and RIGHT a screenful at
/// a time, START ticks or unticks, SELECT unticks them all, BACK closes it.
fn press_picker(rip: &mut Rip, key: Key, first: bool) -> Heard {
    let count = pick_count(rip);
    match key {
        Key::Back => {
            if !first {
                return Heard::Nothing;
            }
            rip.picking = None;
            rip.hold = None;
            Heard::Change
        }
        Key::Up | Key::Down | Key::Left | Key::Right => {
            let step: isize = match key {
                Key::Up => -1,
                Key::Down => 1,
                Key::Left => -(lo::RIP_PICK_ROWS as isize),
                _ => lo::RIP_PICK_ROWS as isize,
            };
            let last = count.max(1) as isize - 1;
            let at = (rip.pick_at as isize + step).clamp(0, last) as usize;
            if at == rip.pick_at {
                return if first {
                    Heard::Invalid
                } else {
                    Heard::Nothing
                };
            }
            rip.pick_at = at;
            clamp_pick(rip);
            Heard::Change
        }
        Key::Start if first => {
            let at = rip.pick_at;
            match rip.picking {
                Some(Pick::Eras) => {
                    let Some(&era) = Era::ALL.get(at) else {
                        return Heard::Nothing;
                    };
                    match rip.eras.iter().position(|have| *have == era) {
                        Some(index) => {
                            rip.eras.remove(index);
                        }
                        None => rip.eras.push(era),
                    }
                }
                Some(Pick::Authors) => {
                    let Some(name) = rip.author_rows.get(at).map(|row| Arc::clone(&row.name))
                    else {
                        return Heard::Nothing;
                    };
                    match rip.chosen.iter().position(|have| **have == *name) {
                        Some(index) => {
                            rip.chosen.remove(index);
                        }
                        None => rip.chosen.push(name.to_string()),
                    }
                }
                None => return Heard::Nothing,
            }
            Heard::Change
        }
        Key::Select if first => {
            let set_empty = match rip.picking {
                Some(Pick::Eras) => rip.eras.is_empty(),
                _ => rip.chosen.is_empty(),
            };
            if set_empty {
                return Heard::Invalid;
            }
            match rip.picking {
                Some(Pick::Eras) => rip.eras.clear(),
                _ => rip.chosen.clear(),
            }
            Heard::Change
        }
        Key::Start | Key::Select => Heard::Nothing,
    }
}

// --- the prompt ---------------------------------------------------------------

fn open_prompt(rip: &mut Rip, kind: PromptKind) -> Heard {
    let text = match kind {
        PromptKind::Typed => rip.typed.clone(),
        PromptKind::Avoid => rip.avoid.clone(),
        PromptKind::Lo => rip.lo.to_string(),
        PromptKind::Hi => rip.hi.to_string(),
    };
    rip.prompt = Some(Prompt {
        kind,
        text,
        caret: 0.0,
    });
    rip.hold = None;
    Heard::Start
}

/// Take what was typed. A meter that is not a number is no answer at all; a
/// number is held to 1..30, and the other end moves out of its way rather
/// than refusing it.
pub(super) fn accept_prompt(rip: &mut Rip) {
    let Some(prompt) = rip.prompt.take() else {
        return;
    };
    let answer = prompt.text.trim();
    match prompt.kind {
        PromptKind::Typed => rip.typed = answer.to_owned(),
        PromptKind::Avoid => rip.avoid = answer.to_owned(),
        PromptKind::Lo | PromptKind::Hi => {
            let Ok(value) = answer.parse::<u32>() else {
                return;
            };
            let value = value.clamp(METER_FLOOR, METER_CEILING);
            if prompt.kind == PromptKind::Lo {
                rip.lo = value;
                if rip.lo > rip.hi {
                    rip.hi = rip.lo;
                }
            } else {
                rip.hi = value;
                if rip.lo > rip.hi {
                    rip.lo = rip.hi;
                }
            }
        }
    }
}

/// Close the prompt, applying nothing.
pub(super) fn cancel_prompt(rip: &mut Rip) {
    rip.prompt = None;
}

/// The pad's keys while the prompt is up: START takes the answer, BACK
/// leaves it.
pub(super) fn press_prompt(rip: &mut Rip, key: Key) -> Heard {
    match key {
        Key::Start => {
            accept_prompt(rip);
            Heard::Start
        }
        Key::Back => {
            cancel_prompt(rip);
            Heard::Change
        }
        _ => Heard::Nothing,
    }
}

/// Typed characters into the prompt, up to its limit. A meter's prompt takes
/// digits only. Whether anything went in.
pub(super) fn prompt_type(rip: &mut Rip, text: &str) -> bool {
    let Some(prompt) = rip.prompt.as_mut() else {
        return false;
    };
    let room = prompt
        .kind
        .max_chars()
        .saturating_sub(prompt.text.chars().count());
    let numeric = prompt.kind.numeric();
    let before = prompt.text.len();
    prompt.text.extend(
        text.chars()
            .filter(|ch| !ch.is_control() && (!numeric || ch.is_ascii_digit()))
            .take(room),
    );
    let changed = prompt.text.len() != before;
    if changed {
        prompt.caret = 0.0;
    }
    changed
}

/// Backspace in the prompt. Whether anything went.
pub(super) fn prompt_backspace(rip: &mut Rip) -> bool {
    let Some(prompt) = rip.prompt.as_mut() else {
        return false;
    };
    let gone = prompt.text.pop().is_some();
    if gone {
        prompt.caret = 0.0;
    }
    gone
}

// --- motion -------------------------------------------------------------------

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// An ease-out quadratic.
pub(super) fn ease(p: f32) -> f32 {
    1.0 - (1.0 - p) * (1.0 - p)
}

/// An ease that overshoots and settles back, about 7% past at 0.61.
pub(super) fn ease_back(p: f32) -> f32 {
    const S: f32 = 1.4;
    let q = p - 1.0;
    q * q * ((S + 1.0) * q + S) + 1.0
}

/// One card's place on the stage this long after the pack opened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CardMotion {
    /// Dealt yet: a card appears the moment its deal starts.
    pub(super) shown: bool,
    pub(super) x: f32,
    pub(super) y: f32,
    /// Degrees, clockwise, as StepMania turns.
    pub(super) rot: f32,
    pub(super) sx: f32,
    pub(super) sy: f32,
    /// Turned over: the face shows from halfway through the flip.
    pub(super) face_up: bool,
}

/// Where card `card` is `elapsed` seconds after the pack opened, `bump`
/// bigger than usual. Cards fly out of the pack's centre one at a time,
/// overshooting their place and settling, then turn over.
pub(super) fn card_motion(card: usize, elapsed: f32, bump: f32) -> CardMotion {
    let start = DEAL_AT + card as f32 * DEAL_EACH;
    let dealt = clamp01((elapsed - start) / DEAL_TIME);
    let (to_x, to_y) = lo::rip_card_xy(card);
    let k = ease_back(dealt);
    // The original counts from one: its even cards lean right.
    let lean = if (card + 1).is_multiple_of(2) {
        14.0
    } else {
        -14.0
    };
    let flip = clamp01((elapsed - start - DEAL_TIME - FLIP_AFTER) / FLIP_TIME);
    let size = (0.6 + 0.4 * dealt) * (1.0 + bump);
    CardMotion {
        shown: elapsed >= start,
        x: lo::RIP_STAGE_X + (to_x - lo::RIP_STAGE_X) * k,
        y: lo::RIP_STAGE_Y + (to_y - lo::RIP_STAGE_Y) * k,
        rot: (1.0 - dealt) * lean,
        sx: (flip * std::f32::consts::PI).cos().abs() * size,
        sy: size,
        face_up: flip >= 0.5,
    }
}

/// The bump a card gives `since` seconds after its song landed.
pub(super) fn bump(since: f32) -> f32 {
    if !(0.0..BUMP_TIME).contains(&since) {
        return 0.0;
    }
    0.07 * (since / BUMP_TIME * std::f32::consts::PI).sin()
}

/// The pack's whole pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PackMotion {
    pub(super) shown: bool,
    pub(super) x: f32,
    pub(super) y: f32,
    /// Degrees, clockwise.
    pub(super) rot: f32,
    /// The halo it has while the songs are looked for.
    pub(super) glow: f32,
    pub(super) body_zoom: f32,
    pub(super) body_alpha: f32,
    pub(super) body_y: f32,
    pub(super) strip_y: f32,
    pub(super) strip_rot: f32,
    pub(super) strip_alpha: f32,
    /// The shine across it: its x within the pack, and its alpha.
    pub(super) sheen: Option<(f32, f32)>,
    /// The burst of light when it tears.
    pub(super) flash: f32,
}

/// Where the strip rests on an unopened pack.
const STRIP_REST_Y: f32 = -lo::RIP_PACK_H / 2.0 + 10.0;

/// The unopened pack: a bob and a tilt at rest, and a shake that grows over
/// twelve seconds while songs are looked for. `t` is the clock.
pub(super) fn idle_pack(t: f64, searching: bool, waited: f64) -> PackMotion {
    let wave = |rate: f64| (t * rate).sin() as f32;
    let bob = wave(1.6) * 4.0;
    let (rot, x, glow) = if searching {
        (
            wave(38.0) * (1.5 + waited.clamp(0.0, 12.0) as f32 * 0.25),
            wave(53.0) * 1.5,
            0.55 + 0.25 * wave(7.0),
        )
    } else {
        (wave(0.9) * 2.0, 0.0, 0.0)
    };
    // The shine runs across now and then; faster while it waits.
    let cycle = if searching { 1.1 } else { 2.6 };
    let p = (t.rem_euclid(cycle) / cycle * 1.6) as f32;
    let sheen = (p <= 1.0).then(|| {
        (
            -lo::RIP_PACK_W / 2.0 + 13.0 + p * (lo::RIP_PACK_W - 26.0),
            0.32 * (p * std::f32::consts::PI).sin(),
        )
    });
    PackMotion {
        shown: true,
        x: lo::RIP_STAGE_X + x,
        y: lo::RIP_STAGE_Y + bob,
        rot,
        glow,
        body_zoom: 1.0,
        body_alpha: 1.0,
        body_y: 0.0,
        strip_y: STRIP_REST_Y,
        strip_rot: 0.0,
        strip_alpha: 1.0,
        sheen,
        flash: 0.0,
    }
}

/// The pack tearing open, `elapsed` seconds after it did: the strip flies
/// off, the body sinks and fades, and a flash goes off over it.
pub(super) fn torn_pack(elapsed: f32) -> PackMotion {
    let tp = clamp01(elapsed / 0.45);
    let bp = clamp01((elapsed - 0.25) / 0.5);
    let flash = if elapsed < 0.1 {
        elapsed.max(0.0) / 0.1 * 0.7
    } else if elapsed < 0.55 {
        0.7 * (1.0 - (elapsed - 0.1) / 0.45)
    } else {
        0.0
    };
    PackMotion {
        shown: bp < 1.0,
        x: lo::RIP_STAGE_X,
        y: lo::RIP_STAGE_Y,
        rot: 0.0,
        glow: 0.0,
        body_zoom: 1.0 - 0.15 * bp,
        body_alpha: 1.0 - bp,
        body_y: 40.0 * ease(bp),
        strip_y: STRIP_REST_Y - 70.0 * ease(tp),
        strip_rot: -28.0 * tp,
        strip_alpha: 1.0 - tp,
        sheen: None,
        flash,
    }
}

/// The pack as the stage stands now.
pub(super) fn pack_motion(rip: &Rip) -> PackMotion {
    match opened_elapsed(rip) {
        Some(elapsed) => torn_pack(elapsed as f32),
        None => idle_pack(
            rip.now,
            rip.stage == Stage::Searching,
            rip.now - rip.phase_at,
        ),
    }
}

/// The gold card's glint: its x within the card and its alpha, now and then.
pub(super) fn shine(t: f64, card: usize) -> Option<(f32, f32)> {
    let p = ((t + (card + 1) as f64 * 0.37).rem_euclid(2.4) / 2.4 * 1.5) as f32;
    (p <= 1.0).then(|| {
        (
            -lo::RIP_CARD_W / 2.0 + 9.0 + p * (lo::RIP_CARD_W - 18.0),
            0.45 * (p * std::f32::consts::PI).sin(),
        )
    })
}

/// The gold card's halo, breathing.
pub(super) fn halo(t: f64, card: usize) -> f32 {
    0.5 + 0.15 * (t * 2.6 + (card + 1) as f64).sin() as f32
}

/// The picture's stand-in, breathing while the picture is on its way.
pub(super) fn stand_in_pulse(t: f64, card: usize) -> f32 {
    0.55 + 0.45 * (t * 5.0 + (card + 1) as f64).sin() as f32
}

/// The shade over a kept card whose song is not in yet; those on their way
/// breathe.
pub(super) fn veil(t: f64, stage: Stage, song: Option<SongState>) -> f32 {
    if stage != Stage::Filling {
        return 0.0;
    }
    match song {
        Some(SongState::Waiting) => 0.62,
        Some(SongState::Downloading) => 0.42 + 0.14 * (t * 6.0).sin() as f32,
        _ => 0.0,
    }
}

// --- titles -------------------------------------------------------------------

/// How wide a card's title wraps, in the font's own units: 86 pixels at the
/// title's zoom.
pub(super) const TITLE_ZOOM: f32 = 0.42;
pub(super) const TITLE_WRAP: f32 = 204.0;

/// The cards' titles and year lines, fitted for the cards on the table --
/// worked out the first time they are drawn, with the font drawing measures
/// in.
pub(super) fn fitted_titles<'a>(
    rip: &'a Rip,
    measure: &dyn Fn(&str) -> f32,
) -> std::cell::Ref<'a, TitleFit> {
    {
        let mut fit = rip.title_fit.borrow_mut();
        let current = fit
            .songs
            .as_ref()
            .is_some_and(|songs| Arc::ptr_eq(songs, &rip.snapshot.songs));
        // Until the font is there to measure with, nothing is kept: a fit
        // made blind would put every title on one line for good.
        if !current && measure("A") > 0.0 {
            fit.titles = rip
                .snapshot
                .songs
                .iter()
                .map(|song| {
                    let (title, lines) = fit_title(measure, &song.title);
                    (Arc::from(title), lines)
                })
                .collect();
            let width = |text: Option<String>| text.map_or(0.0, |text| measure(&text));
            fit.year_lines = rip
                .snapshot
                .songs
                .iter()
                .map(|song| (width(year_text(song)), width(bpm_text(song))))
                .collect();
            fit.songs = Some(Arc::clone(&rip.snapshot.songs));
        }
    }
    rip.title_fit.borrow()
}

/// A title wrapped at `TITLE_WRAP`, taking whole words off the end and adding
/// "..." until it fits in two lines. Its lines, newline-joined, and how many.
pub(super) fn fit_title(measure: &dyn Fn(&str) -> f32, title: &str) -> (String, usize) {
    let words: Vec<&str> = title.split_whitespace().collect();
    if words.is_empty() {
        return (String::new(), 1);
    }
    let whole = wrap_words(measure, &words, false);
    if whole.1 <= 2 {
        return whole;
    }
    let mut cut = whole;
    for keep in (1..words.len()).rev() {
        cut = wrap_words(measure, &words[..keep], true);
        if cut.1 <= 2 {
            break;
        }
    }
    cut
}

/// Greedy word wrap, as StepMania's text does it: a word goes on the line
/// while the line still fits, and a word too long for any line has one of
/// its own.
fn wrap_words(measure: &dyn Fn(&str) -> f32, words: &[&str], ellipsis: bool) -> (String, usize) {
    let mut out = String::new();
    let mut line = String::new();
    let mut lines = 1;
    for (index, word) in words.iter().enumerate() {
        let tail = if ellipsis && index + 1 == words.len() {
            "..."
        } else {
            ""
        };
        if line.is_empty() {
            line.push_str(word);
            line.push_str(tail);
            continue;
        }
        let trial = format!("{line} {word}{tail}");
        if measure(&trial) > TITLE_WRAP {
            out.push_str(&line);
            out.push('\n');
            lines += 1;
            line.clear();
            line.push_str(word);
            line.push_str(tail);
        } else {
            line = trial;
        }
    }
    out.push_str(&line);
    (out, lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::content_browser::state;
    use deadsync_online::rip_packs::{CardBpm, RipAuthor};

    fn song(index: usize, meter: u32) -> RipSong {
        RipSong {
            key: format!("{}:{}", 100 + index, 7000 + index),
            pack_id: 100 + index as u64,
            pack_name: format!("Pack {index}"),
            song_id: 7000 + index as u64,
            title: format!("Song {index}"),
            artist: "Artist".to_owned(),
            credit: if index == 0 {
                String::new()
            } else {
                "Zaia".to_owned()
            },
            meter,
            year: index.is_multiple_of(2).then_some(2021),
            bpm: match index {
                0 => CardBpm::default(),
                1 => CardBpm {
                    text: "???".to_owned(),
                    tempo: 0,
                },
                _ => CardBpm {
                    text: "150-190".to_owned(),
                    tempo: 175,
                },
            },
            art_url: Some(format!("https://example.test/{index}.png")),
            folder: format!("Song {index}"),
            bytes: 1024 * 1024,
        }
    }

    fn ready_snapshot(set: u64, meters: &[u32], revision: u64) -> Arc<RipSnapshot> {
        Arc::new(RipSnapshot {
            phase: RipPhase::Ready,
            set,
            songs: Arc::from(
                meters
                    .iter()
                    .enumerate()
                    .map(|(index, meter)| song(index, *meter))
                    .collect::<Vec<_>>(),
            ),
            short: meters.len() < BOOSTER_SONGS,
            message: None,
            keep: None,
            kept_count: Some(2),
            revision,
        })
    }

    fn on_rip_tab() -> State {
        let mut state = state::init();
        state.tab_index = state::TABS
            .iter()
            .position(|tab| *tab == state::Tab::Rip)
            .expect("the rip tab");
        state.zone = Zone::Rip;
        state
    }

    fn sync_with(state: &mut State, snapshot: Arc<RipSnapshot>) {
        let authors = Arc::clone(&state.rip.authors);
        sync(&mut state.rip, snapshot, authors);
    }

    fn tick(state: &mut State, seconds: f32) {
        state::update(state, seconds);
    }

    /// UP and DOWN walk the nine rows; DOWN off the last is invalid on a
    /// press and silent held; UP off the first is the tab row.
    #[test]
    fn the_rows_are_walked_and_their_ends_are_solid() {
        let mut state = on_rip_tab();
        assert_eq!(press(&mut state, Key::Down, true), Heard::Change);
        assert_eq!(state.rip.row, 1);
        state.rip.row = Row::ALL.len() - 1;
        assert_eq!(press(&mut state, Key::Down, true), Heard::Invalid);
        assert_eq!(press(&mut state, Key::Down, false), Heard::Nothing);
        assert_eq!(state.rip.row, Row::ALL.len() - 1);

        state.rip.row = 0;
        // held against the top it stays, and still says so
        assert_eq!(press(&mut state, Key::Up, false), Heard::Change);
        assert_eq!(state.zone, Zone::Rip);
        assert_eq!(press(&mut state, Key::Up, true), Heard::Change);
        assert_eq!(state.zone, Zone::Tabs, "a press climbs to the tabs");

        state.zone = Zone::Rip;
        assert_eq!(press(&mut state, Key::Back, true), Heard::Change);
        assert_eq!(state.zone, Zone::Tabs);
    }

    /// The meter's ends hold each other in: FROM never passes TO, and both
    /// stay inside 1..30, saying so with the change even at a limit.
    #[test]
    fn the_meter_ends_clamp_against_each_other() {
        let mut state = on_rip_tab();
        state.rip.row = 4; // EXPERT FROM
        state.rip.lo = 13;
        state.rip.hi = 14;
        assert_eq!(press(&mut state, Key::Right, true), Heard::Change);
        assert_eq!(press(&mut state, Key::Right, false), Heard::Change);
        assert_eq!(state.rip.lo, 14, "not past TO");
        state.rip.lo = 1;
        assert_eq!(press(&mut state, Key::Left, true), Heard::Change);
        assert_eq!(state.rip.lo, 1);

        state.rip.row = 5; // EXPERT TO
        state.rip.hi = 30;
        press(&mut state, Key::Right, true);
        assert_eq!(state.rip.hi, 30);
        state.rip.lo = 9;
        state.rip.hi = 9;
        press(&mut state, Key::Left, true);
        assert_eq!(state.rip.hi, 9, "not under FROM");

        // a row that cannot change says so once, and is silent held
        state.rip.row = 0;
        assert_eq!(press(&mut state, Key::Right, true), Heard::Invalid);
        assert_eq!(press(&mut state, Key::Right, false), Heard::Nothing);
        // a yes-or-no does not repeat
        state.rip.row = 6;
        assert_eq!(press(&mut state, Key::Right, true), Heard::Change);
        assert!(state.rip.full);
        assert_eq!(press(&mut state, Key::Right, false), Heard::Nothing);
        assert!(state.rip.full);
    }

    /// The picker ticks rows in, unticks them, clears them all, and opens on
    /// the first one ticked; eras are shown in the list's order whatever the
    /// order they were ticked.
    #[test]
    fn the_picker_ticks_and_clears() {
        let mut state = on_rip_tab();
        assert_eq!(press(&mut state, Key::Start, true), Heard::Start);
        assert_eq!(state.rip.picking, Some(Pick::Eras));
        assert_eq!(
            press(&mut state, Key::Up, true),
            Heard::Invalid,
            "at the top"
        );
        assert_eq!(press(&mut state, Key::Down, true), Heard::Change);
        assert_eq!(press(&mut state, Key::Down, true), Heard::Change);
        assert_eq!(press(&mut state, Key::Start, true), Heard::Change);
        assert_eq!(press(&mut state, Key::Up, true), Heard::Change);
        assert_eq!(press(&mut state, Key::Start, true), Heard::Change);
        assert_eq!(state.rip.eras, vec![Era::Vintage, Era::Mid], "as ticked");
        assert_eq!(row_value(&state.rip, Row::Era), "Mid-era, Vintage");
        assert_eq!(
            eras_in_order(&state.rip).collect::<Vec<_>>(),
            vec![Era::Mid, Era::Vintage]
        );
        assert_eq!(press(&mut state, Key::Start, true), Heard::Change);
        assert_eq!(state.rip.eras, vec![Era::Vintage], "unticked");

        assert_eq!(press(&mut state, Key::Back, true), Heard::Change);
        assert_eq!(state.rip.picking, None);
        // and opens again on the first ticked
        press(&mut state, Key::Start, true);
        assert_eq!(state.rip.pick_at, 2);
        assert_eq!(press(&mut state, Key::Select, true), Heard::Change);
        assert!(state.rip.eras.is_empty());
        assert_eq!(press(&mut state, Key::Select, true), Heard::Invalid);
    }

    /// The author picker pages a screenful at a time and keeps its window on
    /// the cursor, with the popular count beside a name.
    #[test]
    fn the_author_picker_pages_and_follows_the_cursor() {
        let mut state = on_rip_tab();
        let authors = Arc::new(AuthorsSnapshot {
            phase: AuthorsPhase::Ready,
            authors: Arc::from(
                (0..30)
                    .map(|index| RipAuthor {
                        name: format!("Charter {index}"),
                        popular: u32::from(index < 2),
                    })
                    .collect::<Vec<_>>(),
            ),
            revision: 1,
            message: None,
        });
        let snapshot = Arc::clone(&state.rip.snapshot);
        sync(&mut state.rip, snapshot, authors);
        assert_eq!(state.rip.author_rows[0].note.as_deref(), Some("1 popular"));
        assert!(state.rip.author_rows[2].note.is_none());

        state.rip.row = 1;
        assert_eq!(press(&mut state, Key::Start, true), Heard::Start);
        assert_eq!(state.rip.picking, Some(Pick::Authors));
        assert_eq!(press(&mut state, Key::Right, true), Heard::Change);
        assert_eq!(state.rip.pick_at, 12);
        assert_eq!(state.rip.pick_top, 1, "the window follows");
        press(&mut state, Key::Right, true);
        assert_eq!(state.rip.pick_at, 24);
        assert_eq!(press(&mut state, Key::Right, true), Heard::Change);
        assert_eq!(state.rip.pick_at, 29, "clamped to the last");
        assert_eq!(press(&mut state, Key::Right, true), Heard::Invalid);
        assert_eq!(press(&mut state, Key::Right, false), Heard::Nothing);
        assert_eq!(state.rip.pick_top, 18);
        press(&mut state, Key::Start, true);
        press(&mut state, Key::Back, true);
        assert_eq!(state.rip.chosen, vec!["Charter 29"]);
        assert_eq!(row_value(&state.rip, Row::Author), "Charter 29");
        assert_eq!(press(&mut state, Key::Select, true), Heard::Change);
        assert!(state.rip.chosen.is_empty());
    }

    /// The author row says how its list is doing, and START on a list that
    /// failed asks for it again rather than opening nothing.
    #[test]
    fn the_author_row_says_how_the_list_is_doing() {
        let mut state = on_rip_tab();
        state.rip.row = 1;
        state.rip.authors = Arc::new(AuthorsSnapshot {
            phase: AuthorsPhase::Loading,
            ..AuthorsSnapshot::default()
        });
        assert_eq!(row_value(&state.rip, Row::Author), "loading the list...");
        assert_eq!(press(&mut state, Key::Start, true), Heard::Invalid);

        state.rip.authors = Arc::new(AuthorsSnapshot {
            phase: AuthorsPhase::Error,
            ..AuthorsSnapshot::default()
        });
        assert_eq!(
            row_value(&state.rip, Row::Author),
            "list unavailable  (START to retry)"
        );
        assert_eq!(press(&mut state, Key::Start, true), Heard::Start);
        assert_eq!(
            take_rip_requests(&mut state),
            vec![RipRequest::RetryAuthors]
        );
        assert_eq!(press(&mut state, Key::Select, true), Heard::Start);
        assert_eq!(
            take_rip_requests(&mut state),
            vec![RipRequest::RetryAuthors]
        );

        state.rip.authors = Arc::new(AuthorsSnapshot::default());
        assert_eq!(
            row_value(&state.rip, Row::Author),
            "Anyone  (START to choose)"
        );
        state.rip.chosen = vec!["A".into(), "B".into(), "C".into(), "D".into()];
        assert_eq!(row_value(&state.rip, Row::Author), "A, B  +2 more");
    }

    /// The prompts: typed names are trimmed, a cancel applies nothing, a
    /// meter takes digits only, and an end typed past the other moves it.
    #[test]
    fn the_prompts_accept_and_cancel() {
        let mut state = on_rip_tab();
        state.rip.row = 2;
        assert_eq!(press(&mut state, Key::Start, true), Heard::Start);
        assert_eq!(
            state.rip.prompt.as_ref().map(|p| p.kind.question()),
            Some("Chart authors, separated by commas")
        );
        assert!(prompt_type(&mut state.rip, "  Zaia, dimo "));
        assert_eq!(press_prompt(&mut state.rip, Key::Start), Heard::Start);
        assert_eq!(state.rip.typed, "Zaia, dimo");
        assert!(!row_muted(&state.rip, Row::Typed));

        press(&mut state, Key::Start, true);
        assert_eq!(
            state.rip.prompt.as_ref().map(|p| p.text.as_str()),
            Some("Zaia, dimo")
        );
        prompt_type(&mut state.rip, "x");
        assert_eq!(press_prompt(&mut state.rip, Key::Back), Heard::Change);
        assert_eq!(state.rip.typed, "Zaia, dimo", "a cancel applies nothing");

        // 160 characters at most
        press(&mut state, Key::Start, true);
        prompt_type(&mut state.rip, &"n".repeat(400));
        assert_eq!(
            state.rip.prompt.as_ref().map(|p| p.text.chars().count()),
            Some(NAMES_MAX_CHARS)
        );
        cancel_prompt(&mut state.rip);

        state.rip.row = 4; // EXPERT FROM
        state.rip.hi = 14;
        press(&mut state, Key::Start, true);
        assert_eq!(
            state.rip.prompt.as_ref().map(|p| p.text.as_str()),
            Some("8")
        );
        assert!(prompt_backspace(&mut state.rip));
        assert!(!prompt_backspace(&mut state.rip));
        assert!(!prompt_type(&mut state.rip, "ab"), "digits only");
        prompt_type(&mut state.rip, "2");
        prompt_type(&mut state.rip, "345");
        assert_eq!(
            state.rip.prompt.as_ref().map(|p| p.text.as_str()),
            Some("23")
        );
        accept_prompt(&mut state.rip);
        assert_eq!((state.rip.lo, state.rip.hi), (23, 23), "TO moved to match");

        state.rip.row = 5; // EXPERT TO
        press(&mut state, Key::Start, true);
        state.rip.prompt.as_mut().expect("open").text = "99".to_owned();
        accept_prompt(&mut state.rip);
        assert_eq!(state.rip.hi, 30, "held to 30");
        press(&mut state, Key::Start, true);
        state.rip.prompt.as_mut().expect("open").text = "4".to_owned();
        accept_prompt(&mut state.rip);
        assert_eq!((state.rip.lo, state.rip.hi), (4, 4), "FROM moved to match");
        press(&mut state, Key::Start, true);
        state.rip.prompt.as_mut().expect("open").text = String::new();
        accept_prompt(&mut state.rip);
        assert_eq!(state.rip.hi, 4, "no number, no answer");
    }

    /// A rip sends the options in the list's order with names merged; the
    /// pack's cards arrive, and for a second START is not a choice. A re-rip
    /// leaves out the cards on the table; a keep asks to keep.
    #[test]
    fn the_guard_and_the_re_rip() {
        let mut state = on_rip_tab();
        state.rip.row = 7;
        state.rip.eras = vec![Era::Vintage, Era::Modern];
        state.rip.chosen = vec!["Zaia".into()];
        state.rip.typed = "zaia, dimo".into();
        state.rip.avoid = "Someone, someone".into();
        assert_eq!(press(&mut state, Key::Start, true), Heard::Start);
        let requests = take_rip_requests(&mut state);
        let [RipRequest::Rip { options, exclude }] = requests.as_slice() else {
            panic!("one rip, got {requests:?}");
        };
        assert!(exclude.is_empty(), "a first rip leaves nothing out");
        assert_eq!(options.eras, vec![Era::Modern, Era::Vintage]);
        assert_eq!(options.authors, vec!["Zaia", "dimo"]);
        assert_eq!(options.avoid, vec!["Someone"]);
        assert!(busy(&state.rip), "until the snapshot says so");
        assert_eq!(press(&mut state, Key::Start, true), Heard::Invalid);
        assert_eq!(
            state.rip.toast.as_ref().map(|t| t.text.as_str()),
            Some("a pack is still being ripped")
        );

        sync_with(&mut state, ready_snapshot(1, &[8, 9, 10], 2));
        assert_eq!(state.rip.stage, Stage::Choosing);
        assert_eq!(state.rip.choice, Choice::Keep);
        assert_eq!(
            press(&mut state, Key::Start, true),
            Heard::Nothing,
            "guarded"
        );
        assert!(take_rip_requests(&mut state).is_empty());
        tick(&mut state, 0.5);
        assert_eq!(press(&mut state, Key::Start, true), Heard::Nothing);
        tick(&mut state, 0.6);

        assert_eq!(press(&mut state, Key::Right, true), Heard::Change);
        assert_eq!(state.rip.choice, Choice::ReRip);
        assert_eq!(press(&mut state, Key::Start, true), Heard::Start);
        let requests = take_rip_requests(&mut state);
        let [RipRequest::Rip { exclude, .. }] = requests.as_slice() else {
            panic!("a re-rip, got {requests:?}");
        };
        assert_eq!(exclude, &vec!["100:7000", "101:7001", "102:7002"]);

        sync_with(&mut state, ready_snapshot(2, &[8], 3));
        assert_eq!(
            state.rip.choice,
            Choice::Keep,
            "a new pack offers KEEP first"
        );
        tick(&mut state, 1.1);
        assert_eq!(press(&mut state, Key::Start, true), Heard::Start);
        assert_eq!(take_rip_requests(&mut state), vec![RipRequest::Keep]);
    }

    /// RESET puts every option back and throws the pack away -- but not while
    /// a kept pack is still coming in.
    #[test]
    fn reset_is_refused_while_a_pack_fills() {
        let mut state = on_rip_tab();
        state.rip.row = 8;
        state.rip.typed = "x".into();
        state.rip.lo = 3;
        state.rip.full = true;
        state.rip.eras = vec![Era::Mid];

        let mut filling = (*ready_snapshot(1, &[8, 9], 2)).clone();
        filling.keep = Some(Arc::new(KeepState {
            set: 1,
            folder: "Content Booster Pack 3 - 2026-10-17".into(),
            phase: KeepPhase::Filling,
            songs: Arc::from(vec![SongState::Downloading, SongState::Waiting]),
            done_bytes: 0,
            total_bytes: 2 * 1024 * 1024,
            got: 0,
            failed: 0,
            group_dir: None,
            message: None,
        }));
        sync_with(&mut state, Arc::new(filling));
        assert_eq!(state.rip.stage, Stage::Filling);
        assert_eq!(press(&mut state, Key::Start, true), Heard::Invalid);
        assert_eq!(
            state.rip.toast.as_ref().map(|t| t.text.as_str()),
            Some("a pack is still downloading")
        );
        assert_eq!(state.rip.typed, "x", "nothing reset");
        assert!(take_rip_requests(&mut state).is_empty());

        sync_with(&mut state, ready_snapshot(1, &[8, 9], 3));
        assert_eq!(press(&mut state, Key::Start, true), Heard::Change);
        assert_eq!(take_rip_requests(&mut state), vec![RipRequest::Reset]);
        assert!(state.rip.typed.is_empty() && state.rip.eras.is_empty());
        assert_eq!((state.rip.lo, state.rip.hi, state.rip.full), (8, 14, false));
        assert_eq!(state.rip.row, 8, "the cursor stays");
    }

    /// The hardest card is gold while three or fewer share its meter; a
    /// failed song still counts toward the top but is never gold itself.
    #[test]
    fn the_gold_card_rule() {
        let gold = gold_cards(&[8, 12, 10, 12], &[false; 4]);
        assert_eq!(&gold[..4], &[false, true, false, true]);
        assert!(!gold[4..].iter().any(|g| *g));

        let gold = gold_cards(&[12, 12, 12, 12, 9], &[false; 5]);
        assert!(
            !gold.iter().any(|g| *g),
            "four at the top is the pack's level"
        );

        let gold = gold_cards(&[14, 13, 13], &[true, false, false]);
        assert_eq!(
            &gold[..3],
            &[false, false, false],
            "the failed one held the top"
        );
        assert_eq!(gold_cards(&[], &[]), [false; BOOSTER_SONGS]);
    }

    /// The deal, as the original times it: card one from half a second,
    /// fifteen from 1.48, each turning over 0.43 after its deal starts.
    #[test]
    fn the_deal_and_the_flip_follow_the_clock() {
        let before = card_motion(0, 0.49, 0.0);
        assert!(!before.shown);
        let start = card_motion(0, 0.5, 0.0);
        assert!(start.shown && !start.face_up);
        assert!(
            (start.x - lo::RIP_STAGE_X).abs() < 1e-3 && (start.y - lo::RIP_STAGE_Y).abs() < 1e-3,
            "out of the pack"
        );
        assert!((start.sx - 0.6).abs() < 1e-5 && (start.rot + 14.0).abs() < 1e-5);
        assert!(
            (card_motion(1, 0.57, 0.0).rot - 14.0).abs() < 1e-4,
            "even cards lean right"
        );

        let landed = card_motion(0, 0.9, 0.0);
        assert_eq!((landed.x, landed.y), lo::rip_card_xy(0));
        assert!((landed.sy - 1.0).abs() < 1e-6 && landed.rot.abs() < 1e-6);
        assert!(!card_motion(0, 1.08, 0.0).face_up);
        assert!(
            card_motion(0, 1.10, 0.0).face_up,
            "halfway through the turn"
        );
        let turned = card_motion(0, 1.25, 0.0);
        assert!((turned.sx - 1.0).abs() < 1e-5);

        let last = BOOSTER_SONGS - 1;
        assert!(!card_motion(last, 1.47, 0.0).shown);
        assert!(card_motion(last, 1.481, 0.0).shown);
        assert!(!card_motion(last, 2.06, 0.0).face_up);
        assert!(card_motion(last, 2.24, 0.0).face_up);

        // the overshoot
        assert!((ease_back(0.61) - 1.071).abs() < 0.01);
        assert!((ease(1.0) - 1.0).abs() < 1e-6);
        // and the bump when a song lands
        assert_eq!(bump(-0.1), 0.0);
        assert!((bump(0.15) - 0.07).abs() < 1e-6);
        assert_eq!(bump(0.3), 0.0);
    }

    /// The tear: the strip is gone by 0.45 s, the body by 0.75, and the flash
    /// peaks at a tenth of a second.
    #[test]
    fn the_tear_follows_the_clock() {
        let start = torn_pack(0.0);
        assert!(start.shown && start.flash == 0.0 && start.strip_alpha == 1.0);
        assert!((torn_pack(0.1).flash - 0.7).abs() < 1e-5);
        assert_eq!(torn_pack(0.45).strip_alpha, 0.0);
        assert!((torn_pack(0.45).strip_rot + 28.0).abs() < 1e-5);
        assert_eq!(torn_pack(0.55).flash, 0.0);
        assert!(torn_pack(0.74).shown);
        assert!(!torn_pack(0.75).shown);

        let idle = idle_pack(0.0, false, 0.0);
        assert_eq!(idle.strip_y, -97.0);
        assert_eq!(idle.glow, 0.0);
        let searching = idle_pack(std::f64::consts::FRAC_PI_2 / 7.0, true, 20.0);
        assert!(
            (searching.glow - 0.8).abs() < 1e-4,
            "the halo while it looks"
        );
    }

    /// The pack opens once per rip and is heard tearing only if the stage
    /// sees it within half a second; coming back to it is silent, and is
    /// drawn where it should be.
    #[test]
    fn the_tear_is_heard_once_and_only_fresh() {
        let mut state = on_rip_tab();
        sync_with(&mut state, ready_snapshot(1, &[8, 9], 2));
        tick(&mut state, 0.1);
        let tear = AudioRequest::PlaySfx(crate::resources::CONTENT_BROWSER_TEAR_SFX);
        assert_eq!(state.pending_audio, vec![tear.clone()]);
        tick(&mut state, 0.1);
        assert_eq!(state.pending_audio.len(), 1, "once");

        // a second rip lands while another tab is open
        state.pending_audio.clear();
        state.tab_index = 1;
        sync_with(&mut state, ready_snapshot(2, &[8], 3));
        tick(&mut state, 0.6);
        state.tab_index = state::TABS
            .iter()
            .position(|t| *t == state::Tab::Rip)
            .unwrap();
        tick(&mut state, 0.016);
        assert!(state.pending_audio.is_empty(), "seen too late to be heard");
        assert!(opened_elapsed(&state.rip).is_some_and(|e| e > 0.5));

        // leaving and coming back finds the deal finished
        state::on_enter(&mut state);
        assert!(opened_elapsed(&state.rip).is_some_and(|e| e > 3.0));
        assert!(!guarded(&state.rip));
    }

    /// A kept song landing lights its card with a bump, and a finished
    /// booster is counted by its songs on the way out.
    #[test]
    fn landings_are_timed_and_boosters_remembered() {
        let mut state = on_rip_tab();
        sync_with(&mut state, ready_snapshot(1, &[8, 9], 2));
        tick(&mut state, 3.0);
        let keep = |states: Vec<SongState>, phase, got, revision| {
            let mut snapshot = (*ready_snapshot(1, &[8, 9], revision)).clone();
            snapshot.keep = Some(Arc::new(KeepState {
                set: 1,
                folder: "Content Booster Pack 3 - 2026-10-17".into(),
                phase,
                songs: Arc::from(states),
                done_bytes: 1024 * 1024,
                total_bytes: 2 * 1024 * 1024,
                got,
                failed: 0,
                group_dir: (phase == KeepPhase::Done)
                    .then(|| std::path::PathBuf::from("Songs/Content Booster Pack 3 - 2026-10-17")),
                message: None,
            }));
            Arc::new(snapshot)
        };
        sync_with(
            &mut state,
            keep(
                vec![SongState::Done, SongState::Downloading],
                KeepPhase::Filling,
                1,
                3,
            ),
        );
        assert_eq!(state.rip.stage, Stage::Filling);
        assert_eq!(state.rip.landed[0], state.rip.now);
        assert!(state.rip.landed[1] < 0.0);
        assert_eq!(button_label(&state.rip), "DOWNLOADING  1 / 2");
        assert_eq!(fill_fraction(&state.rip), Some(0.5));
        assert_eq!(heading(&state.rip), "CONTENT BOOSTER PACK 3 - 2026-10-17");
        assert_eq!(
            under_line(&state.rip).0,
            "1 of 2 songs in   -   1.0 MB of 2.0 MB"
        );
        assert!(keeping(&state.rip));

        sync_with(
            &mut state,
            keep(
                vec![SongState::Done, SongState::Done],
                KeepPhase::Done,
                2,
                4,
            ),
        );
        assert_eq!(state.rip.stage, Stage::Done);
        assert_eq!(
            kept_songs(
                &state.rip,
                std::ffi::OsStr::new("Content Booster Pack 3 - 2026-10-17")
            ),
            Some(2)
        );
        assert_eq!(
            under_line(&state.rip).0,
            "2 songs added, synced NULL   -   only 2 matched   -   reload songs when you leave to play them"
        );
        assert_eq!(button_label(&state.rip), "RIP A PACK");
    }

    /// The stage's words in every phase, verbatim from the original.
    #[test]
    fn the_stage_says_what_is_happening() {
        let mut state = on_rip_tab();
        assert_eq!(heading(&state.rip), "RIP A BOOSTER PACK");
        assert_eq!(under_line(&state.rip).0, IDLE_LINE);
        assert_eq!(
            info_text(&state.rip),
            "15 random songs, each from a different pack on stepmaniaonline.net, as a new pack in your library."
        );

        let searching = Arc::new(RipSnapshot {
            phase: RipPhase::Searching,
            set: 1,
            revision: 1,
            ..RipSnapshot::default()
        });
        sync_with(&mut state, searching);
        tick(&mut state, 3.4);
        assert_eq!(heading(&state.rip), "LOOKING FOR SONGS...");
        assert_eq!(button_label(&state.rip), "RIPPING...");
        assert_eq!(
            under_line(&state.rip).0,
            "Searching stepmaniaonline.net for songs that fit...   3s"
        );

        sync_with(&mut state, ready_snapshot(1, &[8; BOOSTER_SONGS], 2));
        assert_eq!(heading(&state.rip), "KEEP THIS PACK?");
        assert_eq!(
            under_line(&state.rip).0,
            "15 songs, 15.0 MB   -   KEEP downloads them, RE-RIP brings a new set"
        );
        assert!(info_text(&state.rip).ends_with("\nPacks ripped so far: 2"));

        let failed = Arc::new(RipSnapshot {
            phase: RipPhase::Failed,
            set: 2,
            message: Some("no songs fit those limits".into()),
            revision: 3,
            ..RipSnapshot::default()
        });
        sync_with(&mut state, failed);
        assert_eq!(heading(&state.rip), "NO PACK THIS TIME");
        assert_eq!(
            under_line(&state.rip),
            (Cow::Borrowed("no songs fit those limits"), true)
        );
        assert!(
            opened_elapsed(&state.rip).is_none(),
            "no cards on a failure"
        );
    }

    /// The pack's foot reads the live options.
    #[test]
    fn the_recipe_reads_the_options() {
        let mut rip = Rip::default();
        assert_eq!(recipe(&rip), "ANY ERA  -  EXPERT 8-14");
        rip.eras = vec![Era::Vintage, Era::Modern];
        rip.lo = 12;
        rip.hi = 12;
        rip.typed = "zaia".into();
        rip.full = true;
        assert_eq!(
            recipe(&rip),
            "MODERN + VINTAGE  -  EXPERT 12  -  ZAIA  -  FULL"
        );
        rip.eras = Era::ALL.to_vec();
        rip.chosen = vec!["Dimo".into()];
        assert_eq!(recipe(&rip), "ANY ERA  -  EXPERT 12  -  2 AUTHORS  -  FULL");
    }

    /// A card's words: the credit only when there is one, the year only when
    /// the pack's dates are its own.
    #[test]
    fn a_cards_words_are_made_once_per_rip() {
        let mut state = on_rip_tab();
        sync_with(&mut state, ready_snapshot(1, &[8, 9], 2));
        assert_eq!(state.rip.cards.len(), 2);
        assert!(state.rip.cards[0].credit.is_none(), "no credit, no line");
        assert_eq!(state.rip.cards[1].credit.as_deref(), Some("by Zaia"));
        assert_eq!(state.rip.cards[0].year.as_deref(), Some("2021"));
        assert!(state.rip.cards[1].year.is_none());
        let bpm = |card: &CardText| {
            card.bpm
                .as_ref()
                .map(|(text, pace)| (text.to_string(), *pace))
        };
        assert_eq!(bpm(&state.rip.cards[0]), None, "no tempo, no BPM");
        assert_eq!(
            bpm(&state.rip.cards[1]),
            Some(("??? BPM".to_owned(), Pace::Unknown))
        );
        assert_eq!(
            bpm(&card_texts(&[song(2, 10)])[0]),
            Some(("150-190 BPM".to_owned(), Pace::Medium)),
            "a range, coloured by the tempo it plays at"
        );
        let before = Arc::as_ptr(&state.rip.cards[1].title);
        sync_with(&mut state, ready_snapshot(1, &[8, 9], 2));
        assert_eq!(
            Arc::as_ptr(&state.rip.cards[1].title),
            before,
            "same revision, same words"
        );

        let mut out = Vec::new();
        wanted_art(&state.rip, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, song_art_key("https://example.test/0.png"));
    }

    /// Two lines at most; past that whole words go and "..." ends it.
    #[test]
    fn titles_fit_two_lines() {
        // ten units a character, wrapping at 204: twenty characters a line
        let measure = |text: &str| text.chars().count() as f32 * 10.0;
        assert_eq!(fit_title(&measure, "Short"), ("Short".to_owned(), 1));
        assert_eq!(
            fit_title(&measure, "A title that wraps once"),
            ("A title that wraps\nonce".to_owned(), 2)
        );
        let (cut, lines) = fit_title(
            &measure,
            "Words words words words words words words words words",
        );
        assert_eq!(lines, 2);
        assert!(cut.ends_with("..."), "{cut}");
        assert_eq!(cut, "Words words words\nwords words words...");
    }

    /// Held directions repeat on the rows the original repeats, with their
    /// sound, and stop with the key.
    #[test]
    fn held_keys_repeat() {
        let mut state = on_rip_tab();
        state.rip.row = 4;
        state.rip.lo = 3;
        hold(&mut state, Key::Right);
        tick(&mut state, 0.3);
        assert_eq!(state.rip.lo, 3, "not yet");
        tick(&mut state, 0.13);
        assert_eq!(state.rip.lo, 4);
        assert_eq!(state.pending_audio, vec![AudioRequest::PlaySfx(SFX_CHANGE)]);
        tick(&mut state, 0.13);
        assert_eq!(state.rip.lo, 5);
        release(&mut state, Key::Right);
        tick(&mut state, 1.0);
        assert_eq!(state.rip.lo, 5, "let go");

        // not while a prompt is up, and not off the tab
        hold(&mut state, Key::Right);
        state.zone = Zone::Tabs;
        tick(&mut state, 1.0);
        assert_eq!(state.rip.lo, 5);
    }

    /// The toast holds two seconds and fades over four tenths.
    #[test]
    fn the_toast_holds_then_fades() {
        assert_eq!(toast_alpha(0.0), Some(1.0));
        assert_eq!(toast_alpha(1.99), Some(1.0));
        assert!((toast_alpha(2.2).unwrap() - 0.5).abs() < 1e-4);
        assert_eq!(toast_alpha(2.4), None);
        let mut state = on_rip_tab();
        rip_refused(&mut state, "this pack was already kept".into());
        tick(&mut state, 2.5);
        assert!(state.rip.toast.is_none());
    }

    /// The tear's sound is a file the theme ships and prepares.
    #[test]
    fn the_tear_sound_is_shipped_and_prepared() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert!(
            root.join(crate::resources::CONTENT_BROWSER_TEAR_SFX)
                .is_file()
        );
        assert!(crate::resources::SFX_PATHS.contains(&crate::resources::CONTENT_BROWSER_TEAR_SFX));
    }
}
