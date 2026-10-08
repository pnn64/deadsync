//! What the browser knows, and how the shell keeps it current.
//!
//! Nothing here owns catalog data. Four services publish immutable snapshots on
//! their own threads and the shell hands us the latest of each every frame:
//!
//! * `stepmaniaonline` -- the CSV catalog: id, name, song count, size, sync,
//!   pack type, substyle. Complete, and with no dates or artwork in it.
//! * `smo_details` -- the rendered table: banner, date added, chart types.
//!   Additive; a pack missing from it still lists and still downloads.
//! * `itgdb` -- the curated dedicated-doubles category, which is the left half
//!   of the doubles view.
//! * `pack_page` -- one pack's song list, read only while its detail page is
//!   open.
//!
//! The visible list is re-derived only when one of those revisions moves, not
//! every frame.

use crate::color;
use deadsync_chart::song::SyncPref;
use deadsync_notefield::ModelMeshCache;
use deadsync_online::beginner::{BeginnerPhase, BeginnerSnapshot};
use deadsync_online::itgdb::{ItgdbSnapshot, normalize_name};
use deadsync_online::pack_page::PageSnapshot;
use deadsync_online::popular_packs::{PopularPhase, PopularSnapshot};
use deadsync_online::smo_describe::{DescribeSnapshot, View, ViewPhase};
use deadsync_online::smo_details::{DetailsPhase, DetailsSnapshot, PackDetails};
use deadsync_online::smo_search::{SearchPhase, SearchSnapshot};
use deadsync_online::smo_songs::{PreviewSnapshot, SongInstallsSnapshot};
use deadsync_online::stepmaniaonline::{InstallPhase, PackInfo, Snapshot, search_catalog};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::layout as lo;
use crate::screens::components::shared::visual_style_bg;

/// The featured grid: two rows of six on screen, two pages of them held.
pub(super) const FEATURED_COLUMNS: usize = lo::FEAT_COLS;
pub(super) const FEATURED_VISIBLE: usize = lo::FEAT_COLS * lo::FEAT_ROWS;
pub(super) const FEATURED_MAX: usize = FEATURED_VISIBLE * 2;

/// Held-direction repeat, matching the Options overlay exactly -- a player who
/// learns one list should not find the other feels different.
pub(super) const NAV_INITIAL_HOLD_DELAY: Duration = Duration::from_millis(375);
pub(super) const NAV_REPEAT_INTERVAL: Duration = Duration::from_millis(125);

/// How long the cursor has to rest on a pack before its page is read.
///
/// The pane wants the pack page -- the chart count, the difficulty span, the
/// authors, the histogram -- but asking for it the instant the cursor lands
/// would read a page for every row somebody scrolls past. Waiting a beat means
/// only packs a reader actually stopped on are fetched.
pub(super) const SELECTION_DWELL: f32 = 0.4;

/// How still the query has to be before the network passes run.
///
/// Long enough that typing a word is one search rather than eight, short
/// enough that it feels like it answered while you were still looking at it.
pub(super) const SEARCH_DEBOUNCE: f32 = 0.35;

/// The original's `MaxInputLength`.
pub(super) const QUERY_MAX_CHARS: usize = 64;

/// Where the reader's attention is.
///
/// The browser is one screen with several depths rather than several screens,
/// so backing out of a pack does not lose the list, and backing out of a list
/// does not lose the tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Zone {
    Tabs,
    /// The banner grid above the list.
    Featured,
    /// The year strip, which only the Years tab has.
    Years,
    /// Choosing which of the two doubles columns to read.
    DoublesPick,
    /// Inside one of them.
    DoublesRows,
    List,
    /// The library grid, which is two columns rather than a list.
    Installed,
    Detail,
}

/// One pack already in the library.
///
/// The browser cannot ask the catalogue about these: a pack can be in the
/// library without ever having come from the site. So the name and the song
/// count come from the game's own song cache, and anything richer -- artwork,
/// a date, a size -- only if the catalogue happens to know the name too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledPack {
    pub name: String,
    /// The name lowercased, which is what matching is done on.
    pub lower: String,
    pub songs: usize,
    /// What this pack's own `Pack.ini` declares, as the engine read it.
    /// `Default` means it declares nothing and the machine setting decides.
    pub sync: SyncPref,
    /// The pack's banner on this machine, as a texture key.
    ///
    /// The library grid draws this rather than the catalogue's copy. The pack
    /// is on disk and so is its artwork; fetching a picture of a pack the
    /// player already has, over the network, from a site that may not even
    /// list it, is work with nothing to show for it.
    pub banner: Option<String>,
}

/// What is known about how a pack was synced, and what can be done about it.
///
/// Worth stating plainly because the obvious reading of the catalogue is
/// wrong: **stepmaniaonline.net never says a pack is ITG-synced.** Its sync
/// column only ever holds `null`, `0`, `n/a`, `mixed` or `other` -- so the
/// site can tell you a pack is null-synced, and otherwise tells you nothing.
/// A pack an old ITG-era author synced the ITG way sits in `n/a` alongside
/// thousands of modern ones.
///
/// That still leaves one thing worth acting on, and it is the useful half:
/// a pack the site *does* call null needs an explicit `SyncOffset=NULL` the
/// moment the machine default is set to ITG, or the engine will delay it by
/// nine milliseconds it does not want.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SmoSync {
    /// The site says this pack is null-synced. The only positive answer it
    /// ever gives.
    Null,
    /// The site says the pack is internally inconsistent, so no one pack-wide
    /// value is right for it.
    Mixed,
    /// The site does not know. Most of the catalogue, and where every genuinely
    /// old pack lives.
    Unknown,
}

impl SmoSync {
    /// Read the catalogue's sync column.
    ///
    /// This column has its own vocabulary and must not go through the generic
    /// "is this field empty" filter the other columns use. There, the literal
    /// string `null` means the field was not set. **Here it means the pack is
    /// null-synced**, which is the site's only positive answer -- so treating
    /// it as absent reported 1,248 packs as having no sync listed when the
    /// site had said exactly what their sync was.
    pub(super) fn read(value: Option<&str>) -> Self {
        match value
            .map(str::trim)
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "null" | "0" => Self::Null,
            "mixed" | "other" => Self::Mixed,
            _ => Self::Unknown,
        }
    }

    /// A phrase for a facts table, where the space is one line.
    pub(super) const fn short(self) -> &'static str {
        match self {
            Self::Null => "NULL (0 ms), per SMO",
            Self::Mixed => "mixed, per SMO",
            Self::Unknown => "not listed by SMO",
        }
    }
}

/// What the catalogue says about a pack's sync, if it says anything.
pub(super) fn smo_sync_of(pack: &PackInfo) -> SmoSync {
    SmoSync::read(pack.sync.as_deref())
}

/// What the catalogue says about a library pack's sync, if it knows it at all.
pub(super) fn installed_smo_sync(state: &State, entry: &InstalledPack) -> SmoSync {
    installed_catalog_entry(state, entry).map_or(SmoSync::Unknown, smo_sync_of)
}

/// Which value the sync dialog should open on.
///
/// The pack's own declaration first -- it is a decision somebody already made.
/// Otherwise the catalogue's, when it has one. Otherwise NULL, which is what
/// everything modern is and what the engine assumes anyway.
pub(super) fn suggested_sync(state: &State, entry: &InstalledPack) -> SyncPref {
    match entry.sync {
        SyncPref::Null | SyncPref::Itg => entry.sync,
        SyncPref::Default => match installed_smo_sync(state, entry) {
            SmoSync::Null => SyncPref::Null,
            SmoSync::Mixed | SmoSync::Unknown => SyncPref::Null,
        },
    }
}

/// The views, in the order the ITGmania browser presents them.
///
/// Every one of these is answerable from data stepmaniaonline.net publishes.
/// One of that browser's tabs is not, and is deliberately absent rather than
/// shipped empty -- see the note in `docs/content-browser-parity.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tab {
    /// Free text over the whole catalogue. First, as in the original.
    Search,
    /// Packs with dance charts -- what a dance pad plays. The landing tab,
    /// because this browser is for dance packs.
    Pad,
    Keyboard,
    /// Packs a beginner can work through, decided by reading each candidate's
    /// charts. The one view the catalogue cannot answer on its own.
    Beginner,
    AllAround,
    Stamina,
    /// Packs built for doubles, then packs that merely carry a doubles chart.
    Doubles,
    /// Grouped by the year they were added to the site.
    Years,
    Installed,
}

/// The tab strip. Fixed rather than derived: these are the questions the
/// browser answers, and a tab vanishing because the catalog shifted would be
/// worse than one that is occasionally thin.
/// The strip, in the original's own order.
pub(super) const TABS: [Tab; 9] = [
    Tab::Search,
    Tab::Pad,
    Tab::Keyboard,
    Tab::Beginner,
    Tab::AllAround,
    Tab::Stamina,
    Tab::Doubles,
    Tab::Years,
    Tab::Installed,
];

impl Tab {
    /// Whether this tab's list is the site's newest-first page, or a slice of
    /// the catalogue the CSV can answer on its own.
    ///
    /// The distinction is what keeps this screen quick. PAD and YEARS are the
    /// whole catalogue in date order, so they read the page the site hands
    /// over and stop there. KEYBOARD, STAMINA and ALL AROUND are small
    /// closed sets the CSV already knows in full -- about 199, 454 and 752
    /// packs -- built once per catalogue, so paging them would only hide rows
    /// the browser already has.
    const fn is_paged(self) -> bool {
        matches!(self, Self::Pad | Self::Years)
    }

    /// The tab strip's icon, from the original's own `ContentBrowserIcons`.
    ///
    /// The artwork is 96px square and drawn at twelve, which is why the
    /// strip's labels start where they do rather than at the pill's edge.
    pub(super) const fn icon(self) -> &'static str {
        match self {
            Self::Search => "content_browser/search.png",
            Self::Pad => "content_browser/pad.png",
            Self::Keyboard => "content_browser/keyboard.png",
            Self::Beginner => "content_browser/beginner.png",
            Self::Doubles => "content_browser/doubles.png",
            Self::Stamina => "content_browser/stamina.png",
            Self::AllAround => "content_browser/tech.png",
            Self::Years => "content_browser/year.png",
            Self::Installed => "content_browser/installed.png",
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Search => "SEARCH",
            Self::Pad => "PAD",
            Self::Keyboard => "KEYBOARD",
            Self::Beginner => "BEGINNER",
            Self::Doubles => "DOUBLES",
            Self::Stamina => "STAMINA",
            Self::AllAround => "ALL AROUND",
            Self::Years => "YEARS",
            Self::Installed => "INSTALLED",
        }
    }

    /// The substyle set the describe service fetches whole for this tab.
    ///
    /// These tabs leave out packs with no banner, so they have to know about
    /// every pack in the set before they can be drawn -- not just the ones
    /// on screen, which is all a per-row lookup would cover.
    pub(super) const fn describe_view(self) -> Option<View> {
        match self {
            Self::Stamina => Some(View::Stamina),
            Self::AllAround => Some(View::AllAround),
            _ => None,
        }
    }

    /// The CSV `Substyle` values this tab selects on.
    ///
    /// ALL AROUND is the original's "tech" bucket: the site's `technical` and
    /// `all around` together. `all around` alone is fifty-odd packs, most of
    /// them a decade old, which is not what the tab is for.
    const fn substyles(self) -> &'static [&'static str] {
        match self {
            Self::Stamina => &["stamina"],
            Self::AllAround => &["technical", "all around"],
            _ => &[],
        }
    }

    /// What an empty list means here, which is never the same thing twice.
    pub(super) const fn empty_message(self) -> &'static str {
        match self {
            Self::Installed => "nothing from this catalogue is in your library yet",
            Self::Search => "nothing matches that search",
            Self::Years => "no packs were added in that year",
            Self::Doubles => "no doubles packs found",
            Self::Beginner => "no beginner-friendly packs found",
            _ => "no packs in this view",
        }
    }
}

/// The question asked on the way out when this session installed packs: the
/// original's reload dialog.
///
/// New packs are not in the song wheel until the library is rescanned, and a
/// reader who downloads a pack and goes straight to play it finds it missing.
/// Asking on the way out, rather than rescanning behind every install, keeps
/// a rescan from racing the next download's unzip -- and says plainly what is
/// about to happen.
#[derive(Clone, Debug, Default)]
pub(super) struct ReloadPrompt {
    /// 0 is "Reload songs", 1 is "Not yet".
    pub(super) choice: usize,
    /// The rescan has been asked for; until it lands the keys answer nothing.
    pub(super) reloading: bool,
    /// The shell has taken the folders and started the rescan, so the next
    /// finish it reports is this one's and not some earlier job's.
    pub(super) started: bool,
    /// What the rescan last said it was on: songs done, songs in all, pack.
    pub(super) progress: Option<(usize, usize, String)>,
}

/// A page turn waiting on the helping it asked for.
///
/// Right on the last page of a list with more to find fetches the next
/// helping. The reader asked to turn the page, not just to fetch, so the turn
/// happens as soon as the new page has a pack on it -- rather than leaving
/// them on the old page wondering whether anything happened.
#[derive(Clone, Copy, Debug)]
pub(super) struct PendingTurn {
    /// The tab it was asked on. A turn for another view is no turn at all.
    pub(super) tab_index: usize,
    /// The page to land on.
    pub(super) page: usize,
    /// Whether the fetch has been seen running, so a fetch that has not
    /// started yet is not mistaken for one that finished short.
    pub(super) saw_loading: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct NavHold {
    pub(super) delta: isize,
    pub(super) held_for: Duration,
    pub(super) since_scroll: Duration,
}

/// Ways of finding a pack in the catalogue, built once per catalogue rather
/// than per lookup.
///
/// The catalogue is nine and a half thousand packs. Every one of these lookups
/// was a linear scan with a `to_lowercase()` allocation per entry, and the
/// library grid did twenty-two of them per frame -- a couple of hundred
/// thousand allocations a frame, which is what a stall is made of.
#[derive(Default)]
pub(super) struct CatalogIndex {
    /// The revision this was built from, so it is rebuilt when the catalogue
    /// changes and not when one of the five other services moves.
    revision: u64,
    by_id: HashMap<u64, usize>,
    /// Exact name, lowercased -- how a library folder is matched to a pack.
    by_lower_name: HashMap<String, usize>,
    /// Alphanumerics only -- how a name from another service is matched, where
    /// punctuation and spacing cannot be relied on to agree.
    by_loose_name: HashMap<String, usize>,
    /// The three views the CSV answers on its own, newest pack first.
    ///
    /// Pack ids are handed out as packs are added, so a higher id is a newer
    /// pack -- the original's own ordering for these tabs, and one that is
    /// known for every pack the moment the catalogue lands. The details
    /// service only ever knows the newest couple of hundred, so ordering by
    /// its dates sorted everything else here by name.
    keyboard: Vec<usize>,
    stamina: Vec<usize>,
    all_around: Vec<usize>,
}

impl CatalogIndex {
    fn rebuild(&mut self, catalog: &[PackInfo], revision: u64) {
        self.revision = revision;
        self.by_id.clear();
        self.by_lower_name.clear();
        self.by_loose_name.clear();
        self.keyboard.clear();
        self.stamina.clear();
        self.all_around.clear();
        self.by_id.reserve(catalog.len());
        self.by_lower_name.reserve(catalog.len());
        self.by_loose_name.reserve(catalog.len());
        for (index, pack) in catalog.iter().enumerate() {
            // First wins: the catalogue has a handful of duplicate names, and
            // the earlier id is the one every other lookup here agrees on.
            self.by_id.entry(pack.id).or_insert(index);
            self.by_lower_name
                .entry(pack.name.to_lowercase())
                .or_insert(index);
            self.by_loose_name
                .entry(normalize_name(pack.name.as_str()))
                .or_insert(index);

            // Stamina and all-around describe pad difficulty, so a keyboard
            // pack has no business in either, whatever its substyle says.
            if matches_type(pack, "keyboard") {
                self.keyboard.push(index);
            } else if Tab::Stamina
                .substyles()
                .iter()
                .any(|wanted| matches_substyle(pack, wanted))
            {
                self.stamina.push(index);
            } else if Tab::AllAround
                .substyles()
                .iter()
                .any(|wanted| matches_substyle(pack, wanted))
            {
                self.all_around.push(index);
            }
        }
        for view in [&mut self.keyboard, &mut self.stamina, &mut self.all_around] {
            view.sort_unstable_by(|a, b| catalog[*b].id.cmp(&catalog[*a].id));
        }
    }

    /// The CSV view for this tab, when it is one.
    fn csv_view(&self, tab: Tab) -> Option<&[usize]> {
        match tab {
            Tab::Keyboard => Some(&self.keyboard),
            Tab::Stamina => Some(&self.stamina),
            Tab::AllAround => Some(&self.all_around),
            _ => None,
        }
    }
}

pub struct State {
    pub active_color_index: i32,
    pub(super) bg: visual_style_bg::State,

    pub(super) snapshot: Arc<Snapshot>,
    pub(super) details: Arc<DetailsSnapshot>,
    pub(super) itgdb: Arc<ItgdbSnapshot>,
    pub(super) page: Arc<PageSnapshot>,
    pub(super) search: Arc<SearchSnapshot>,
    pub(super) beginner: Arc<BeginnerSnapshot>,
    pub(super) built_beginner_revision: u64,
    /// Banners, dates and types for packs no list page described.
    pub(super) describe: Arc<DescribeSnapshot>,
    pub(super) built_describe_revision: u64,
    pub(super) built_describe_views_revision: u64,
    pub(super) popular: Arc<PopularSnapshot>,
    /// Packs whose artwork is not coming. Without this a row cannot tell
    /// "still arriving" from "never arriving", and spins on both.
    pub(super) banner_failed: Arc<HashSet<u64>>,
    /// Artwork the ranking serves in a small size, by pack id. Preferred over
    /// the catalogue's own: a 256px JPEG against most of a megabyte of PNG,
    /// for a card 96px wide.
    pub(super) popular_banner: HashMap<u64, String>,
    /// The ranking's artwork for every pack it ranks, not just the featured
    /// ones. The last resort for a row: the catalogue's own art is preferred
    /// wherever it is known, as the original's rows use it, but most of the
    /// packs the beginner walk turns up are nowhere in the details pages.
    pub(super) popular_art: HashMap<u64, String>,
    /// The (ranking, catalogue) revisions `popular_art` was joined from. The
    /// join normalises two hundred and fifty names, so it is redone when
    /// either side moves -- not on every beginner verdict.
    popular_art_built: (u64, u64),
    pub(super) index: CatalogIndex,
    /// Why each hit matched, for the row that shows it. A result whose reason
    /// is invisible reads as a wrong answer.
    pub(super) search_why: HashMap<u64, String>,
    /// Revisions the visible list was built from, so it is rebuilt when a
    /// service moves and not otherwise.
    pub(super) built_catalog_revision: u64,
    pub(super) built_details_revision: u64,
    pub(super) built_itgdb_revision: u64,
    pub(super) built_search_revision: u64,
    pub(super) built_popular_revision: u64,

    pub(super) tab_index: usize,
    pub(super) zone: Zone,

    /// The featured grid's packs: catalog indices, newest first, limited to
    /// packs that actually have artwork -- a grid of blank cards is worse than
    /// a shorter grid.
    pub(super) featured: Vec<usize>,
    pub(super) featured_index: usize,
    /// The page the grid is showing, as a card offset. Only ever a multiple of
    /// `FEATURED_VISIBLE`: the grid pages, it does not scroll.
    pub(super) featured_window: usize,
    /// Whether the detail page was opened from the grid, so BACK returns there
    /// rather than dumping the reader into the list.
    pub(super) featured_from: bool,

    /// Which chip of the year strip is picked. Slot 0 is the current year and
    /// the last slot is OLDER, exactly as the original's `YearList()`.
    pub(super) year_slot: usize,

    /// Indices into `snapshot.catalog`, in display order.
    pub(super) results: Vec<usize>,
    /// Whether the site has rows past the ones on screen, and this view is one
    /// that reads them in date order.
    pub(super) can_load_more: bool,
    /// Absolute index into `results`. The page is `cursor / ROWS`, so paging
    /// and stepping are the same number and cannot disagree.
    pub(super) cursor: usize,

    /// The doubles view's two columns: itgdb's curated list, then the packs
    /// the site says merely carry a doubles chart.
    pub(super) doubles_left: Vec<usize>,
    pub(super) doubles_right: Vec<usize>,
    /// Whether packs for the curated column are still being looked up. Its
    /// packs join it as their banners are confirmed, so until then it is a
    /// column that is filling in, not one that is short.
    pub(super) doubles_left_waiting: bool,
    pub(super) doubles_column: usize,
    pub(super) doubles_window: [usize; 2],
    pub(super) doubles_row: usize,

    /// The committed query, which is what the list is built from.
    pub(super) query: String,
    /// How long the query has been unchanged, which is what debounces the two
    /// network passes.
    pub(super) query_idle: f32,
    pub(super) caret_elapsed: f32,

    /// The pack the cursor is resting on, and for how long -- see
    /// `SELECTION_DWELL`.
    pub(super) dwell_pack: Option<u64>,
    pub(super) dwell: f32,

    /// The detail page's song list: the window offset and the picked row.
    pub(super) song_window: usize,
    pub(super) song_pick: usize,

    /// What is already in the library. Rebuilt by the shell only when the song
    /// cache changes.
    pub(super) installed: Vec<InstalledPack>,
    /// Absolute index into `installed`. The page is `cursor / INST_ROWS`.
    pub(super) installed_cursor: usize,
    /// The pack a delete has been asked about but not yet confirmed.
    ///
    /// Deleting is the one thing here that cannot be undone, so it is the one
    /// thing that is asked about twice.
    pub(super) removing: Option<String>,
    /// What the last delete did, shown until the reader moves on.
    pub(super) remove_result: Option<String>,
    /// The dialog for changing a library pack's sync, while it is up.
    pub(super) sync_dialog: Option<super::sync_dialog::SyncDialog>,
    /// DeadSync's pack sync review, when the dialog has started a measure.
    pub(super) pack_sync_overlay: crate::screens::pack_sync::OverlayState,
    /// The pack that review is about.
    pub(super) pack_sync_group: Option<String>,
    /// The review's settings, written by the shell: the confidence a measured
    /// offset needs to be offered, and the menu buttons it answers to.
    pub(super) pack_sync_confidence: u8,
    pub(super) pack_sync_menu_only: bool,
    pub(super) pack_sync_three_key: bool,
    /// Whether the engine will actually act on a `Pack.ini` right now. Written
    /// by the shell, because it is a machine setting rather than a fact about
    /// any pack -- and a dialog that does not say this is a dialog that looks
    /// like it did nothing.
    pub(super) pack_ini_offsets_on: bool,

    pub(super) nav_hold: Option<NavHold>,
    pub(super) pending_turn: Option<PendingTurn>,
    pub(super) reload_prompt: Option<ReloadPrompt>,
    /// Pack folders installed since the library was last rescanned. Kept
    /// across visits: "Not yet" leaves them for the next way out.
    pub(super) installed_dirs: Vec<PathBuf>,
    /// The chart preview up, or fading out.
    pub(super) preview: Option<super::preview::Preview>,
    /// "Listen or Download?", while it is asked.
    pub(super) song_menu: Option<super::preview::SongMenu>,
    /// The chart preview's and the single songs' latest answers.
    pub(super) song_preview: Arc<PreviewSnapshot>,
    pub(super) song_installs: Arc<SongInstallsSnapshot>,
    /// Seconds into the playing audio file, from the music clock. Only fed
    /// while a preview is playing.
    pub(super) music_time: Option<f32>,
    /// The reader's own noteskin, once it is loaded and its textures are up.
    pub(super) preview_skin: Option<Arc<deadsync_assets::noteskin::Noteskin>>,
    /// That skin's model geometry, built once on the loader's thread and
    /// shared by every note the window draws. Borrowed mutably while drawing,
    /// which only has the state to read.
    pub(super) preview_model_cache: RefCell<ModelMeshCache>,
    /// Why the last preview or song request did not happen, for the song
    /// list's header, as the original shows it.
    pub(super) preview_message: Option<String>,
    /// The single song last asked for, by pack and title: its progress and
    /// how it ended stay in its pack's header whatever row the cursor is on,
    /// where the original holds a window up for it. Replaced by the next
    /// request, and dropped by the next preview.
    pub(super) watched_song: Option<(u64, String, String)>,
    /// Single songs a finished rescan has loaded, by pack and title, so the
    /// reload question counts only the ones still owed. Their install records
    /// outlive the rescan; at most `smo_songs`'s 64 of them.
    pub(super) reloaded_songs: Vec<(u64, String, String)>,
    /// The detail page's cursor is on the pack's download button rather than
    /// on the song list: the original's `detailZone == "download"`, reached by
    /// UP from the first song.
    pub(super) detail_on_button: bool,
    /// Work for the shell: song requests and the music they start and stop.
    pub(super) pending_songs: Vec<super::preview::SongRequest>,
    pub(super) pending_audio: Vec<deadsync_theme::AudioRequest>,
    pub(super) pending_reload_dirs: Vec<PathBuf>,
}

pub fn init() -> State {
    State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        bg: visual_style_bg::State::new(),
        snapshot: Arc::new(Snapshot::default()),
        details: Arc::new(DetailsSnapshot::default()),
        itgdb: Arc::new(ItgdbSnapshot::default()),
        page: Arc::new(PageSnapshot::default()),
        search: Arc::new(SearchSnapshot::default()),
        beginner: Arc::new(BeginnerSnapshot::default()),
        built_beginner_revision: u64::MAX,
        describe: Arc::new(DescribeSnapshot::default()),
        built_describe_revision: u64::MAX,
        built_describe_views_revision: u64::MAX,
        search_why: HashMap::new(),
        popular: Arc::new(PopularSnapshot::default()),
        banner_failed: Arc::new(HashSet::new()),
        popular_banner: HashMap::new(),
        popular_art: HashMap::new(),
        popular_art_built: (u64::MAX, u64::MAX),
        index: CatalogIndex::default(),
        built_catalog_revision: u64::MAX,
        built_details_revision: u64::MAX,
        built_itgdb_revision: u64::MAX,
        built_search_revision: u64::MAX,
        built_popular_revision: u64::MAX,
        // PAD, not SEARCH: opening the browser should show packs rather than
        // a tab whose whole content is a prompt.
        tab_index: 1,
        zone: Zone::List,
        featured: Vec::new(),
        featured_index: 0,
        featured_window: 0,
        featured_from: false,
        year_slot: 0,
        results: Vec::new(),
        can_load_more: false,
        cursor: 0,
        doubles_left: Vec::new(),
        doubles_right: Vec::new(),
        doubles_left_waiting: false,
        doubles_column: 0,
        doubles_window: [0, 0],
        doubles_row: 0,
        query: String::new(),
        query_idle: f32::MAX,
        caret_elapsed: 0.0,
        dwell_pack: None,
        dwell: 0.0,
        song_window: 0,
        song_pick: 0,
        installed: Vec::new(),
        installed_cursor: 0,
        removing: None,
        remove_result: None,
        sync_dialog: None,
        pack_sync_overlay: crate::screens::pack_sync::OverlayState::Hidden,
        pack_sync_group: None,
        pack_sync_confidence: DEFAULT_PACK_SYNC_CONFIDENCE,
        pack_sync_menu_only: false,
        pack_sync_three_key: false,
        pack_ini_offsets_on: false,
        nav_hold: None,
        pending_turn: None,
        reload_prompt: None,
        installed_dirs: Vec::new(),
        preview: None,
        song_menu: None,
        song_preview: Arc::new(PreviewSnapshot::default()),
        song_installs: Arc::new(SongInstallsSnapshot::default()),
        music_time: None,
        preview_skin: None,
        preview_model_cache: RefCell::new(ModelMeshCache::default()),
        preview_message: None,
        watched_song: None,
        reloaded_songs: Vec::new(),
        detail_on_button: false,
        pending_songs: Vec::new(),
        pending_audio: Vec::new(),
        pending_reload_dirs: Vec::new(),
    }
}

/// Entering fresh: keep the catalog, put the reader back at the top of the
/// list rather than inside whichever pack they were last reading.
pub fn on_enter(state: &mut State) {
    // Always the landing tab, and always the top of it.
    //
    // The tab used to survive leaving, which meant coming back from a session
    // that ended on INSTALLED or a half-typed search put the reader somewhere
    // they had not asked to be. Opening the browser is a fresh question.
    state.tab_index = TABS
        .iter()
        .position(|tab| *tab == Tab::Pad)
        .unwrap_or_default();
    state.query.clear();
    state.query_idle = f32::MAX;
    state.search_why.clear();
    state.year_slot = 0;
    state.cursor = 0;
    state.nav_hold = None;
    state.caret_elapsed = 0.0;
    state.reload_prompt = None;
    state.pending_turn = None;
    state.preview = None;
    state.song_menu = None;
    state.preview_message = None;
    state.watched_song = None;
    state.music_time = None;
    state.song_window = 0;
    state.song_pick = 0;
    state.detail_on_button = false;
    // A confirm that was left open is not still being asked.
    state.removing = None;
    state.remove_result = None;
    state.sync_dialog = None;
    // A review left running was cancelled by the shell on the way out.
    state.pack_sync_overlay = crate::screens::pack_sync::OverlayState::Hidden;
    state.pack_sync_group = None;
    // The tab survives leaving and coming back, so the landing zone has to
    // follow it: two of the tabs do not have a single list to land in, and
    // dropping the cursor into one that is not drawn strands it.
    // The cursor starts on the tab strip, not in the list.
    //
    // The first question this screen asks is which of eight views you want,
    // and the strip is where that is answered. Landing in the list instead
    // put the cursor on a pack before the reader had said they wanted packs,
    // and made the tabs something you had to climb back up to find.
    state.zone = Zone::Tabs;
    rebuild_results(state, None);
}

pub fn update(state: &mut State, delta_time: f32) {
    if delta_time <= 0.0 || !delta_time.is_finite() {
        return;
    }
    super::sync_dialog::tick(state);
    // While the pack sync review is up it is the screen: nothing behind it
    // moves.
    if super::sync_dialog::overlay_visible(state) {
        return;
    }
    state.caret_elapsed += delta_time;
    state.query_idle += delta_time;
    super::input::repeat_nav_hold(state, delta_time);
    super::preview::update(state, delta_time);

    // Reset the dwell whenever the cursor lands on a different pack, so the
    // timer measures rest rather than elapsed time on the screen.
    let here = focused_pack(state).map(|pack| pack.id);
    if here == state.dwell_pack {
        state.dwell += delta_time;
    } else {
        state.dwell_pack = here;
        state.dwell = 0.0;
    }
}

/// Everything the browser reads, handed over together.
///
/// Six independent services publish on their own threads and the browser is a
/// pure reader of all of them. Passing them as one value rather than as six
/// arguments keeps the handoff honest: they are one frame's worth of the
/// world, and taking them apart at the call site is how they drift.
pub struct Services {
    pub catalog: Arc<Snapshot>,
    pub details: Arc<DetailsSnapshot>,
    pub itgdb: Arc<ItgdbSnapshot>,
    pub page: Arc<PageSnapshot>,
    pub search: Arc<SearchSnapshot>,
    pub popular: Arc<PopularSnapshot>,
    pub beginner: Arc<BeginnerSnapshot>,
    pub describe: Arc<DescribeSnapshot>,
    /// The chart preview's answer and the single songs on their way in.
    pub song_preview: Arc<PreviewSnapshot>,
    pub song_installs: Arc<SongInstallsSnapshot>,
    /// Artwork that is not coming, so a row can stop waiting for it.
    pub banner_failed: Arc<HashSet<u64>>,
    /// Whether `MachinePackIniOffsets` is on. Off by default, and while it is
    /// off a pack's `Pack.ini` sync value changes nothing at all.
    pub pack_ini_offsets_on: bool,
    /// The pack sync review's settings: the confidence a measured offset
    /// needs, and whether only the dedicated menu buttons -- and only three of
    /// them -- navigate it.
    pub pack_sync_confidence: u8,
    pub pack_sync_menu_only: bool,
    pub pack_sync_three_key: bool,
}

impl Default for Services {
    fn default() -> Self {
        Self {
            catalog: Arc::new(Snapshot::default()),
            details: Arc::new(DetailsSnapshot::default()),
            itgdb: Arc::new(ItgdbSnapshot::default()),
            page: Arc::new(PageSnapshot::default()),
            search: Arc::new(SearchSnapshot::default()),
            popular: Arc::new(PopularSnapshot::default()),
            beginner: Arc::new(BeginnerSnapshot::default()),
            describe: Arc::new(DescribeSnapshot::default()),
            song_preview: Arc::new(PreviewSnapshot::default()),
            song_installs: Arc::new(SongInstallsSnapshot::default()),
            banner_failed: Arc::new(HashSet::new()),
            pack_ini_offsets_on: false,
            pack_sync_confidence: DEFAULT_PACK_SYNC_CONFIDENCE,
            pack_sync_menu_only: false,
            pack_sync_three_key: false,
        }
    }
}

/// The review's confidence until the shell says otherwise: the machine
/// default for Null-or-Die.
const DEFAULT_PACK_SYNC_CONFIDENCE: u8 = 80;

/// The shell's per-frame handoff.
pub fn sync_stepmaniaonline(
    state: &mut State,
    services: Services,
    ready_song_dirs: Vec<PathBuf>,
    installed_groups: Option<Vec<InstalledPack>>,
) {
    let Services {
        catalog,
        details,
        itgdb,
        page,
        search,
        popular,
        beginner,
        describe,
        song_preview,
        song_installs,
        banner_failed,
        pack_ini_offsets_on,
        pack_sync_confidence,
        pack_sync_menu_only,
        pack_sync_three_key,
    } = services;
    state.pack_sync_confidence = pack_sync_confidence;
    state.pack_sync_menu_only = pack_sync_menu_only;
    state.pack_sync_three_key = pack_sync_three_key;
    state.song_preview = song_preview;
    state.song_installs = song_installs;
    super::preview::sync(state);
    state.banner_failed = banner_failed;
    state.pack_ini_offsets_on = pack_ini_offsets_on;
    // A view landing changes which packs a list holds. A lookup landing only
    // changes how a row is drawn -- except in the curated doubles column,
    // whose packs join it as their banners are confirmed.
    let views_moved = describe.views_revision != state.built_describe_views_revision;
    let described_moved = describe.revision != state.built_describe_revision;
    state.describe = describe;

    let selected_id = selected_pack(state).map(|pack| pack.id);
    let moved = catalog.revision != state.built_catalog_revision
        || details.revision != state.built_details_revision
        || itgdb.revision != state.built_itgdb_revision
        || search.revision != state.built_search_revision
        || views_moved
        || popular.revision != state.built_popular_revision
        || beginner.revision != state.built_beginner_revision;
    state.snapshot = catalog;
    state.details = details;
    state.itgdb = itgdb;
    state.search = search;
    state.popular = popular;
    state.beginner = beginner;

    // A new pack page means a new song list, so the reader starts at its top
    // rather than at a row number left over from the pack before.
    if page.pack_id != state.page.pack_id {
        state.song_window = 0;
        state.song_pick = 0;
        state.detail_on_button = false;
    }
    state.page = page;

    // The lookups everything else here is built on. Rebuilt only when the
    // catalogue itself moves -- not when one of the other five services does,
    // and never per row.
    if state.index.revision != state.snapshot.revision || state.index.by_id.is_empty() {
        let catalog = Arc::clone(&state.snapshot.catalog);
        state
            .index
            .rebuild(catalog.as_ref(), state.snapshot.revision);
    }

    // None means the library has not changed, so the names we have still hold.
    let installed_changed = installed_groups.is_some();
    if let Some(groups) = installed_groups {
        state.installed = groups;
    }

    if moved || installed_changed {
        rebuild_featured(state);
        rebuild_results(state, selected_id);
        rebuild_doubles(state);
        state.built_catalog_revision = state.snapshot.revision;
        state.built_details_revision = state.details.revision;
        state.built_itgdb_revision = state.itgdb.revision;
        state.built_search_revision = state.search.revision;
        state.built_popular_revision = state.popular.revision;
        state.built_beginner_revision = state.beginner.revision;
        state.built_describe_revision = state.describe.revision;
    } else if described_moved && doubles_showing(state) {
        rebuild_doubles(state);
        state.built_describe_revision = state.describe.revision;
    }
    // Only consumed where the doubles columns were rebuilt from it, so
    // lookups that land while another tab is open are still news when the
    // doubles tab shows again.
    state.built_describe_views_revision = state.describe.views_revision;

    settle_pending_turn(state);

    for dir in ready_song_dirs {
        if !state.installed_dirs.contains(&dir) {
            state.installed_dirs.push(dir);
        }
    }
}

/// Whether a chart preview is up. The shell feeds the music clock and loads
/// the reader's noteskin only while it is.
pub fn preview_active(state: &State) -> bool {
    state.preview.is_some()
}

/// Seconds into the playing audio file, from the music clock.
pub fn set_music_time(state: &mut State, seconds: Option<f32>) {
    state.music_time = seconds;
}

/// The reader's noteskin, once loaded with its textures resident, and the
/// model geometry built for it off the game thread -- handed over once, with
/// the first frame the skin is. A skin that arrives without one gets an empty
/// cache, which builds each model the first time it is drawn and keeps it.
pub fn set_preview_skin(
    state: &mut State,
    skin: Option<Arc<deadsync_assets::noteskin::Noteskin>>,
    models: Option<super::chart_window::PreviewSkinModels>,
) {
    let changed = match (&state.preview_skin, &skin) {
        (Some(old), Some(new)) => !Arc::ptr_eq(old, new),
        (None, None) => false,
        _ => true,
    };
    if let Some(models) = models {
        *state.preview_model_cache.get_mut() = models.into_cache();
    } else if changed {
        *state.preview_model_cache.get_mut() = ModelMeshCache::default();
    }
    state.preview_skin = skin;
}

/// Previews and single songs the page asked for since the last frame.
pub fn take_song_requests(state: &mut State) -> Vec<super::preview::SongRequest> {
    std::mem::take(&mut state.pending_songs)
}

/// Music the preview started or stopped since the last frame.
pub fn take_audio_requests(state: &mut State) -> Vec<deadsync_theme::AudioRequest> {
    std::mem::take(&mut state.pending_audio)
}

/// A song request `smo_songs` refused, in its own words.
pub fn song_request_refused(state: &mut State, reason: String) {
    state.preview_message = Some(reason);
}

/// Song directories the shell should rescan, taken rather than borrowed so the
/// caller cannot process the same install twice. Only ever filled when the
/// reader says "Reload songs" on the way out.
pub fn take_pending_reload_dirs(state: &mut State) -> Vec<PathBuf> {
    let dirs = std::mem::take(&mut state.pending_reload_dirs);
    if !dirs.is_empty()
        && let Some(prompt) = state.reload_prompt.as_mut()
    {
        prompt.started = true;
    }
    dirs
}

/// The rescan's progress, for the dialog that asked for it.
///
/// Returns whether the rescan this screen started has finished -- the shell
/// then refreshes the song wheel and takes the reader out, as the original
/// does. A finish from some earlier job, before this one was handed over, is
/// not this one's.
pub fn sync_reload_events(
    state: &mut State,
    events: impl IntoIterator<Item = crate::views::SimplyLoveContentReloadEvent>,
) -> bool {
    let mut finished = false;
    for event in events {
        let Some(prompt) = state.reload_prompt.as_mut().filter(|prompt| prompt.started) else {
            continue;
        };
        match event {
            crate::views::SimplyLoveContentReloadEvent::Song {
                done, total, pack, ..
            } => prompt.progress = Some((done, total, pack)),
            crate::views::SimplyLoveContentReloadEvent::Finished { .. } => finished = true,
            _ => {}
        }
    }
    if finished {
        state.reload_prompt = None;
        state.installed_dirs.clear();
        // Every single song in so far is loaded now. Remembered by name, not
        // counted: `smo_songs` drops old records past 64, and a count
        // would then hide a song that is new. Rebuilt rather than added to,
        // so a song whose record has gone is forgotten here too.
        let installs = Arc::clone(&state.song_installs.installs);
        state.reloaded_songs.clear();
        state.reloaded_songs.extend(
            installs
                .iter()
                .filter(|install| {
                    install.phase == deadsync_online::smo_songs::SongInstallPhase::Installed
                })
                .map(|install| {
                    (
                        install.pack_id,
                        install.title.clone(),
                        install.artist.clone(),
                    )
                }),
        );
    }
    finished
}

/// Whether a pack is still on its way in. The reload is not offered then: it
/// would race the unzip and miss the pack.
pub(super) fn downloads_active(state: &State) -> bool {
    state.snapshot.installs.iter().any(|install| {
        matches!(
            install.phase,
            InstallPhase::Queued | InstallPhase::Downloading | InstallPhase::Extracting
        )
    })
}

/// The shell's answer to a delete request.
///
/// Reported rather than assumed: a pack can be partly deleted -- a song whose
/// audio is still open will not go -- and the grid must say so instead of
/// quietly showing one fewer row.
pub fn finish_pack_deletion(state: &mut State, result: Result<usize, String>) {
    state.removing = None;
    state.remove_result = Some(match result {
        Ok(0) => "nothing was deleted".to_owned(),
        Ok(1) => "deleted 1 song folder".to_owned(),
        Ok(count) => format!("deleted {count} song folders"),
        Err(error) => format!("could not delete: {error}"),
    });
    state.installed_cursor = state
        .installed_cursor
        .min(state.installed.len().saturating_sub(1));
}

/// The query the shell should be running.
///
/// `None` while the reader is still typing. Every keystroke is a different
/// query, and running the two network passes on each of them would be eight
/// searches for an eight-letter charter's name. The catalogue's own names are
/// matched locally in the meantime, so the list still answers instantly.
pub fn wanted_search(state: &State) -> Option<&str> {
    (state.query_idle >= SEARCH_DEBOUNCE).then_some(state.query.as_str())
}

/// The packs the beginner walk should consider, in the order it should
/// consider them.
///
/// Popularity order, because a beginner is better served by a pack people
/// actually play. Two kinds are dropped before a request is spent on them:
/// one the site calls stamina cannot be mostly easy charts, and one it calls
/// mods is not a difficulty at all.
pub fn beginner_candidates(state: &State) -> Vec<u64> {
    let mut out = Vec::new();
    for entry in state.popular.packs.iter() {
        let Some(index) = state
            .index
            .by_loose_name
            .get(&normalize_name(entry.name.as_str()))
            .copied()
        else {
            continue;
        };
        let pack = &state.snapshot.catalog[index];
        let substyle = pack
            .substyle
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .to_lowercase();
        if substyle == "stamina" || substyle == "mods" {
            continue;
        }
        let details = state.details.by_id.get(&pack.id);
        if !details.is_none_or(PackDetails::is_dance_only) || matches_type(pack, "keyboard") {
            continue;
        }
        // A beginner list is a main list, and those only hold packs with a
        // banner -- so no page read is spent deciding one that could not be
        // shown anyway.
        if !has_banner(state, pack.id) {
            continue;
        }
        out.push(pack.id);
    }
    out
}

/// Whether the beginner view is the one showing.
pub fn beginner_showing(state: &State) -> bool {
    tab(state) == Tab::Beginner && state.query.is_empty()
}

/// Whether the first beginner walk should be started.
///
/// Only while its own tab is open. Reading a pack page per candidate is the
/// most expensive thing this screen does, and doing it for a view nobody is
/// looking at would spend thirty requests on nothing.
///
/// Only the first walk starts here; every later helping is asked for. So this
/// goes false the moment the walk leaves Idle, which also keeps the shell from
/// rebuilding the candidate list on every frame the tab is open. And it waits
/// for the catalogue: a walk over no candidates would finish at once as
/// "exhausted" and never be tried again.
pub fn wants_beginner_walk(state: &State) -> bool {
    beginner_showing(state)
        && state.beginner.phase == BeginnerPhase::Idle
        && !state.popular.packs.is_empty()
        && !state.index.by_id.is_empty()
}

/// Whether the beginner list is still being assembled, with nothing to say
/// about it yet other than that it is coming.
///
/// Idle counts only while the popularity list it walks is still on its way.
/// If that failed, nothing is coming, and the view has to say so rather than
/// spin until the screen closes.
pub(super) fn beginner_building(state: &State) -> bool {
    beginner_showing(state)
        && match state.beginner.phase {
            BeginnerPhase::Loading => true,
            BeginnerPhase::Idle => matches!(
                state.popular.phase,
                PopularPhase::Idle | PopularPhase::Loading
            ),
            BeginnerPhase::Ready | BeginnerPhase::Error => false,
        }
}

/// Whether the year view is still filling itself in.
///
/// Keyed on there being more to fetch, not on a request being in flight. The
/// view pulls one page per round trip, so a "loading" indicator that follows
/// the request blinks on and off several times a second -- with the header
/// text swapping under it each time. This stays lit for the whole walk and
/// goes out once, at the end.
pub(super) fn year_indexing(state: &State) -> bool {
    years_showing(state) && (state.details.has_more || state.details.phase == DetailsPhase::Loading)
}

/// Whether this view wants the next page fetched without being asked.
///
/// The year strip is the case: its buckets are spread across the whole
/// catalogue, so a single page answers only the newest weeks. It fills itself
/// in the background -- one page at a time, with the progress bar beside the
/// heading saying so -- rather than making the reader press for more.
pub fn wants_more_pages(state: &State) -> bool {
    years_showing(state) && state.details.has_more
}

/// Whether the search is still bringing passes in. The name pass answers
/// immediately; the two that need the network land after it.
pub(super) fn search_running(state: &State) -> bool {
    !state.query.is_empty()
        && state.search.query == state.query
        && state.search.phase == SearchPhase::Loading
}

/// Whether more matched than the search will show.
pub(super) fn search_capped(state: &State) -> bool {
    state.search.query == state.query && state.search.capped
}

/// The pack whose page the shell should be reading, if any.
///
/// The detail page always wants one. A list only wants one once the cursor has
/// come to rest, which is what keeps scrolling free.
pub fn wanted_pack_page(state: &State) -> Option<u64> {
    if state.zone != Zone::Detail && state.dwell < SELECTION_DWELL {
        return None;
    }
    focused_pack(state).map(|pack| pack.id)
}

pub(super) fn tab(state: &State) -> Tab {
    TABS[state.tab_index.min(TABS.len() - 1)]
}

/// Whether the featured grid is on screen.
///
/// The original's `GridShowing()`: not the keyboard tab, no search running,
/// and the plain list rather than one of the views that replaces it.
pub(super) fn grid_showing(state: &State) -> bool {
    tab(state) == Tab::Pad && state.query.is_empty() && !state.featured.is_empty()
}

/// Whether the grid can take the cursor. A grid mid-load with no drawn cards
/// deliberately is not landable.
pub(super) fn grid_landable(state: &State) -> bool {
    grid_showing(state)
}

/// Whether typing goes into a search box the reader can see.
///
/// True whenever the SEARCH tab is the one open. There is no prompt to open
/// and nothing to press first: the box is the tab, so a reader who arrives
/// there and types is already searching.
pub(super) fn search_field_showing(state: &State) -> bool {
    tab(state) == Tab::Search
}

/// The heading band's title and blurb, verbatim from the original.
///
/// Every view that does not have the featured grid above it gets one -- the
/// band is that view's header, and without it a filtered list is just a list
/// of packs with nothing saying what filtered it.
///
/// The tabs missing from this table are the ones that head themselves: PAD has
/// the grid, YEARS has the year strip, INSTALLED has its own count line.
pub(super) fn band(state: &State) -> Option<(&'static str, &'static str)> {
    if !state.query.is_empty() {
        return Some((
            "SEARCH RESULTS",
            "in pack names, chart authors and song titles.",
        ));
    }
    match tab(state) {
        Tab::Keyboard => Some((
            "KEYBOARD PACKS",
            "Packs stepmaniaonline.net lists as keyboard.",
        )),
        Tab::AllAround => Some((
            "ALL AROUND",
            "Packs stepmaniaonline.net lists as technical or all-around.",
        )),
        Tab::Stamina => Some((
            "HARD CONTENT / STAMINA",
            "Packs stepmaniaonline.net lists as stamina.",
        )),
        Tab::Doubles => Some((
            "DOUBLES",
            "Packs made for doubles, then packs with doubles charts in them.",
        )),
        Tab::Beginner => Some((
            "BEGINNER FRIENDLY",
            "Popular packs whose beginner charts are mostly 1s to 4s.",
        )),
        Tab::Search | Tab::Pad | Tab::Years | Tab::Installed => None,
    }
}

/// Whether the next helping of this list is being gathered right now.
///
/// Per view: the beginner list is the walk's, the pad list is the details
/// service's. Asking either service regardless of tab made one list claim to
/// be loading because of work the other had in flight.
pub(super) fn more_loading(state: &State) -> bool {
    if beginner_showing(state) {
        state.beginner.phase == BeginnerPhase::Loading
    } else {
        state.details.phase == DetailsPhase::Loading
    }
}

/// Whether the cursor is on the last pack of a list that has more to find.
///
/// Down from here asks for the next helping instead of wrapping to the top:
/// the same key cannot mean both "fetch more" and "throw me back to row one".
///
/// An empty list that can still find more counts: a helping that turned up
/// nothing -- every page it read failed, say -- must still be askable, or the
/// view is a dead end for the rest of the session.
pub(super) fn at_list_end(state: &State) -> bool {
    state.can_load_more && (state.results.is_empty() || state.cursor + 1 == state.results.len())
}

/// Carry out a waiting page turn once its page has a pack on it, or drop it
/// when the reader has gone elsewhere or the fetch finished without one.
fn settle_pending_turn(state: &mut State) {
    let loading = more_loading(state);
    let Some(turn) = state.pending_turn.as_mut() else {
        return;
    };
    if turn.tab_index != state.tab_index || state.zone != Zone::List {
        state.pending_turn = None;
        return;
    }
    let first = turn.page * lo::ROWS;
    if state.results.len() > first {
        state.cursor = first;
        state.pending_turn = None;
        return;
    }
    if loading {
        turn.saw_loading = true;
    } else if turn.saw_loading {
        state.pending_turn = None;
    }
}

/// The mark under the last row saying there is more, as the original draws
/// it: `Some(false)` for "more", `Some(true)` for "finding more", `None` when
/// there is no mark to draw.
///
/// A mark rather than a row. A row takes a slot, so a helping that exactly
/// fills a page pushed it onto a page of its own, where nobody reading the
/// first screen would find it. The mark hangs under the last row instead, so
/// it never makes a page.
pub(super) fn more_mark(state: &State) -> Option<bool> {
    let at_end = state.can_load_more && list_page(state) + 1 >= total_pages(state);
    at_end.then(|| more_loading(state))
}

/// Whether the list cannot yet be put in the right order.
///
/// Every view but Installed is ordered newest-first, and that order comes from
/// the details service rather than from the catalogue -- the CSV has no dates
/// in it at all. Drawing the catalogue before the order is known shows it in
/// the CSV's own order, which is alphabetical, and then reshuffles it under
/// the reader a second later. Skeleton rows say "coming" instead.
pub(super) fn awaiting_order(state: &State) -> bool {
    // Stamina and all-around wait for their whole set to be described:
    // without it they cannot tell which packs have banners.
    if state.query.is_empty()
        && let Some(view) = tab(state).describe_view()
    {
        return matches!(
            state.describe.view_phase(view),
            ViewPhase::Idle | ViewPhase::Loading
        );
    }
    // The other CSV views are ordered by pack id, which the catalogue has from
    // the start, so they wait for nothing.
    if !state.query.is_empty()
        || tab(state) == Tab::Installed
        || state.index.csv_view(tab(state)).is_some()
    {
        return false;
    }
    state.details.newest_first.is_empty()
        && matches!(
            state.details.phase,
            DetailsPhase::Idle | DetailsPhase::Loading
        )
}

/// Whether the heading band is up, which is what pushes the list down to its
/// tight top.
pub(super) fn band_showing(state: &State) -> bool {
    band(state).is_some()
}

/// How many packs the band should say it found.
pub(super) fn band_count(state: &State) -> usize {
    if doubles_showing(state) {
        state.doubles_left.len() + state.doubles_right.len()
    } else {
        state.results.len()
    }
}

/// The doubles view replaces the single list entirely.
pub(super) fn doubles_showing(state: &State) -> bool {
    tab(state) == Tab::Doubles && state.query.is_empty()
}

pub(super) fn years_showing(state: &State) -> bool {
    tab(state) == Tab::Years && state.query.is_empty()
}

/// The library grid replaces the single list entirely, the way doubles does.
pub(super) fn installed_showing(state: &State) -> bool {
    tab(state) == Tab::Installed && state.query.is_empty()
}

pub(super) const fn installed_page(state: &State) -> usize {
    state.installed_cursor / lo::INST_ROWS
}

pub(super) fn installed_pages(state: &State) -> usize {
    state.installed.len().div_ceil(lo::INST_ROWS).max(1)
}

/// The cell the library cursor is on, as the (page, column, row) it really is.
pub(super) const fn installed_cell(state: &State) -> (usize, usize) {
    let slot = state.installed_cursor % lo::INST_ROWS;
    (slot / lo::INST_PER_COL, slot % lo::INST_PER_COL)
}

/// Move to an absolute (page, column, row). False when nothing is there,
/// which is what makes the edges of the grid feel solid.
pub(super) fn installed_goto(state: &mut State, page: isize, column: isize, row: isize) -> bool {
    if page < 0 || column < 0 || column >= lo::INST_COLS as isize {
        return false;
    }
    if row < 0 || row >= lo::INST_PER_COL as isize {
        return false;
    }
    let index = page * lo::INST_ROWS as isize + column * lo::INST_PER_COL as isize + row;
    if index < 0 || index >= state.installed.len() as isize {
        return false;
    }
    state.installed_cursor = index as usize;
    true
}

/// The library pack the cursor is on.
pub(super) fn installed_at(state: &State) -> Option<&InstalledPack> {
    state.installed.get(state.installed_cursor)
}

/// The catalogue's record for a library pack, when it has one -- which is
/// where the artwork, the size and the date come from.
pub(super) fn installed_catalog_entry<'a>(
    state: &'a State,
    entry: &InstalledPack,
) -> Option<&'a PackInfo> {
    let index = *state.index.by_lower_name.get(entry.lower.as_str())?;
    state.snapshot.catalog.get(index)
}

/// Rows the list has room for. Fixed at seven, as the original: `ROW_H = 35`
/// exists precisely so that seven of them clear `CONTENT_BOT` in both the
/// tall and the tight layout.
pub(super) const fn visible_rows() -> usize {
    lo::ROWS
}

/// Which page of the list the cursor is on.
pub(super) const fn list_page(state: &State) -> usize {
    state.cursor / lo::ROWS
}

pub(super) fn total_pages(state: &State) -> usize {
    row_count(state).div_ceil(lo::ROWS).max(1)
}

/// The first result on the current page.
pub(super) const fn window_start(state: &State) -> usize {
    list_page(state) * lo::ROWS
}

/// The grid cursor as the (page, row, column) it really is.
///
/// The original's comment is the whole design: "A page is the whole grid, so
/// Left/Right walk a row and carry to the next page rather than wrapping into
/// the row below; Up/Down are the only way between the two rows."
pub(super) const fn featured_row_col(state: &State) -> (usize, usize) {
    let slot = state.featured_index.saturating_sub(state.featured_window);
    (slot / FEATURED_COLUMNS, slot % FEATURED_COLUMNS)
}

pub(super) const fn featured_page(state: &State) -> usize {
    state.featured_window / FEATURED_VISIBLE
}

/// Move to an absolute (page, row, column). False when nothing is there,
/// which is what makes the edges of the grid feel solid.
pub(super) fn featured_goto(state: &mut State, page: isize, row: isize, column: isize) -> bool {
    if page < 0 || row < 0 || row >= lo::FEAT_ROWS as isize {
        return false;
    }
    if column < 0 || column >= FEATURED_COLUMNS as isize {
        return false;
    }
    let index = page * FEATURED_VISIBLE as isize + row * FEATURED_COLUMNS as isize + column;
    if index < 0 || index >= state.featured.len() as isize {
        return false;
    }
    state.featured_window = page as usize * FEATURED_VISIBLE;
    state.featured_index = index as usize;
    true
}

/// The pack the grid cursor is on.
pub(super) fn featured_pack(state: &State) -> Option<&PackInfo> {
    let index = *state.featured.get(state.featured_index)?;
    state.snapshot.catalog.get(index)
}

/// The pack the reader is acting on, whichever zone they are in.
pub(super) fn focused_pack(state: &State) -> Option<&PackInfo> {
    match state.zone {
        Zone::Featured => featured_pack(state),
        Zone::DoublesPick | Zone::DoublesRows => doubles_pack(state),
        Zone::Detail if state.featured_from => featured_pack(state),
        _ if doubles_showing(state) => doubles_pack(state),
        _ if installed_showing(state) => {
            installed_at(state).and_then(|entry| installed_catalog_entry(state, entry))
        }
        _ => selected_pack(state),
    }
}

pub(super) fn selected_pack(state: &State) -> Option<&PackInfo> {
    let index = *state.results.get(state.cursor)?;
    state.snapshot.catalog.get(index)
}

/// One doubles column's list.
pub(super) fn doubles_column(state: &State, column: usize) -> &[usize] {
    if column == 1 {
        state.doubles_right.as_slice()
    } else {
        state.doubles_left.as_slice()
    }
}

/// The absolute index the doubles cursor sits at within its column.
pub(super) const fn doubles_cursor(state: &State) -> usize {
    state.doubles_window[if state.doubles_column == 1 { 1 } else { 0 }] + state.doubles_row
}

pub(super) fn doubles_pack(state: &State) -> Option<&PackInfo> {
    let column = doubles_column(state, state.doubles_column);
    let index = *column.get(doubles_cursor(state))?;
    state.snapshot.catalog.get(index)
}

pub(super) fn details_for<'a>(state: &'a State, pack: &PackInfo) -> Option<&'a PackDetails> {
    state
        .details
        .by_id
        .get(&pack.id)
        .or_else(|| state.describe.get(pack.id))
}

/// The year a strip slot stands for. `None` is the OLDER bucket, which is the
/// last slot and holds everything before the floor.
pub(super) fn year_at(slot: usize) -> Option<u16> {
    let slots = lo::year_slots();
    if slot + 1 >= slots {
        return None;
    }
    Some(lo::current_year() - slot as u16)
}

pub(super) fn year_label(slot: usize) -> String {
    match year_at(slot) {
        Some(year) => year.to_string(),
        None => "OLDER".to_owned(),
    }
}

/// The grid's packs: the most played, per arrowcloud's ranking.
///
/// The catalogue can say what is new. It has no play data at all, so it cannot
/// say what is worth playing -- which is why the strip is sourced from
/// somewhere else entirely and joined back by name. When that service is
/// unreachable the strip falls back to the newest packs with artwork, because
/// a strip of something beats a hole in the screen.
fn rebuild_featured(state: &mut State) {
    let catalog = Arc::clone(&state.snapshot.catalog);
    let details = Arc::clone(&state.details);
    let popular = Arc::clone(&state.popular);
    state.popular_banner.clear();

    let art_key = (popular.revision, state.snapshot.revision);
    if state.popular_art_built != art_key {
        state.popular_art_built = art_key;
        state.popular_art.clear();
        for entry in popular.packs.iter() {
            let Some(url) = entry.banner_url.as_ref() else {
                continue;
            };
            if let Some(index) = state
                .index
                .by_loose_name
                .get(&normalize_name(entry.name.as_str()))
                .copied()
            {
                state
                    .popular_art
                    .entry(catalog[index].id)
                    .or_insert_with(|| url.clone());
            }
        }
    }

    let mut featured: Vec<usize> = Vec::with_capacity(FEATURED_MAX);
    let mut seen: Vec<usize> = Vec::with_capacity(FEATURED_MAX);
    for entry in popular.packs.iter() {
        if featured.len() >= FEATURED_MAX {
            break;
        }
        // The ranking is its own catalogue; a pack it knows that the site does
        // not is not downloadable and so is not offered.
        let Some(index) = state
            .index
            .by_loose_name
            .get(&normalize_name(entry.name.as_str()))
            .copied()
        else {
            continue;
        };
        if seen.contains(&index) {
            continue;
        }
        let pack_id = catalog[index].id;
        let art = entry.banner_url.clone().or_else(|| {
            details
                .by_id
                .get(&pack_id)
                .and_then(|known| known.banner_url.clone())
        });
        // A card is artwork with a name under it. Without any, it is a plate.
        let Some(art) = art else {
            continue;
        };
        if entry.banner_url.is_some() {
            state.popular_banner.insert(pack_id, art);
        }
        seen.push(index);
        featured.push(index);
    }

    // Nothing from the ranking yet, or it could not be reached: the newest
    // packs that have artwork, which is at least an honest answer.
    if featured.is_empty() {
        featured = details
            .newest_first
            .iter()
            .filter(|id| {
                details
                    .by_id
                    .get(id)
                    .is_some_and(|entry| entry.banner_url.is_some())
            })
            .filter_map(|id| state.index.by_id.get(id).copied())
            .take(FEATURED_MAX)
            .collect();
    }

    state.featured = featured;
    state.featured_index = state
        .featured_index
        .min(state.featured.len().saturating_sub(1));
    state.featured_window = state.featured_index - state.featured_index % FEATURED_VISIBLE;
}

/// What the featured strip is currently able to say it is showing.
///
/// The heading is not decoration: a strip labelled POPULAR PACKS that is
/// really the newest ones is a lie the reader has no way to catch.
pub(super) fn featured_heading(state: &State) -> &'static str {
    if state.popular.phase == PopularPhase::Ready && !state.popular.packs.is_empty() {
        "POPULAR PACKS"
    } else {
        "JUST ADDED"
    }
}

/// Split the doubles candidates into the two columns the view shows.
///
/// A pack built for doubles and a pack with four doubles charts buried in it
/// are two different things to go looking for, so each gets a column rather
/// than one list with a seam in it the reader has to find.
///
/// Both are main lists, so both hold only packs with a banner. The candidates
/// were described by the doubles pass and are decided at once; the curated
/// packs mostly were not, so each joins its column once a lookup confirms it
/// has art.
///
/// Joined from the two short lists -- itgdb's few dozen names, the doubles
/// pass's couple of hundred ids -- rather than by walking the catalogue and
/// normalising all nine and a half thousand names on every rebuild.
fn rebuild_doubles(state: &mut State) {
    let catalog = Arc::clone(&state.snapshot.catalog);
    let details = Arc::clone(&state.details);
    let itgdb = Arc::clone(&state.itgdb);

    let mut left: Vec<usize> = Vec::with_capacity(itgdb.dedicated.len());
    let mut curated: HashSet<usize> = HashSet::with_capacity(itgdb.dedicated.len());
    let mut waiting = false;
    for name in itgdb.dedicated.iter() {
        let Some(index) = state.index.by_loose_name.get(name).copied() else {
            continue;
        };
        if !curated.insert(index) {
            continue;
        }
        let pack_id = catalog[index].id;
        if has_banner(state, pack_id) {
            left.push(index);
        } else if state.describe.pending.contains(&pack_id) {
            waiting = true;
        }
    }
    let mut right: Vec<usize> = details
        .doubles
        .iter()
        .filter_map(|id| state.index.by_id.get(id).copied())
        .filter(|index| !curated.contains(index) && has_banner(state, catalog[*index].id))
        .collect();

    // Newest pack first, as every other list here.
    for column in [&mut left, &mut right] {
        column.sort_unstable_by(|a, b| catalog[*b].id.cmp(&catalog[*a].id));
    }
    state.doubles_left = left;
    state.doubles_right = right;
    state.doubles_left_waiting = waiting;
    clamp_doubles(state);
}

/// The curated doubles packs nothing has described yet, so the shell can look
/// them all up while the tab is open. A few dozen at most.
fn doubles_unknown(state: &State) -> impl Iterator<Item = &PackInfo> {
    let catalog = &state.snapshot.catalog;
    state
        .itgdb
        .dedicated
        .iter()
        .filter_map(|name| state.index.by_loose_name.get(name).copied())
        .filter_map(|index| catalog.get(index))
        .filter(|pack| !banner_known(state, pack.id))
}

pub(super) fn clamp_doubles(state: &mut State) {
    let column = state.doubles_column.min(1);
    state.doubles_column = column;
    let len = doubles_column(state, column).len();
    let window = &mut state.doubles_window[column];
    *window = (*window).min(len.saturating_sub(1));
    let visible = lo::DBL_ROWS.min(len.saturating_sub(*window));
    state.doubles_row = state.doubles_row.min(visible.saturating_sub(1));
}

/// Re-derive the visible list.
///
/// Search wins over the tab: somebody who typed a query is asking a narrower
/// question than the tab they happened to leave open.
pub(super) fn rebuild_results(state: &mut State, keep_id: Option<u64>) {
    // The CSV views are built once per catalogue, so a rebuild is a copy of
    // a few hundred indices -- ahead of everything below, which would
    // otherwise start by listing all nine and a half thousand packs.
    if state.query.is_empty()
        && let Some(view) = state.index.csv_view(tab(state))
    {
        // Stamina and all-around are main lists, which hold only packs with a
        // banner; their whole set is described before they are drawn. Should
        // that fail, the set is shown whole rather than not at all. Keyboard
        // keeps every pack: most keyboard packs have no banner at all.
        let bannered = tab(state)
            .describe_view()
            .is_some_and(|set| state.describe.view_phase(set) == ViewPhase::Ready);
        // A pack whose page is open stays until the reader backs out of it,
        // banner or not: the set landing must not swap the page -- and the
        // pack a START would download -- out from under them.
        let open = (state.zone == Zone::Detail && !state.featured_from)
            .then_some(keep_id)
            .flatten();
        let list: Vec<usize> = if bannered {
            let catalog = &state.snapshot.catalog;
            view.iter()
                .copied()
                .filter(|index| {
                    let id = catalog[*index].id;
                    has_banner(state, id) || open == Some(id)
                })
                .collect()
        } else {
            view.to_vec()
        };
        state.results = list;
        state.can_load_more = false;
        state.search_why.clear();
        let catalog = &state.snapshot.catalog;
        state.cursor = keep_id
            .and_then(|id| {
                state
                    .results
                    .iter()
                    .position(|index| catalog[*index].id == id)
            })
            .unwrap_or(0);
        clamp_cursor(state);
        return;
    }

    let catalog = Arc::clone(&state.snapshot.catalog);
    let details = Arc::clone(&state.details);
    let searching = !state.query.is_empty();

    let mut results = if searching {
        // The search service has already scored and ordered these across all
        // three of its passes, so the order here is its order -- not one this
        // screen invents on top of it.
        let search = Arc::clone(&state.search);
        state.search_why.clear();
        if search.query == state.query {
            let mut ordered = Vec::with_capacity(search.hits.len());
            for hit in search.hits.iter() {
                if let Some(index) = state.index.by_id.get(&hit.pack_id).copied() {
                    ordered.push(index);
                    state.search_why.insert(hit.pack_id, hit.why.clone());
                }
            }
            ordered
        } else {
            // The service has not caught up with what was typed. Answer from
            // the catalogue's own names meanwhile rather than blanking the
            // list under the reader.
            search_catalog(&catalog, state.query.as_str())
        }
    } else {
        state.search_why.clear();
        (0..catalog.len()).collect()
    };

    // A paged view reads the site's own newest-first ordering and stops at the
    // end of what has been fetched. It never touches the rest of the
    // catalogue, which is the difference between filtering a couple of hundred
    // packs and filtering nine and a half thousand of them on every republish.
    if !searching && tab(state).is_paged() {
        let order = Arc::clone(&state.details.newest_first);
        let active = tab(state);
        let year = year_at(state.year_slot);
        let floor = lo::YEAR_FLOOR;
        let mut paged = Vec::with_capacity(order.len());
        for id in order.iter() {
            let Some(index) = state.index.by_id.get(id).copied() else {
                continue;
            };
            let pack = &catalog[index];
            let pack_details = details.by_id.get(id);
            // A pack with no banner is not in these lists, as the original
            // has it: a row that is only a "no picture" plate is not something
            // to browse by.
            if pack_details.is_none_or(|details| details.banner_url.is_none()) {
                continue;
            }
            let keep = match active {
                Tab::Pad => {
                    pack_details.is_none_or(PackDetails::is_dance_only)
                        && !matches_type(pack, "keyboard")
                }
                Tab::Years => {
                    let Some(added) = pack_details.and_then(PackDetails::year) else {
                        continue;
                    };
                    let in_slice = match year {
                        Some(wanted) => added == wanted,
                        None => added < floor,
                    };
                    in_slice
                        && pack_details.is_none_or(PackDetails::is_dance_only)
                        && !matches_type(pack, "keyboard")
                }
                _ => true,
            };
            if keep {
                paged.push(index);
            }
        }
        state.results = paged;
        // The year view indexes itself in the background, so a mark asking
        // the reader for more would be asking for something already coming.
        state.can_load_more = state.details.has_more && active == Tab::Pad;
        state.search_why.clear();
        let restored = keep_id.and_then(|id| {
            state
                .results
                .iter()
                .position(|index| catalog[*index].id == id)
        });
        state.cursor = restored.unwrap_or(0);
        clamp_cursor(state);
        return;
    }

    // The beginner walk decides its own list, one candidate at a time, and
    // hands it over already ordered.
    if !searching && tab(state) == Tab::Beginner {
        state.results = state
            .beginner
            .packs
            .iter()
            .filter_map(|id| state.index.by_id.get(id).copied())
            .collect();
        // Only once a walk has run: before that there is nothing to ask for
        // more of, and a mark on an empty list would fetch the wrong thing.
        state.can_load_more = !state.beginner.exhausted
            && matches!(
                state.beginner.phase,
                BeginnerPhase::Ready | BeginnerPhase::Loading
            );
        state.search_why.clear();
        let restored = keep_id.and_then(|id| {
            state
                .results
                .iter()
                .position(|index| catalog[*index].id == id)
        });
        state.cursor = restored.unwrap_or(0);
        clamp_cursor(state);
        return;
    }

    state.can_load_more = false;
    if !searching {
        let active = tab(state);
        let year = year_at(state.year_slot);
        let floor = lo::YEAR_FLOOR;
        results.retain(|index| {
            let pack = &catalog[*index];
            let pack_details = details.by_id.get(&pack.id);
            match active {
                // With no query there is no answer, and the whole catalogue is
                // not one. This arm is only ever reached with an empty query,
                // because a search that has one does not filter by tab at all.
                Tab::Search => false,
                // The ITGmania browser's rule: every reported game type must
                // be dance, and the pack must not be a keyboard pack. Chart
                // types carry the first half because the CSV's PackType is
                // blank for three quarters of the catalog.
                Tab::Pad => {
                    pack_details.is_none_or(PackDetails::is_dance_only)
                        && !matches_type(pack, "keyboard")
                }
                // The doubles view builds its own two columns; the single
                // list is not what it shows.
                Tab::Doubles => false,
                // The beginner list is the walk's own answer, in its own
                // order, so it is not filtered out of the catalogue here.
                Tab::Beginner => false,
                // Built once per catalogue and answered above.
                Tab::Keyboard | Tab::Stamina | Tab::AllAround => false,
                // A year is a view rather than a filter tab, so it ignores
                // whichever of pad/keyboard was last selected -- but it keeps
                // the pad rule, because that is what the view is for.
                Tab::Years => {
                    let Some(added) = pack_details.and_then(PackDetails::year) else {
                        return false;
                    };
                    let in_slice = match year {
                        Some(wanted) => added == wanted,
                        None => added < floor,
                    };
                    in_slice
                        && pack_details.is_none_or(PackDetails::is_dance_only)
                        && !matches_type(pack, "keyboard")
                }
                // The library grid builds itself from the song cache rather
                // than from the catalogue, so the single list is not what
                // this tab shows.
                Tab::Installed => false,
            }
        });
    }

    // No sort here. A search is already in its own relevance order, and every
    // tab that reaches this point with rows in it has been answered in order
    // above.
    state.results = results;

    let restored = keep_id.and_then(|id| {
        state
            .results
            .iter()
            .position(|index| catalog[*index].id == id)
    });
    state.cursor = restored.unwrap_or(0);
    clamp_cursor(state);
}

fn matches_type(pack: &PackInfo, wanted: &str) -> bool {
    pack.pack_type
        .as_deref()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(wanted))
}

fn matches_substyle(pack: &PackInfo, wanted: &str) -> bool {
    pack.substyle
        .as_deref()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(wanted))
}

pub(super) fn clamp_cursor(state: &mut State) {
    let last = state.results.len();
    if last == 0 {
        state.cursor = 0;
        return;
    }
    state.cursor = state.cursor.min(last - 1);
}

/// Rows the list has. The "more" mark is not one of them.
pub(super) fn row_count(state: &State) -> usize {
    state.results.len()
}

/// The banners this view wants right now.
///
/// Returned rather than requested from here because the theme has no network.
/// The service ignores anything already fetched, queued or failed, so this can
/// be called every frame and stay honest about what is visible.
pub fn wanted_banners(state: &State) -> Vec<(u64, String)> {
    /// Rows above and below the window to fetch ahead, so paging does not
    /// start from nothing.
    const LOOKAHEAD: usize = lo::ROWS;

    let mut out = Vec::new();
    let mut want = |state: &State, index: usize| {
        let Some(pack) = state.snapshot.catalog.get(index) else {
            return;
        };
        if let Some(url) = banner_url_for(state, pack.id) {
            out.push((pack.id, url.to_owned()));
        }
    };

    // The grid is the first thing anyone sees, so its artwork is asked for
    // whether or not the grid is currently showing.
    for index in state.featured.clone() {
        want(state, index);
    }

    let start = window_start(state).saturating_sub(LOOKAHEAD);
    let end = (window_start(state) + visible_rows() + LOOKAHEAD).min(state.results.len());
    for index in state.results.get(start..end).unwrap_or_default().to_vec() {
        want(state, index);
    }

    // Both doubles columns, since both are on screen at once.
    for column in 0..2 {
        let list = doubles_column(state, column);
        let first = state.doubles_window[column].saturating_sub(2);
        let last = (state.doubles_window[column] + lo::DBL_ROWS + 2).min(list.len());
        for index in list.get(first..last).unwrap_or_default().to_vec() {
            want(state, index);
        }
    }

    // The open detail page's song jackets, which are not pack banners at all
    // but go through the same fetcher.
    if state.zone == Zone::Detail
        && let Some(page) = state.page.page.as_ref()
    {
        let last = (state.song_window + lo::SONG_ROWS + 2).min(page.songs.len());
        for song in page.songs.get(state.song_window..last).unwrap_or_default() {
            if let Some(url) = song.image_url.as_deref() {
                out.push((song_art_key(url), url.to_owned()));
            }
        }
    }
    out
}

/// Texture key for a pack's banner. Must match the shell's own derivation --
/// they are the two halves of one name.
pub(super) fn banner_key(pack_id: u64) -> String {
    format!("smo-banner/{pack_id}")
}

/// Whether this artwork is never arriving, as opposed to not having arrived.
///
/// The two look identical to a row, and drawing the same wheel for both means
/// a pack whose banner 404s spins for as long as the screen is open.
pub(super) fn banner_is_lost(state: &State, key: u64) -> bool {
    state.banner_failed.contains(&key)
}

/// Whether anything has told us where this pack's banner is.
///
/// A spinner is a promise that something is coming. Nothing is coming for a
/// pack no service has given a URL for, so the row must say "no picture"
/// rather than turn a wheel until the screen closes.
///
/// A lookup in flight is something coming too, so its row spins rather than
/// flashing "no picture" for the second the answer takes.
pub(super) fn banner_expected(state: &State, pack_id: u64) -> bool {
    banner_url_for(state, pack_id).is_some()
        || state.describe.pending.contains(&pack_id)
        || (query_unsettled(state) && !banner_known(state, pack_id))
}

/// Whether the reader is still typing. The list is then the catalogue's own
/// name matches for a query about to change -- not rows worth asking the site
/// about, one lookup per keystroke.
fn query_unsettled(state: &State) -> bool {
    !state.query.is_empty() && state.query_idle < SEARCH_DEBOUNCE
}

/// Whether this pack has a banner to show.
pub(super) fn has_banner(state: &State, pack_id: u64) -> bool {
    banner_url_for(state, pack_id).is_some()
}

/// Whether anything has said whether this pack has a banner, either way.
fn banner_known(state: &State, pack_id: u64) -> bool {
    has_banner(state, pack_id)
        || state.details.by_id.contains_key(&pack_id)
        || state.describe.answered(pack_id)
}

/// The packs on screen that nothing has described, for the describe service
/// to look up: the list window, the curated doubles column whole, and the
/// focused pack. Bounded by what is visible, never by the catalogue, and
/// empty -- no allocation at all -- once everything showing is known.
pub fn wanted_descriptions(state: &State) -> Vec<(u64, &str)> {
    let mut out: Vec<(u64, &str)> = Vec::new();
    if state.snapshot.catalog.is_empty() || installed_showing(state) || query_unsettled(state) {
        return out;
    }
    if doubles_showing(state) {
        for pack in doubles_unknown(state) {
            want(state, &mut out, pack);
        }
        let list = doubles_column(state, 1);
        let first = state.doubles_window[1];
        let last = (first + lo::DBL_ROWS).min(list.len());
        for index in list.get(first..last).unwrap_or_default() {
            if let Some(pack) = state.snapshot.catalog.get(*index) {
                want(state, &mut out, pack);
            }
        }
    } else {
        let start = window_start(state);
        let end = (start + visible_rows()).min(state.results.len());
        for index in state.results.get(start..end).unwrap_or_default() {
            if let Some(pack) = state.snapshot.catalog.get(*index) {
                want(state, &mut out, pack);
            }
        }
    }
    if let Some(pack) = focused_pack(state) {
        want(state, &mut out, pack);
    }
    out
}

/// Add a pack to the lookups, once, if nothing has described it.
fn want<'a>(state: &State, out: &mut Vec<(u64, &'a str)>, pack: &'a PackInfo) {
    if !banner_known(state, pack.id) && !out.iter().any(|(id, _)| *id == pack.id) {
        out.push((pack.id, pack.name.as_str()));
    }
}

/// The substyle set the open tab needs described whole, if any.
pub fn wanted_view(state: &State) -> Option<View> {
    if !state.query.is_empty() {
        return None;
    }
    tab(state).describe_view()
}

/// Where this pack's banner is, from whichever service knows.
///
/// One place, so the row that draws it and the list that asks for it can
/// never disagree about whether a picture is coming. In order: the featured
/// card's small ranking art, the catalogue's own, then the ranking's art for
/// any pack it ranks.
pub(super) fn banner_url_for(state: &State, pack_id: u64) -> Option<&str> {
    state
        .popular_banner
        .get(&pack_id)
        .map(String::as_str)
        .or_else(|| {
            state
                .details
                .by_id
                .get(&pack_id)
                .and_then(|details| details.banner_url.as_deref())
        })
        .or_else(|| {
            state
                .describe
                .get(pack_id)
                .and_then(|details| details.banner_url.as_deref())
        })
        .or_else(|| {
            // The open pack's own page names its banner too.
            (state.page.pack_id == pack_id)
                .then(|| state.page.page.as_deref())
                .flatten()
                .and_then(|page| page.banner_url.as_deref())
        })
        .or_else(|| state.popular_art.get(&pack_id).map(String::as_str))
}

/// A cache key for one song's jacket.
///
/// Song art has no id of its own, only a URL, so the URL is hashed. The top
/// bit is set to keep the result out of the pack-id key space: pack ids are
/// small integers, so nothing there can collide with a key from this half.
pub(super) fn song_art_key(url: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    hasher.finish() | (1 << 63)
}

/// Whether itgdb files this pack under its doubles category, as opposed to it
/// merely containing a doubles chart.
pub(super) fn is_dedicated_doubles(state: &State, pack: &PackInfo) -> bool {
    state.itgdb.is_dedicated(pack.name.as_str())
}

pub(super) fn is_installed(state: &State, pack: &PackInfo) -> bool {
    // The library is tens of packs where the catalogue is thousands, so this
    // is the cheap direction to scan -- but the name is lowercased once per
    // call rather than once per library entry.
    let lower = pack.name.to_lowercase();
    state.installed.iter().any(|entry| entry.lower == lower)
}

/// The install queue's view of one pack, if it is in it.
pub(super) fn install_for<'a>(
    state: &'a State,
    pack: &PackInfo,
) -> Option<&'a deadsync_online::stepmaniaonline::InstallSnapshot> {
    state
        .snapshot
        .installs
        .iter()
        .find(|install| install.pack_id == pack.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_online::itgdb::{ItgdbPhase, normalize_name};
    use deadsync_online::smo_details::DetailsPhase;
    use deadsync_online::smo_search::SearchHit;
    use deadsync_online::stepmaniaonline::CatalogPhase;

    fn pack(id: u64, name: &str, kind: Option<&str>, substyle: Option<&str>) -> PackInfo {
        PackInfo::new(
            id,
            name.to_owned(),
            10,
            1024 * 1024,
            None,
            kind.map(str::to_owned),
            substyle.map(str::to_owned),
            None,
        )
    }

    fn catalog() -> Vec<PackInfo> {
        vec![
            pack(1, "Stamina One", Some("itg"), Some("stamina")),
            pack(2, "Tech One", Some("itg"), Some("technical")),
            pack(3, "Keys One", Some("keyboard"), Some("None")),
            pack(4, "Plain One", Some("None"), Some("None")),
            pack(5, "Pump Only", Some("None"), Some("None")),
        ]
    }

    fn details_with_doubles(
        entries: &[(u64, &str, &[&str])],
        doubles: &[u64],
    ) -> Arc<DetailsSnapshot> {
        let mut by_id = HashMap::new();
        let mut newest_first = Vec::new();
        for (id, date, types) in entries {
            by_id.insert(
                *id,
                PackDetails {
                    banner_url: Some(format!("https://example.test/{id}.jpg")),
                    date_added: Some((*date).to_owned()),
                    chart_types: types.iter().map(|t| (*t).to_owned()).collect(),
                },
            );
            newest_first.push(*id);
        }
        Arc::new(DetailsSnapshot {
            phase: DetailsPhase::Ready,
            by_id: Arc::new(by_id),
            newest_first: Arc::from(newest_first),
            doubles: Arc::new(doubles.iter().copied().collect()),
            has_more: false,
            revision: 1,
            message: None,
        })
    }

    fn details(entries: &[(u64, &str, &[&str])]) -> Arc<DetailsSnapshot> {
        details_with_doubles(entries, &[])
    }

    fn itgdb(names: &[&str]) -> Arc<ItgdbSnapshot> {
        Arc::new(ItgdbSnapshot {
            phase: ItgdbPhase::Ready,
            dedicated: Arc::new(names.iter().map(|n| normalize_name(n)).collect()),
            revision: 1,
            message: None,
        })
    }

    fn ready(state: &mut State, details: Arc<DetailsSnapshot>, installed: Vec<&str>) {
        ready_with(
            state,
            details,
            Arc::new(ItgdbSnapshot::default()),
            installed,
        );
    }

    fn library(names: &[&str]) -> Vec<InstalledPack> {
        names
            .iter()
            .map(|name| InstalledPack {
                name: (*name).to_owned(),
                lower: name.to_lowercase(),
                songs: 10,
                sync: SyncPref::Default,
                banner: None,
            })
            .collect()
    }

    fn ready_with(
        state: &mut State,
        details: Arc<DetailsSnapshot>,
        itgdb: Arc<ItgdbSnapshot>,
        installed: Vec<&str>,
    ) {
        let snapshot = Snapshot {
            phase: CatalogPhase::Ready,
            catalog: Arc::from(catalog()),
            revision: 1,
            message: None,
            installs: Vec::new(),
        };
        sync_stepmaniaonline(
            state,
            Services {
                catalog: Arc::new(snapshot),
                details,
                itgdb,
                ..Services::default()
            },
            Vec::new(),
            Some(library(installed.as_slice())),
        );
    }

    fn select_tab(state: &mut State, wanted: Tab) {
        state.tab_index = TABS.iter().position(|t| *t == wanted).expect("tab exists");
        rebuild_results(state, None);
    }

    fn names(state: &State) -> Vec<String> {
        state
            .results
            .iter()
            .map(|index| state.snapshot.catalog[*index].name.clone())
            .collect()
    }

    fn column_names(state: &State, column: usize) -> Vec<String> {
        doubles_column(state, column)
            .iter()
            .map(|index| state.snapshot.catalog[*index].name.clone())
            .collect()
    }

    /// A paged view is exactly the page the site handed over, in the order it
    /// handed it over -- no local sort, and nothing from outside the page.
    ///
    /// That is the whole saving: the catalogue is nine and a half thousand
    /// packs, and a view built from it filtered and sorted all of them every
    /// time any of the six services republished.
    #[test]
    fn a_paged_view_is_the_page_the_site_sent_in_its_order() {
        let mut state = init();
        ready(
            &mut state,
            details(&[(4, "2026-01-01", &["dance"]), (1, "2020-01-01", &["dance"])]),
            Vec::new(),
        );
        select_tab(&mut state, Tab::Pad);

        assert_eq!(
            names(&state),
            vec!["Plain One", "Stamina One"],
            "the page, in the site's order, and nothing else"
        );
    }

    fn ready_catalog(state: &mut State, catalog: Vec<PackInfo>, details: Arc<DetailsSnapshot>) {
        let snapshot = Snapshot {
            phase: CatalogPhase::Ready,
            catalog: Arc::from(catalog),
            revision: 1,
            message: None,
            installs: Vec::new(),
        };
        sync_stepmaniaonline(
            state,
            Services {
                catalog: Arc::new(snapshot),
                details,
                ..Services::default()
            },
            Vec::new(),
            Some(Vec::new()),
        );
    }

    /// The tabs the CSV can answer whole are not paged: they are closed sets
    /// of a few hundred packs, and paging them would hide rows the browser
    /// already holds. They are newest pack first by id, as the original's are
    /// -- known for every pack, where the details service's dates cover only
    /// the newest couple of hundred and left everything else sorted by name.
    #[test]
    fn the_csv_views_are_newest_pack_first() {
        let mut state = init();
        ready_catalog(
            &mut state,
            vec![
                pack(10, "Alpha Keys", Some("keyboard"), Some("None")),
                pack(30, "Zeta Keys", Some("keyboard"), Some("None")),
                pack(20, "Mid Keys", Some("keyboard"), Some("technical")),
                pack(11, "Old Tech", Some("itg"), Some("technical")),
                pack(40, "New All Around", Some("itg"), Some("all around")),
                pack(25, "Some Stamina", Some("itg"), Some("stamina")),
                pack(5, "Ancient Stamina", Some("None"), Some("Stamina")),
            ],
            // A date that says the oldest keyboard pack is the newest. The
            // order does not follow it: it follows the id, which every pack
            // has, rather than a date only a few do.
            details(&[(10, "2026-06-01", &["dance"])]),
        );

        assert!(!Tab::Keyboard.is_paged(), "199 packs is not worth paging");
        select_tab(&mut state, Tab::Keyboard);
        assert_eq!(names(&state), vec!["Zeta Keys", "Mid Keys", "Alpha Keys"]);
        assert!(!state.can_load_more, "a whole set has no more to fetch");

        // technical and all-around together, and never a keyboard pack
        select_tab(&mut state, Tab::AllAround);
        assert_eq!(names(&state), vec!["New All Around", "Old Tech"]);

        select_tab(&mut state, Tab::Stamina);
        assert_eq!(names(&state), vec!["Some Stamina", "Ancient Stamina"]);

        // and none of them waits for the details service to put it in order
        select_tab(&mut state, Tab::Keyboard);
        assert!(!awaiting_order(&state));
    }

    /// A pack with no banner is not in the pad or year lists, as the original
    /// has it. The site's "NO BANNER" stand-in counts as none.
    #[test]
    fn a_pack_with_no_banner_is_not_in_the_pad_list() {
        let mut state = init();
        let mut snap =
            (*details(&[(4, "2026-01-01", &["dance"]), (1, "2025-01-01", &["dance"])])).clone();
        let mut by_id = (*snap.by_id).clone();
        by_id.get_mut(&1).expect("pack 1").banner_url = None;
        snap.by_id = Arc::new(by_id);
        ready(&mut state, Arc::new(snap), Vec::new());
        select_tab(&mut state, Tab::Pad);
        assert_eq!(names(&state), vec!["Plain One"]);
    }

    /// The ITGmania rule: EVERY reported game type must be dance, and the
    /// pack must not be a keyboard pack. A dance+pump pack is a multi-game
    /// pack, and most of what it downloads is unplayable on a pad.
    #[test]
    fn the_pad_tab_requires_every_type_to_be_dance_and_excludes_keyboard() {
        let mut state = init();
        ready(
            &mut state,
            details(&[
                (5, "2026-01-01", &["pump"]),
                (4, "2025-01-01", &["dance"]),
                (2, "2024-01-01", &["dance", "pump"]),
                (3, "2023-01-01", &["dance"]),
            ]),
            Vec::new(),
        );
        select_tab(&mut state, Tab::Pad);

        let listed = names(&state);
        assert!(
            listed.contains(&"Plain One".to_owned()),
            "dance-only belongs"
        );
        assert!(
            !listed.contains(&"Stamina One".to_owned()),
            "a pack the page does not carry is not in the page"
        );
        assert!(
            !listed.contains(&"Pump Only".to_owned()),
            "pump is not a pad game"
        );
        assert!(
            !listed.contains(&"Tech One".to_owned()),
            "dance+pump is multi-game"
        );
        assert!(
            !listed.contains(&"Keys One".to_owned()),
            "a keyboard pack is excluded even with dance charts"
        );
    }

    /// Doubles is two lists side by side, not one list with a seam in it:
    /// itgdb's curated category, and the packs the site merely says carry a
    /// doubles chart. A pack in the first is never repeated in the second.
    #[test]
    fn doubles_splits_the_curated_list_from_the_candidates() {
        let mut state = init();
        ready_with(
            &mut state,
            details_with_doubles(
                &[
                    (1, "2026-01-01", &["dance"]),
                    (2, "2025-01-01", &["dance"]),
                    (4, "2024-01-01", &["dance"]),
                ],
                // the site says all three carry a doubles chart
                &[1, 2, 4],
            ),
            // itgdb knows this one by a differently-punctuated name
            itgdb(&["plain  one!"]),
            Vec::new(),
        );

        assert_eq!(column_names(&state, 0), vec!["Plain One"]);
        // newest pack first, as every other list here
        assert_eq!(column_names(&state, 1), vec!["Tech One", "Stamina One"]);
        // and the single list is not what this view shows
        select_tab(&mut state, Tab::Doubles);
        assert!(state.results.is_empty());
    }

    /// Both doubles columns are main lists, so both hold only packs with a
    /// banner. The candidates were described by the doubles pass and are
    /// decided at once; a curated pack joins its column when a lookup
    /// confirms it has art, and the column says it is filling in meanwhile.
    #[test]
    fn doubles_lists_hold_only_packs_with_banners() {
        let mut state = init();
        let mut snap = (*details_with_doubles(
            &[(1, "2026-01-01", &["dance"]), (2, "2025-01-01", &["dance"])],
            &[1, 2],
        ))
        .clone();
        let mut by_id = (*snap.by_id).clone();
        by_id.get_mut(&2).expect("pack 2").banner_url = None;
        snap.by_id = Arc::new(by_id);
        // itgdb curates a pack no details page described
        ready_with(&mut state, Arc::new(snap), itgdb(&["keys one"]), Vec::new());
        select_tab(&mut state, Tab::Doubles);

        assert_eq!(
            column_names(&state, 1),
            vec!["Stamina One"],
            "no banner, no row"
        );
        assert!(
            column_names(&state, 0).is_empty(),
            "not until its art is known"
        );
        assert_eq!(
            wanted_descriptions(&state)
                .iter()
                .map(|(id, _)| *id)
                .collect::<Vec<_>>(),
            vec![3],
            "so it is looked up"
        );

        state.describe = Arc::new(DescribeSnapshot {
            pending: Arc::new(HashSet::from([3])),
            ..DescribeSnapshot::default()
        });
        rebuild_doubles(&mut state);
        assert!(state.doubles_left_waiting, "filling in, not empty");
        assert!(banner_expected(&state, 3), "and its row would spin");

        state.describe = Arc::new(DescribeSnapshot {
            by_id: Arc::new(HashMap::from([(
                3,
                PackDetails {
                    banner_url: Some("https://example.test/3.jpg".to_owned()),
                    ..PackDetails::default()
                },
            )])),
            ..DescribeSnapshot::default()
        });
        rebuild_doubles(&mut state);
        assert_eq!(column_names(&state, 0), vec!["Keys One"]);
        assert!(!state.doubles_left_waiting);
        assert!(
            wanted_descriptions(&state).is_empty(),
            "nothing left to ask"
        );
    }

    /// Stamina and all-around hold only packs with a banner, so they wait for
    /// their whole set to be described. Keyboard keeps every pack -- most
    /// keyboard packs have no banner -- and so does a set that could not be
    /// described, rather than showing nothing.
    #[test]
    fn stamina_lists_only_packs_with_banners_once_its_set_is_described() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());
        select_tab(&mut state, Tab::Stamina);
        assert!(awaiting_order(&state), "the set is not described yet");
        assert_eq!(wanted_view(&state), Some(View::Stamina));

        let described = |banner: Option<&str>| {
            Arc::new(DescribeSnapshot {
                view_rows: Arc::new(HashMap::from([(
                    1,
                    PackDetails {
                        banner_url: banner.map(str::to_owned),
                        ..PackDetails::default()
                    },
                )])),
                stamina: ViewPhase::Ready,
                ..DescribeSnapshot::default()
            })
        };
        state.describe = described(None);
        select_tab(&mut state, Tab::Stamina);
        assert!(!awaiting_order(&state));
        assert!(state.results.is_empty(), "its only pack has no banner");

        state.describe = described(Some("https://example.test/1.jpg"));
        select_tab(&mut state, Tab::Stamina);
        assert_eq!(names(&state), vec!["Stamina One"]);

        state.describe = Arc::new(DescribeSnapshot {
            stamina: ViewPhase::Error,
            ..DescribeSnapshot::default()
        });
        select_tab(&mut state, Tab::Stamina);
        assert_eq!(names(&state), vec!["Stamina One"], "shown whole on failure");

        select_tab(&mut state, Tab::Keyboard);
        assert_eq!(names(&state), vec!["Keys One"], "keyboard needs no banner");
        assert_eq!(wanted_view(&state), None);
    }

    /// Search and keyboard rows no details page described are looked up while
    /// on screen, and draw from the answer: art, date and types.
    #[test]
    fn rows_nothing_described_are_looked_up_and_drawn_from_the_answer() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());
        select_tab(&mut state, Tab::Keyboard);
        let keys = state.snapshot.catalog[state.results[0]].clone();
        assert_eq!(
            wanted_descriptions(&state),
            vec![(keys.id, keys.name.as_str())],
            "the row on screen, once, though it is also the focused pack"
        );
        assert!(!banner_expected(&state, keys.id), "nothing coming yet");

        state.describe = Arc::new(DescribeSnapshot {
            by_id: Arc::new(HashMap::from([(
                keys.id,
                PackDetails {
                    banner_url: Some("https://example.test/k.jpg".to_owned()),
                    date_added: Some("2024-05-01".to_owned()),
                    chart_types: vec!["kb7".to_owned()],
                },
            )])),
            ..DescribeSnapshot::default()
        });
        assert_eq!(
            banner_url_for(&state, keys.id),
            Some("https://example.test/k.jpg")
        );
        assert_eq!(
            details_for(&state, &keys).and_then(|d| d.date_added.as_deref()),
            Some("2024-05-01")
        );
        assert!(wanted_descriptions(&state).is_empty());

        // a lookup that found no row is an answer too, and is not asked again
        state.describe = Arc::new(DescribeSnapshot {
            missing: Arc::new(HashSet::from([keys.id])),
            ..DescribeSnapshot::default()
        });
        assert!(wanted_descriptions(&state).is_empty());
    }

    /// itgdb is optional: without it the right column still answers from SMO
    /// alone, and the left one is simply empty.
    #[test]
    fn doubles_still_works_when_itgdb_is_unreachable() {
        let mut state = init();
        ready(
            &mut state,
            details_with_doubles(&[(1, "2026-01-01", &["dance"])], &[1]),
            Vec::new(),
        );
        assert!(column_names(&state, 0).is_empty());
        assert_eq!(column_names(&state, 1), vec!["Stamina One"]);
    }

    #[test]
    fn substyle_tabs_select_on_the_csv_column() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());

        select_tab(&mut state, Tab::Stamina);
        assert_eq!(names(&state), vec!["Stamina One"]);
        select_tab(&mut state, Tab::AllAround);
        assert_eq!(names(&state), vec!["Tech One"], "technical is all-around");
    }

    #[test]
    fn the_keyboard_tab_selects_on_pack_type() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());
        select_tab(&mut state, Tab::Keyboard);
        assert_eq!(names(&state), vec!["Keys One"]);
    }

    /// The strip is a fixed run of years back to the floor plus one OLDER
    /// bucket, exactly as the original -- not whichever years the catalogue
    /// happened to mention, which changed shape as the walk went on.
    #[test]
    fn the_year_strip_is_fixed_and_ends_in_an_older_bucket() {
        let last = lo::year_slots() - 1;
        assert_eq!(year_at(0), Some(lo::current_year()));
        assert_eq!(year_at(1), Some(lo::current_year() - 1));
        assert_eq!(year_at(last), None, "the last slot is OLDER");
        assert_eq!(year_label(last), "OLDER");
        assert_eq!(year_at(last - 1), Some(lo::YEAR_FLOOR));
    }

    #[test]
    fn a_year_slot_selects_that_years_packs_and_older_takes_the_rest() {
        let mut state = init();
        ready(
            &mut state,
            details(&[
                (1, "2026-05-04", &["dance"]),
                (2, "2020-02-02", &["dance"]),
                (4, "2014-02-02", &["dance"]),
            ]),
            Vec::new(),
        );

        select_tab(&mut state, Tab::Years);
        state.year_slot = 0;
        rebuild_results(&mut state, None);
        assert_eq!(names(&state), vec!["Stamina One"], "the current year");

        state.year_slot = (lo::current_year() - 2020) as usize;
        rebuild_results(&mut state, None);
        assert_eq!(names(&state), vec!["Tech One"]);

        state.year_slot = lo::year_slots() - 1;
        rebuild_results(&mut state, None);
        assert_eq!(names(&state), vec!["Plain One"], "before the floor");
    }

    /// The end of a paged list is marked under its last row rather than being
    /// a row of its own -- so it never takes a slot, and never makes a page.
    #[test]
    fn a_paged_list_marks_its_end_instead_of_adding_a_row() {
        let mut state = init();
        let mut snap = details(&[(4, "2026-01-01", &["dance"])]);
        snap = Arc::new(DetailsSnapshot {
            has_more: true,
            ..(*snap).clone()
        });
        ready(&mut state, Arc::clone(&snap), Vec::new());
        select_tab(&mut state, Tab::Pad);

        assert_eq!(state.results.len(), 1);
        assert!(state.can_load_more);
        assert_eq!(row_count(&state), 1, "the mark is not a row");
        assert_eq!(total_pages(&state), 1);
        assert_eq!(more_mark(&state), Some(false));
        assert!(at_list_end(&state), "Down from the last pack asks for more");

        // The year view indexes itself in the background, so it never asks.
        let mut years = init();
        ready(&mut years, snap, Vec::new());
        select_tab(&mut years, Tab::Years);
        assert!(!years.can_load_more);
        assert_eq!(more_mark(&years), None);

        // and a view with everything already fetched has no mark
        let mut settled = init();
        ready(
            &mut settled,
            details(&[(4, "2026-01-01", &["dance"])]),
            Vec::new(),
        );
        select_tab(&mut settled, Tab::Pad);
        assert!(!settled.can_load_more);
        assert_eq!(more_mark(&settled), None);
    }

    fn many(count: u64) -> Vec<PackInfo> {
        (1..=count)
            .map(|id| pack(id, &format!("Pack {id:02}"), Some("None"), Some("None")))
            .collect()
    }

    /// A catalogue of twenty packs, and a beginner walk that has found these.
    fn ready_beginner(state: &mut State, found: &[u64], phase: BeginnerPhase, exhausted: bool) {
        let snapshot = Snapshot {
            phase: CatalogPhase::Ready,
            catalog: Arc::from(many(20)),
            revision: 1,
            message: None,
            installs: Vec::new(),
        };
        sync_stepmaniaonline(
            state,
            Services {
                catalog: Arc::new(snapshot),
                details: details(&[]),
                beginner: Arc::new(BeginnerSnapshot {
                    phase,
                    packs: Arc::from(found.to_vec()),
                    checked: found.len(),
                    candidates: 20,
                    exhausted,
                    revision: 1,
                    message: None,
                }),
                ..Services::default()
            },
            Vec::new(),
            Some(Vec::new()),
        );
        select_tab(state, Tab::Beginner);
    }

    /// A helping is a page, and the mark asking for the next one sits at the
    /// foot of that page -- not alone on the next one, where nobody reading
    /// the first screen would find it.
    #[test]
    fn a_full_beginner_helping_keeps_its_more_mark_on_the_first_page() {
        let first: Vec<u64> = (1..=7).collect();
        let mut state = init();
        ready_beginner(&mut state, &first, BeginnerPhase::Ready, false);
        assert_eq!(state.results.len(), lo::ROWS);
        assert_eq!(total_pages(&state), 1, "seven packs are one page");
        assert_eq!(list_page(&state), 0);
        assert_eq!(more_mark(&state), Some(false));

        // A second helping is a second page, and the mark follows the list's
        // end onto it.
        let second: Vec<u64> = (1..=14).collect();
        let mut more = init();
        ready_beginner(&mut more, &second, BeginnerPhase::Ready, false);
        assert_eq!(total_pages(&more), 2);
        assert_eq!(
            more_mark(&more),
            None,
            "not under a page the list goes past"
        );
        more.cursor = 13;
        assert_eq!(more_mark(&more), Some(false));
    }

    /// Nothing to ask for more of before a walk has run, "finding more" while
    /// one is, and no mark once the list is complete.
    #[test]
    fn the_beginner_list_offers_more_only_once_a_walk_has_run() {
        let mut idle = init();
        ready_beginner(&mut idle, &[], BeginnerPhase::Idle, false);
        assert!(
            !idle.can_load_more,
            "a lone mark would fetch the wrong thing"
        );
        assert_eq!(more_mark(&idle), None);
        assert!(
            beginner_building(&idle),
            "the popularity list is on its way"
        );
        idle.popular = Arc::new(PopularSnapshot {
            phase: PopularPhase::Error,
            ..PopularSnapshot::default()
        });
        assert!(
            !beginner_building(&idle),
            "nothing is coming once the popularity list failed"
        );

        let mut walking = init();
        ready_beginner(&mut walking, &[1, 2], BeginnerPhase::Loading, false);
        assert!(beginner_building(&walking));
        assert_eq!(more_mark(&walking), Some(true), "finding more");

        let mut done = init();
        ready_beginner(&mut done, &[1, 2], BeginnerPhase::Ready, true);
        assert!(!done.can_load_more);
        assert_eq!(more_mark(&done), None, "a complete list has no mark");

        // A helping that found nothing -- every page it read failed, say --
        // is still askable, or the tab is a dead end for the session.
        let mut empty = init();
        ready_beginner(&mut empty, &[], BeginnerPhase::Ready, false);
        assert!(empty.results.is_empty());
        assert!(empty.can_load_more);
        assert!(at_list_end(&empty));
        assert_eq!(more_mark(&empty), Some(false));
    }

    /// Lookups for the curated doubles column that land while another tab is
    /// open are still applied when the doubles tab shows again.
    #[test]
    fn doubles_lookups_that_land_elsewhere_are_not_lost() {
        let mut state = init();
        let snapshot = Arc::new(Snapshot {
            phase: CatalogPhase::Ready,
            catalog: Arc::from(catalog()),
            revision: 1,
            message: None,
            installs: Vec::new(),
        });
        let services = |describe: Arc<DescribeSnapshot>| Services {
            catalog: Arc::clone(&snapshot),
            details: details(&[]),
            itgdb: itgdb(&["keys one"]),
            describe,
            ..Services::default()
        };
        let pending = Arc::new(DescribeSnapshot {
            pending: Arc::new(HashSet::from([3])),
            revision: 1,
            ..DescribeSnapshot::default()
        });
        sync_stepmaniaonline(&mut state, services(Arc::clone(&pending)), Vec::new(), None);
        select_tab(&mut state, Tab::Doubles);
        sync_stepmaniaonline(&mut state, services(pending), Vec::new(), None);
        assert!(state.doubles_left_waiting);

        // the answer lands while PAD is open
        select_tab(&mut state, Tab::Pad);
        let answered = Arc::new(DescribeSnapshot {
            by_id: Arc::new(HashMap::from([(
                3,
                PackDetails {
                    banner_url: Some("https://example.test/3.jpg".to_owned()),
                    ..PackDetails::default()
                },
            )])),
            revision: 2,
            ..DescribeSnapshot::default()
        });
        sync_stepmaniaonline(
            &mut state,
            services(Arc::clone(&answered)),
            Vec::new(),
            None,
        );

        // and nothing new is published by the time DOUBLES is back
        select_tab(&mut state, Tab::Doubles);
        sync_stepmaniaonline(&mut state, services(answered), Vec::new(), None);
        assert_eq!(column_names(&state, 0), vec!["Keys One"]);
        assert!(!state.doubles_left_waiting);
    }

    /// No lookups for a query still being typed: its list is the catalogue's
    /// own name matches, about to change on the next key.
    #[test]
    fn a_query_being_typed_asks_the_site_about_nothing() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());
        select_tab(&mut state, Tab::Search);
        state.query = "one".to_owned();
        state.query_idle = 0.0;
        rebuild_results(&mut state, None);
        assert!(!state.results.is_empty(), "the catalogue answers meanwhile");
        assert!(wanted_descriptions(&state).is_empty());
        let first = state.snapshot.catalog[state.results[0]].id;
        assert!(
            banner_expected(&state, first),
            "its rows wait rather than go blank"
        );

        state.query_idle = SEARCH_DEBOUNCE;
        assert!(
            !wanted_descriptions(&state).is_empty(),
            "and ask once it rests"
        );
    }

    /// The set landing does not swap an open pack page out from under the
    /// reader, even when that pack turns out to have no banner.
    #[test]
    fn an_open_pack_survives_its_set_landing() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());
        state.describe = Arc::new(DescribeSnapshot {
            stamina: ViewPhase::Error,
            ..DescribeSnapshot::default()
        });
        select_tab(&mut state, Tab::Stamina);
        assert_eq!(names(&state), vec!["Stamina One"]);
        state.zone = Zone::Detail;

        // the retry lands: the set is described, and this pack has no banner
        state.describe = Arc::new(DescribeSnapshot {
            view_rows: Arc::new(HashMap::from([(1, PackDetails::default())])),
            stamina: ViewPhase::Ready,
            ..DescribeSnapshot::default()
        });
        rebuild_results(&mut state, Some(1));
        assert_eq!(names(&state), vec!["Stamina One"], "still open");

        // and it leaves the list once the reader backs out
        state.zone = Zone::List;
        rebuild_results(&mut state, Some(1));
        assert!(state.results.is_empty());
    }

    /// A list drawn before its order is known is a list in the wrong order,
    /// and it reshuffles under the reader when the order arrives.
    #[test]
    fn a_list_is_not_drawn_before_its_order_is_known() {
        let mut state = init();
        let snapshot = Snapshot {
            phase: CatalogPhase::Ready,
            catalog: Arc::from(catalog()),
            revision: 1,
            message: None,
            installs: Vec::new(),
        };
        // the catalogue has landed but the ordering service has not
        let loading = DetailsSnapshot {
            phase: DetailsPhase::Loading,
            ..DetailsSnapshot::default()
        };
        sync_stepmaniaonline(
            &mut state,
            Services {
                catalog: Arc::new(snapshot),
                details: Arc::new(loading),
                ..Services::default()
            },
            Vec::new(),
            Some(Vec::new()),
        );
        select_tab(&mut state, Tab::Pad);
        assert!(awaiting_order(&state), "wait rather than show it wrong");

        // the library is local, so it never waits on the network to be ordered
        select_tab(&mut state, Tab::Installed);
        assert!(!awaiting_order(&state));

        // and once one page of the walk has landed, the list is drawable
        select_tab(&mut state, Tab::Pad);
        ready(
            &mut state,
            details(&[(1, "2026-01-01", &["dance"])]),
            Vec::new(),
        );
        assert!(!awaiting_order(&state));
    }

    /// Deciding a candidate costs reading its page, so the ones that cannot
    /// possibly pass are dropped before a request is spent on them.
    #[test]
    fn the_beginner_walk_does_not_spend_requests_on_hopeless_candidates() {
        let mut state = init();
        // every catalogue pack is popular, so only the rules do the filtering
        let popular = PopularSnapshot {
            phase: deadsync_online::popular_packs::PopularPhase::Ready,
            packs: Arc::from(
                catalog()
                    .iter()
                    .map(|pack| deadsync_online::popular_packs::PopularPack {
                        name: pack.name.clone(),
                        popularity: 1.0,
                        simfile_count: 1,
                        // every one with art but one
                        banner_url: (pack.name != "Tech One")
                            .then(|| format!("https://example.test/{}.jpg", pack.id)),
                    })
                    .collect::<Vec<_>>(),
            ),
            revision: 1,
            message: None,
        };
        let snapshot = Snapshot {
            phase: CatalogPhase::Ready,
            catalog: Arc::from(catalog()),
            revision: 1,
            message: None,
            installs: Vec::new(),
        };
        sync_stepmaniaonline(
            &mut state,
            Services {
                catalog: Arc::new(snapshot),
                details: details(&[(5, "2026-01-01", &["pump"])]),
                popular: Arc::new(popular),
                ..Services::default()
            },
            Vec::new(),
            Some(Vec::new()),
        );

        let names: Vec<String> = beginner_candidates(&state)
            .into_iter()
            .filter_map(|id| {
                state
                    .snapshot
                    .catalog
                    .iter()
                    .find(|pack| pack.id == id)
                    .map(|pack| pack.name.clone())
            })
            .collect();

        // a pack the site calls stamina cannot be mostly easy charts
        assert!(!names.contains(&"Stamina One".to_owned()));
        // a keyboard pack is not a pad pack
        assert!(!names.contains(&"Keys One".to_owned()));
        // and a pump pack is not a dance pack
        assert!(!names.contains(&"Pump Only".to_owned()));
        // a pack with no banner could not be shown in the list anyway
        assert!(!names.contains(&"Tech One".to_owned()));
        // what is left is worth a request
        assert_eq!(names, vec!["Plain One".to_owned()]);
    }

    /// The walk only runs while its own tab is open. It is the most expensive
    /// thing this screen does -- about four pack pages read per row kept.
    #[test]
    fn the_beginner_walk_runs_only_on_its_own_tab() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());
        state.popular = Arc::new(PopularSnapshot {
            phase: deadsync_online::popular_packs::PopularPhase::Ready,
            packs: Arc::from(vec![deadsync_online::popular_packs::PopularPack {
                name: "Plain One".to_owned(),
                popularity: 1.0,
                simfile_count: 1,
                banner_url: None,
            }]),
            revision: 1,
            message: None,
        });

        select_tab(&mut state, Tab::Pad);
        assert!(!wants_beginner_walk(&state));

        select_tab(&mut state, Tab::Beginner);
        assert!(wants_beginner_walk(&state));

        // Only the first walk starts unasked: once it has run, later helpings
        // are asked for. That also keeps the shell from building candidates
        // every frame the tab is open.
        state.beginner = Arc::new(BeginnerSnapshot {
            phase: BeginnerPhase::Ready,
            ..BeginnerSnapshot::default()
        });
        assert!(!wants_beginner_walk(&state));
        state.beginner = Arc::new(BeginnerSnapshot::default());
        assert!(wants_beginner_walk(&state));

        // and never before there is a ranking to walk
        state.popular = Arc::new(PopularSnapshot::default());
        assert!(!wants_beginner_walk(&state));
    }

    /// Nine tabs, in the original's order. The strip is what a reader learns
    /// first, so a port that reorders it is a port they have to relearn.
    #[test]
    fn the_strip_is_the_originals_nine_in_its_own_order() {
        assert_eq!(
            TABS.map(Tab::label),
            [
                "SEARCH",
                "PAD",
                "KEYBOARD",
                "BEGINNER",
                "ALL AROUND",
                "STAMINA",
                "DOUBLES",
                "YEARS",
                "INSTALLED",
            ]
        );
    }

    /// Every view without the featured grid above it is headed by the band.
    /// A filtered list with nothing saying what filtered it is the complaint
    /// this answers.
    #[test]
    fn every_view_without_a_grid_has_a_heading() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());

        for (wanted, expected) in [
            (Tab::Keyboard, Some("KEYBOARD PACKS")),
            (Tab::AllAround, Some("ALL AROUND")),
            (Tab::Stamina, Some("HARD CONTENT / STAMINA")),
            (Tab::Doubles, Some("DOUBLES")),
            // these three head themselves: the grid, the year strip, a count
            (Tab::Pad, None),
            (Tab::Years, None),
            (Tab::Installed, None),
        ] {
            select_tab(&mut state, wanted);
            assert_eq!(band(&state).map(|(title, _)| title), expected, "{wanted:?}");
        }

        // and a running search heads every tab, whichever one is open
        select_tab(&mut state, Tab::Pad);
        state.query = "stam".to_owned();
        assert_eq!(band(&state).map(|(title, _)| title), Some("SEARCH RESULTS"));
    }

    /// The library grid is built from the game's own song cache, not from the
    /// catalogue -- a pack can be installed without ever having come from the
    /// site -- but a name the catalogue also knows is matched case-blind, so
    /// the rest of the browser can badge it "In Library".
    #[test]
    fn the_library_is_matched_to_the_catalogue_case_insensitively() {
        let mut state = init();
        ready(&mut state, details(&[]), vec!["tech one"]);

        let tech = state
            .snapshot
            .catalog
            .iter()
            .find(|pack| pack.name == "Tech One")
            .expect("in the fixture catalogue");
        assert!(is_installed(&state, tech));

        let entry = state.installed.first().expect("one library pack");
        assert_eq!(
            installed_catalog_entry(&state, entry).map(|pack| pack.name.as_str()),
            Some("Tech One")
        );

        // and the tab shows the grid rather than the single list
        select_tab(&mut state, Tab::Installed);
        assert!(installed_showing(&state));
        assert!(state.results.is_empty(), "the grid is not the list");
    }

    /// The SEARCH tab with nothing typed has no answer, and the whole
    /// catalogue is not one. The box is the instruction, so the list below it
    /// shows the shape of an answer rather than a sentence telling the reader
    /// what to press.
    #[test]
    fn the_search_tab_shows_nothing_until_something_is_searched_for() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());
        select_tab(&mut state, Tab::Search);
        assert!(state.results.is_empty());
        assert!(search_field_showing(&state), "the box is the instruction");

        // and with a query it is the search's own answer, not a tab filter
        state.query = "stamina".to_owned();
        rebuild_results(&mut state, None);
        assert_eq!(names(&state), vec!["Stamina One"]);
    }

    /// The sync column has its own vocabulary, and `null` is a value in it.
    ///
    /// Running it through the generic "is this field set" filter reported
    /// 1,248 packs as having no sync listed when the site had said precisely
    /// what their sync was.
    #[test]
    fn a_null_sync_is_an_answer_rather_than_an_empty_field() {
        assert_eq!(SmoSync::read(Some("null")), SmoSync::Null);
        assert_eq!(SmoSync::read(Some("NULL")), SmoSync::Null);
        assert_eq!(SmoSync::read(Some("0")), SmoSync::Null);
        assert_eq!(SmoSync::read(Some("mixed")), SmoSync::Mixed);
        assert_eq!(SmoSync::read(Some("other")), SmoSync::Mixed);
        // these genuinely are absent
        assert_eq!(SmoSync::read(Some("n/a")), SmoSync::Unknown);
        assert_eq!(SmoSync::read(Some("")), SmoSync::Unknown);
        assert_eq!(SmoSync::read(None), SmoSync::Unknown);

        assert_eq!(SmoSync::Null.short(), "NULL (0 ms), per SMO");
        assert_ne!(SmoSync::Null.short(), SmoSync::Unknown.short());
    }

    /// Every lookup the screen makes is a map built once per catalogue, not a
    /// scan per row -- twenty-two library cells a frame against nine and a
    /// half thousand packs is what a stall is made of.
    #[test]
    fn catalogue_lookups_do_not_scan() {
        let mut state = init();
        ready(&mut state, details(&[]), vec!["tech one"]);

        assert_eq!(state.index.by_id.len(), 5);
        assert_eq!(state.index.by_lower_name.len(), 5);
        assert_eq!(state.index.revision, state.snapshot.revision);

        let entry = state.installed.first().expect("one library pack");
        assert_eq!(
            installed_catalog_entry(&state, entry).map(|pack| pack.name.as_str()),
            Some("Tech One")
        );
        // a library pack the catalogue has never heard of simply misses
        let stranger = InstalledPack {
            name: "Homemade".to_owned(),
            lower: "homemade".to_owned(),
            songs: 3,
            sync: SyncPref::Default,
            banner: None,
        };
        assert!(installed_catalog_entry(&state, &stranger).is_none());
    }

    /// Search asks a narrower question than any tab, so it ignores the tab
    /// rather than intersecting with it.
    #[test]
    fn search_overrides_the_tab() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());
        select_tab(&mut state, Tab::Keyboard);

        state.query = "stamina".to_owned();
        rebuild_results(&mut state, None);
        assert_eq!(names(&state), vec!["Stamina One"]);
    }

    /// The whole point of the search service: a charter's name is not a pack
    /// name, so the answer comes from the site's credit index and arrives in
    /// the service's relevance order -- which must not be re-sorted by date.
    #[test]
    fn a_charter_search_keeps_the_services_own_order_and_its_reasons() {
        let mut state = init();
        // "Plain One" is the newest pack, so a date sort would put it first
        ready(
            &mut state,
            details(&[(4, "2026-01-01", &["dance"]), (2, "2020-01-01", &["dance"])]),
            Vec::new(),
        );

        state.query = "rosewood".to_owned();
        let hits = vec![
            SearchHit {
                pack_id: 2,
                score: 160,
                why: "all charts by rosewood".to_owned(),
            },
            SearchHit {
                pack_id: 4,
                score: 61,
                why: "1 chart by rosewood".to_owned(),
            },
        ];
        state.search = Arc::new(SearchSnapshot {
            phase: SearchPhase::Ready,
            query: "rosewood".to_owned(),
            hits: Arc::from(hits),
            capped: false,
            revision: 1,
            message: None,
        });
        rebuild_results(&mut state, None);

        assert_eq!(
            names(&state),
            vec!["Tech One", "Plain One"],
            "relevance, not recency"
        );
        assert_eq!(
            state.search_why.get(&2).map(String::as_str),
            Some("all charts by rosewood"),
            "the row has to say why it is an answer"
        );
        assert!(!search_running(&state));
    }

    /// A service that has not caught up with what was typed must not blank the
    /// list: the catalogue's own names are answerable without it.
    #[test]
    fn a_stale_search_snapshot_falls_back_to_pack_names() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());

        state.query = "stamina".to_owned();
        state.search = Arc::new(SearchSnapshot {
            phase: SearchPhase::Ready,
            query: "something else entirely".to_owned(),
            hits: Arc::from(Vec::new()),
            capped: false,
            revision: 1,
            message: None,
        });
        rebuild_results(&mut state, None);
        assert_eq!(names(&state), vec!["Stamina One"]);
        assert!(state.search_why.is_empty());
    }

    /// The browser has to work before -- or without -- the enrichment service.
    ///
    /// The paged views cannot: without dates there is no order and no page, so
    /// they wait rather than showing the catalogue in the CSV's own
    /// alphabetical order and reshuffling it later. The views the CSV answers
    /// on its own carry on regardless.
    #[test]
    fn the_csv_views_still_work_when_details_never_arrive() {
        let mut state = init();
        let snapshot = Snapshot {
            phase: CatalogPhase::Ready,
            catalog: Arc::from(catalog()),
            revision: 1,
            message: None,
            installs: Vec::new(),
        };
        sync_stepmaniaonline(
            &mut state,
            Services {
                catalog: Arc::new(snapshot),
                ..Services::default()
            },
            Vec::new(),
            Some(Vec::new()),
        );

        // The keyboard tab is a CSV property, so it answers with no network
        // enrichment at all.
        select_tab(&mut state, Tab::Keyboard);
        assert_eq!(names(&state), vec!["Keys One"]);

        // The paged views have no page yet, so they have nothing to show --
        // and say so rather than showing the catalogue in the wrong order.
        select_tab(&mut state, Tab::Pad);
        assert!(state.results.is_empty());
        select_tab(&mut state, Tab::Years);
        assert!(state.results.is_empty());
    }

    /// The grid only ever holds packs that actually have artwork -- a row of
    /// blank plates is worse than a shorter grid -- and never more than it has
    /// slots for.
    #[test]
    fn the_featured_grid_holds_only_packs_with_artwork() {
        let mut state = init();
        let entries: Vec<(u64, &str, &[&str])> = vec![
            (1, "2026-01-01", &["dance"]),
            (2, "2026-01-01", &["dance"]),
            (3, "2026-01-01", &["dance"]),
        ];
        let mut snap = details(&entries);
        // strip one banner to prove the filter bites
        {
            let mut by_id = (*snap.by_id).clone();
            if let Some(entry) = by_id.get_mut(&2) {
                entry.banner_url = None;
            }
            snap = Arc::new(DetailsSnapshot {
                phase: snap.phase,
                by_id: Arc::new(by_id),
                newest_first: Arc::clone(&snap.newest_first),
                doubles: Arc::clone(&snap.doubles),
                has_more: snap.has_more,
                revision: snap.revision,
                message: None,
            });
        }
        ready(&mut state, snap, Vec::new());

        assert_eq!(state.featured.len(), 2, "the banner-less pack is left out");
        assert!(state.featured.len() <= FEATURED_MAX);
        for index in &state.featured {
            let pack = &state.snapshot.catalog[*index];
            assert!(
                state.details.by_id[&pack.id].banner_url.is_some(),
                "every card has artwork"
            );
        }
    }

    /// The grid leads the landing tab only. On a filtered view the list is the
    /// answer, and unrelated packs above it would be noise.
    #[test]
    fn the_grid_shows_on_the_landing_tab_only() {
        let mut state = init();
        ready(
            &mut state,
            details(&[(1, "2026-01-01", &["dance"]), (4, "2025-01-01", &["dance"])]),
            Vec::new(),
        );

        select_tab(&mut state, Tab::Pad);
        assert!(grid_showing(&state));

        select_tab(&mut state, Tab::Keyboard);
        assert!(!grid_showing(&state), "only the landing tab");

        // and a search replaces the grid with its own answer
        select_tab(&mut state, Tab::Pad);
        state.query = "stam".to_owned();
        rebuild_results(&mut state, None);
        assert!(!grid_showing(&state));
        assert!(band_showing(&state));
    }

    /// Seven rows a page, and the page the cursor is on is arithmetic on the
    /// cursor -- so paging and stepping can never disagree about which rows
    /// are on screen.
    #[test]
    fn the_page_is_derived_from_the_cursor() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());
        select_tab(&mut state, Tab::Search);
        assert_eq!(visible_rows(), lo::ROWS);

        for cursor in 0..state.results.len() {
            state.cursor = cursor;
            let start = window_start(&state);
            assert!(
                state.cursor >= start && state.cursor < start + visible_rows(),
                "cursor {cursor} fell outside its own page"
            );
        }
    }

    /// Song art and pack banners share one fetcher, so their keys must not be
    /// able to collide -- a song jacket appearing as a pack banner would be a
    /// very confusing bug to find.
    #[test]
    fn song_art_keys_cannot_collide_with_pack_ids() {
        let key = song_art_key("https://example.test/a.png");
        assert!(key >= 1 << 63, "song keys live in the top half");
        assert_ne!(key, song_art_key("https://example.test/b.png"));
        assert_eq!(key, song_art_key("https://example.test/a.png"));
    }

    /// Scrolling must not read a pack page per row: only a cursor that has
    /// come to rest asks for one, and the detail page asks immediately.
    #[test]
    fn a_pack_page_is_only_read_once_the_cursor_rests() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());
        // a tab with rows in it: SEARCH has none until something is searched
        // for, and PAD has none until a page has been fetched
        select_tab(&mut state, Tab::Keyboard);
        assert!(wanted_pack_page(&state).is_none(), "not on arrival");

        // the first tick only notices which pack is in view; the wait starts
        // from there
        update(&mut state, 0.016);
        assert!(wanted_pack_page(&state).is_none());
        update(&mut state, SELECTION_DWELL + 0.01);
        let resting = wanted_pack_page(&state);
        assert!(resting.is_some(), "and yes once it has settled");

        // the detail page does not wait at all
        state.zone = Zone::Detail;
        assert!(wanted_pack_page(&state).is_some());
    }

    /// Opening the browser is a fresh question, so it always opens on the same
    /// answer: the landing tab, at the top, with no query.
    ///
    /// The tab used to survive leaving, which meant a session that ended on
    /// INSTALLED or half a search put the next reader somewhere they had not
    /// asked to be.
    #[test]
    fn entering_always_lands_on_the_pad_tab() {
        let mut state = init();
        ready(&mut state, details(&[]), Vec::new());

        for wherever in [Tab::Doubles, Tab::Years, Tab::Installed, Tab::Search] {
            select_tab(&mut state, wherever);
            state.query = "left over".to_owned();
            state.cursor = 3;
            state.year_slot = 4;

            on_enter(&mut state);

            assert_eq!(tab(&state), Tab::Pad, "after leaving {wherever:?}");
            assert_eq!(state.zone, Zone::Tabs, "the strip is the first question");
            assert_eq!(state.cursor, 0);
            assert_eq!(state.year_slot, 0);
            assert!(state.query.is_empty(), "and no leftover search");
        }
    }

    #[test]
    fn finished_installs_are_handed_over_once() {
        let mut state = init();
        let dir = PathBuf::from("songs/Some Pack");
        let snapshot = Arc::new(Snapshot {
            phase: CatalogPhase::Ready,
            catalog: Arc::from(catalog()),
            revision: 1,
            message: None,
            installs: Vec::new(),
        });
        sync_stepmaniaonline(
            &mut state,
            Services {
                catalog: Arc::clone(&snapshot),
                ..Services::default()
            },
            vec![dir.clone()],
            None,
        );
        sync_stepmaniaonline(
            &mut state,
            Services {
                catalog: snapshot,
                ..Services::default()
            },
            vec![dir.clone()],
            None,
        );

        // remembered once, and not rescanned behind the reader's back
        assert_eq!(state.installed_dirs, vec![dir]);
        assert!(take_pending_reload_dirs(&mut state).is_empty());
    }

    /// The rescan's progress feeds the dialog, and only the finish of the
    /// rescan this screen handed over counts as its own.
    #[test]
    fn the_reload_dialog_follows_its_own_rescan() {
        use crate::views::SimplyLoveContentReloadEvent as Event;
        let mut state = init();
        state.installed_dirs = vec![PathBuf::from("songs/Some Pack")];
        state.reload_prompt = Some(ReloadPrompt {
            reloading: true,
            ..ReloadPrompt::default()
        });
        state.pending_reload_dirs = state.installed_dirs.clone();

        // an earlier job's finish, before this one was handed over
        assert!(!sync_reload_events(
            &mut state,
            [Event::Finished {
                song_packs: Vec::new()
            }]
        ));
        assert!(state.reload_prompt.is_some());

        assert_eq!(take_pending_reload_dirs(&mut state).len(), 1);
        assert!(state.reload_prompt.as_ref().is_some_and(|p| p.started));
        assert!(!sync_reload_events(
            &mut state,
            [Event::Song {
                done: 3,
                total: 10,
                pack: "Some Pack".to_owned(),
                song: "A Song".to_owned(),
            }]
        ));
        assert_eq!(
            state
                .reload_prompt
                .as_ref()
                .and_then(|p| p.progress.clone()),
            Some((3, 10, "Some Pack".to_owned()))
        );
        assert!(sync_reload_events(
            &mut state,
            [Event::Finished {
                song_packs: Vec::new()
            }]
        ));
        assert!(state.reload_prompt.is_none());
        assert!(
            state.installed_dirs.is_empty(),
            "rescanned, so nothing owed"
        );
    }

    /// Single songs a finished rescan loaded are not asked about again: their
    /// install records outlive it, so they are remembered as loaded.
    #[test]
    fn a_rescan_settles_the_single_songs_it_loaded() {
        use crate::views::SimplyLoveContentReloadEvent as Event;
        use deadsync_online::smo_songs::{SongInstall, SongInstallPhase, SongInstallsSnapshot};
        let song = |title: &str, phase| SongInstall {
            pack_id: 7,
            title: title.to_owned(),
            artist: String::new(),
            group: deadsync_online::smo_songs::SINGLES_GROUP.to_owned(),
            phase,
            downloaded_bytes: 0,
            total_bytes: None,
            message: None,
        };
        let mut state = init();
        state.song_installs = Arc::new(SongInstallsSnapshot {
            installs: Arc::from(vec![
                song("Song A", SongInstallPhase::Installed),
                song("Song B", SongInstallPhase::Error),
            ]),
            revision: 1,
        });
        assert_eq!(super::super::preview::songs_added(&state), 1);
        state.reload_prompt = Some(ReloadPrompt {
            reloading: true,
            started: true,
            ..ReloadPrompt::default()
        });
        assert!(sync_reload_events(
            &mut state,
            [Event::Finished {
                song_packs: Vec::new()
            }]
        ));
        assert_eq!(
            state.reloaded_songs,
            vec![(7, "Song A".to_owned(), String::new())]
        );
        assert_eq!(
            super::super::preview::songs_added(&state),
            0,
            "nothing owed"
        );

        // A song added after that rescan is owed one of its own.
        state.song_installs = Arc::new(SongInstallsSnapshot {
            installs: Arc::from(vec![
                song("Song A", SongInstallPhase::Installed),
                song("Song B", SongInstallPhase::Installed),
            ]),
            revision: 2,
        });
        assert_eq!(super::super::preview::songs_added(&state), 1);
    }
}
