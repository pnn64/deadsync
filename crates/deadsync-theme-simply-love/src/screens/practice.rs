use crate::SimplyLoveEffect as ThemeEffect;
use crate::act;
use crate::color;
use crate::fonts::machine_font_key;
use crate::i18n;
use crate::i18n::{LookupKey, lookup_key};
use crate::screens::Screen;
use crate::screens::components::shared::density;
use crate::screens::gameplay as gameplay_screen;
use crate::views::PracticeRuntimeView;
use deadlib_platform::input::RawKeyboardEvent;
use deadlib_present::actors::{Actor, InlineText, SizeSpec};
use deadlib_present::space::{
    screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use deadlib_render_core::{BlendMode, MeshVertex};
use deadsync_assets::gameplay_stage::{
    ActiveGameplayStage, PracticeEditorPresentation, PracticeLivePresentation, PracticeStagePresenter,
};
use deadsync_gameplay::practice::editor::{
    PracticeEditorConfig, PracticeEditorFeedback, PracticeEditorFlash, PracticeEditorSound,
};
use deadsync_gameplay::practice::{
    PracticeDestination, PracticeHost, PracticeMainMenuItem, PracticeMenuKind, PracticePlaybackUpdate,
    PracticeSession, PracticeViewRequest,
};
use deadsync_gameplay::{
    AutosyncMode, GameplayAudioSnapshot, GameplayTimingTickMode, ScrollEffects,
    spacing_multiplier_for_percent,
};
use deadsync_input::InputEvent;
use deadsync_input::practice::PracticeKeyEvent;
use deadsync_notefield::{TimingLabelStyle, TimingSegmentLabel as PracticeTimingLabel};
use deadsync_online::gameplay_lobby::GameplayLobbySession;
use deadsync_profile as profile_data;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::{SpeedSegment, SpeedUnit, TimingSegments};
use deadsync_theme::FontRole;
use std::sync::Arc;

const MARKER_Z: f32 = 2985.0;
const EDIT_FIELD_CURSOR_TEX: &str = "practice/snap_display_icon_9x1 (doubleres).png";
const EDIT_FIELD_CURSOR_Z: f32 = MARKER_Z + 1.0;
const EDIT_MENU_ROW_HEIGHT: f32 = 32.0;
const EDIT_MENU_ROW_BG_HEIGHT: f32 = 30.0;
const EDIT_MENU_TITLE_X_OFFSET: f32 = 200.0;
const EDIT_MENU_TEXT_ZOOM: f32 = 1.0;
const EDIT_HELP_HEADER_ZOOM: f32 = 0.26;
const EDIT_HELP_MENU_Y: f32 = 150.0;
const EDIT_HELP_MISC_Y: f32 = 224.0;
const EDIT_FIELD_ZOOM_AT_480P: f32 = 0.5;
const EDIT_FIELD_HEIGHT_AT_480P: f32 = 360.0;
const EDIT_SNAP_CURSOR_ZOOM: f32 = 0.5;
const EDIT_INFO_VALUE_CHARS: usize = 28;
const EDIT_LINE_SOUND: &str = "assets/sounds/change.ogg";
const EDIT_MARKER_SOUND: &str = "assets/sounds/screen_edit_marker.ogg";
const EDIT_SNAP_SOUND: &str = "assets/sounds/screen_edit_snap.ogg";
const EDIT_INVALID_SOUND: &str = "assets/sounds/common_invalid.ogg";
const EDIT_SCROLL_SPEEDS: [f32; 7] = [1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0];
const FLASH_DURATION_SECS: f32 = 0.75;
const PRACTICE_DENSITY_Z: i16 = 2990;
const PRACTICE_DENSITY_LINE_Z: i16 = PRACTICE_DENSITY_Z + 1;
const PRACTICE_DENSITY_LINE_WIDTH: f32 = 2.0;

pub use deadsync_gameplay::practice::MusicStartSnap;

// PARITY[ITGmania NoteField]: Simply Love inherits these timing label
// colors, sides, and offsets from `_fallback/metrics.ini` `[NoteField]`.
const BPM_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [1.0, 0.0, 0.0, 1.0],
    left_side: true,
    offset_x: 60.0,
};
const STOP_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [0.8, 0.8, 0.0, 1.0],
    left_side: true,
    offset_x: 50.0,
};
const DELAY_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [0.0, 0.8, 0.8, 1.0],
    left_side: true,
    offset_x: 120.0,
};
const WARP_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [1.0, 0.0, 0.5, 1.0],
    left_side: false,
    offset_x: 90.0,
};
const TIME_SIG_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [1.0, 0.55, 0.0, 1.0],
    left_side: true,
    offset_x: 30.0,
};
const SPEED_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [0.5, 1.0, 1.0, 1.0],
    left_side: false,
    offset_x: 30.0,
};
const SCROLL_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [0.3, 0.8, 1.0, 1.0],
    left_side: true,
    offset_x: 100.0,
};
const FAKE_LABEL_STYLE: TimingLabelStyle = TimingLabelStyle {
    color: [1.0, 1.0, 0.5, 1.0],
    left_side: true,
    offset_x: 90.0,
};

#[derive(Clone, Copy, Debug)]
enum MarkerPlacement {
    P1,
    P2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PracticeDensityGeom {
    x: f32,
    y: f32,
    length: f32,
    thickness: f32,
}

/// Immutable Practice density graph owned by the game thread for the screen
/// lifetime. It is built once during Practice entry with an exact-size shared
/// vertex slice, then read without synchronization or mutation. Empty chart
/// density produces no mesh; there is no insertion, eviction, pruning, or
/// background work after warmup. The slice is freed on screen teardown, mesh
/// presence exposes the only useful hit/miss state, and each editing frame is
/// bounded to one shared-reference clone plus one position-line quad.
struct PracticeDensityGraph {
    mesh: Option<Arc<[MeshVertex]>>,
    geom: PracticeDensityGeom,
    duration: f32,
}

#[derive(Clone, Copy)]
struct EditInfoSource {
    cursor_beat: f32,
    current_second: f32,
    selection_anchor: Option<f32>,
    selection_end: Option<f32>,
    snap_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EditInfoKey {
    cursor_beat: u32,
    current_second: u32,
    selection_anchor: Option<u32>,
    selection_end: Option<u32>,
    snap_index: usize,
}

impl EditInfoSource {
    fn key(self) -> EditInfoKey {
        EditInfoKey {
            cursor_beat: self.cursor_beat.to_bits(),
            current_second: self.current_second.to_bits(),
            selection_anchor: self.selection_anchor.map(f32::to_bits),
            selection_end: self.selection_end.map(f32::to_bits),
            snap_index: self.snap_index,
        }
    }
}

struct PracticeEditLabels {
    title: Arc<str>,
    help_titles: [Arc<str>; 3],
    help_bodies: [Arc<str>; 3],
    info_suffix: Arc<str>,
}

/// Localized actor-ready Practice sidebar text owned by the game thread.
///
/// Static labels rebuild only on locale revision. The larger chart/status
/// payload rebuilds on locale, cursor, selection, or snap changes into one
/// retained scratch buffer, then publishes one shared status string. Capacity
/// is one fixed label set, one scratch buffer, and one shared string; there is
/// no eviction, pruning, synchronization, or background work. Stable actor
/// frames perform one revision load, one compact key comparison, and
/// shared-reference clones.
struct PracticeEditText {
    labels: Option<PracticeEditLabels>,
    info: Option<(EditInfoKey, Arc<str>)>,
    info_scratch: String,
    i18n_revision: u64,
}

impl PracticeEditText {
    const fn new() -> Self {
        Self {
            labels: None,
            info: None,
            info_scratch: String::new(),
            i18n_revision: u64::MAX,
        }
    }

    fn sync(&mut self, gameplay: &ActiveGameplayStage, source: EditInfoSource) {
        let revision = i18n::revision();
        let labels_dirty = self.labels.is_none() || self.i18n_revision != revision;
        if labels_dirty {
            self.labels = Some(PracticeEditLabels {
                title: i18n::tr("Practice", "TitlePracticeMode"),
                help_titles: [
                    i18n::tr("Practice", "HelpSidebarNavigatingTitle"),
                    i18n::tr("Practice", "HelpSidebarMenusTitle"),
                    i18n::tr("Practice", "HelpSidebarMiscTitle"),
                ],
                help_bodies: [
                    i18n::tr("Practice", "HelpSidebarNavigatingBody"),
                    i18n::tr("Practice", "HelpSidebarMenusBody"),
                    i18n::tr("Practice", "HelpSidebarMiscBody"),
                ],
                info_suffix: Arc::from(build_edit_info_suffix(gameplay)),
            });
            self.info = None;
            self.i18n_revision = revision;
        }

        let key = source.key();
        let info_dirty = self.info.as_ref().is_none_or(|(cached, _)| *cached != key);
        if info_dirty {
            let suffix = &self
                .labels
                .as_ref()
                .expect("Practice edit labels sync before info")
                .info_suffix;
            self.info_scratch.clear();
            build_edit_info_text_into(&mut self.info_scratch, source, suffix);
            self.info = Some((key, Arc::from(self.info_scratch.as_str())));
        }
    }
}

/// Localized rows retained for the active Practice menu on the app thread.
///
/// Capacity is one exact-size boxed slice for the screen lifetime. It warms on
/// the first visible menu frame and replaces only when the menu definition or
/// locale revision changes. There is no growth, eviction, pruning, locking, or
/// worker work on a stable frame; the revision and menu kind are the
/// invalidation instrumentation. Replaced rows drop during menu rendering and
/// remaining rows drop at screen teardown. A miss performs at most the 14
/// bounded Help-menu lookups and one boxed-slice allocation.
struct PracticeMenuText {
    kind: Option<PracticeMenuKind>,
    labels: Option<Box<[Arc<str>]>>,
    i18n_revision: u64,
}

impl PracticeMenuText {
    const fn new() -> Self {
        Self {
            kind: None,
            labels: None,
            i18n_revision: u64::MAX,
        }
    }

    fn sync(&mut self, kind: PracticeMenuKind) {
        let revision = i18n::revision();
        let current = self.labels.is_some()
            && self.i18n_revision == revision
            && self.kind == Some(kind);
        if current {
            return;
        }

        self.labels = Some(match kind {
            PracticeMenuKind::Main => PracticeMainMenuItem::ALL
                .into_iter()
                .map(|item| main_menu_label(item).get())
                .collect(),
            PracticeMenuKind::Help => HELP_MENU_LABELS.iter().map(|key| key.get()).collect(),
        });
        self.kind = Some(kind);
        self.i18n_revision = revision;
    }
}

pub type State = PracticeHost<gameplay_screen::State, PracticePresentation>;

use deadsync_assets::gameplay_presentation::PracticeGameplayHud;

impl PracticeGameplayHud for gameplay_screen::GameplayPresentation {
    fn set_practice_timing_labels(&mut self, labels: [Box<[PracticeTimingLabel]>; 2]) {
        self.set_practice_timing_labels(labels);
    }

    fn music_rate_changed(&mut self, stage: &ActiveGameplayStage) {
        gameplay_screen::sync_music_rate_text(stage, self);
    }

    fn machine_font(&self) -> deadsync_config::theme::MachineFont { self.machine_font() }
}

pub struct PracticePresentation {
    hud: profile_data::GameplayHudSnapshot,
    density_graph: PracticeDensityGraph,
    edit_text: PracticeEditText,
    menu_text: PracticeMenuText,
    edit_scroll_speed_index: usize,
    /// Localized presentation feedback, retained until its visibility expires.
    flash: Option<(Arc<str>, f32)>,
    pending_sfx: Vec<&'static str>,
}

fn main_menu_label(item: PracticeMainMenuItem) -> LookupKey {
    let key = match item {
        PracticeMainMenuItem::PlayWholeSong => "MenuPlayWholeSong",
        PracticeMainMenuItem::PlayCurrentToEnd => "MenuPlayCurrentToEnd",
        PracticeMainMenuItem::PlaySelection => "MenuPlaySelection",
        PracticeMainMenuItem::SetSelectionStart => "MenuSetSelectionStart",
        PracticeMainMenuItem::SetSelectionEnd => "MenuSetSelectionEnd",
        PracticeMainMenuItem::PlayerOptions => "MenuPracticeModeOptions",
        PracticeMainMenuItem::Exit => "MenuExit",
    };
    lookup_key("Practice", key)
}

const HELP_MENU_LABELS: [LookupKey; PracticeMenuKind::Help.row_count()] = [
    lookup_key("Practice", "HelpHoldUpDown"),
    lookup_key("Practice", "HelpSemicolonApostrophe"),
    lookup_key("Practice", "HelpCtrlUpDown"),
    lookup_key("Practice", "HelpHomeEnd"),
    lookup_key("Practice", "HelpLeftRight"),
    lookup_key("Practice", "HelpBracketKeys"),
    lookup_key("Practice", "HelpSpace"),
    lookup_key("Practice", "HelpShiftNavigate"),
    lookup_key("Practice", "HelpP"),
    lookup_key("Practice", "HelpF7Ticks"),
    lookup_key("Practice", "HelpF8Autoplay"),
    lookup_key("Practice", "HelpF6Autosync"),
    lookup_key("Practice", "HelpOffsetKeys"),
    lookup_key("Practice", "HelpEscEnter"),
];

const SNAP_LABELS: [&str; 9] = [
    "4th", "8th", "12th", "16th", "24th", "32nd", "48th", "64th", "192nd",
];
pub fn init(mut gameplay: gameplay_screen::State, runtime: PracticeRuntimeView) -> State {
    let session = PracticeSession::new(
        &mut gameplay.gameplay,
        PracticeEditorConfig {
            only_dedicated_menu_buttons: runtime.only_dedicated_menu_buttons,
            three_key_navigation: runtime.three_key_navigation,
            tab_acceleration: runtime.tab_acceleration,
        },
    );
    init_with_session(gameplay, runtime.hud, session)
}

pub fn init_with_session(
    mut gameplay: gameplay_screen::State,
    hud: profile_data::GameplayHudSnapshot,
    mut session: PracticeSession,
) -> State {
    let presentation = PracticePresentation::new(&gameplay.stage, &mut gameplay.presentation, hud);
    session.snap_display_to_cursor(&mut gameplay.stage.gameplay);
    State {
        gameplay,
        session,
        presentation,
    }
}

impl PracticePresentation {
    pub fn new(
        stage: &ActiveGameplayStage,
        gameplay_hud: &mut gameplay_screen::GameplayPresentation,
        hud: profile_data::GameplayHudSnapshot,
    ) -> Self {
        Self::for_gameplay(stage, gameplay_hud, hud)
    }

    pub fn new_mounted(
        stage: &ActiveGameplayStage,
        gameplay_hud: &mut impl PracticeGameplayHud,
        hud: profile_data::GameplayHudSnapshot,
    ) -> Self {
        Self::for_gameplay(stage, gameplay_hud, hud)
    }

    fn for_gameplay(
        stage: &ActiveGameplayStage,
        gameplay_hud: &mut impl PracticeGameplayHud,
        hud: profile_data::GameplayHudSnapshot,
    ) -> Self {
        let active_players = stage.num_players();
        let density_graph = build_practice_density_graph(stage);
        let timing_labels = std::array::from_fn(|player| {
            if player < active_players {
                stage.gameplay_chart(player).map_or_else(
                    || Vec::new().into_boxed_slice(),
                    |chart| compile_timing_labels(&chart.timing_segments),
                )
            } else {
                Vec::new().into_boxed_slice()
            }
        });
        gameplay_hud.set_practice_timing_labels(timing_labels);
        Self {
            hud,
            density_graph,
            edit_text: PracticeEditText::new(),
            menu_text: PracticeMenuText::new(),
            edit_scroll_speed_index: 0,
            flash: None,
            pending_sfx: Vec::with_capacity(8),
        }
    }

    pub const fn edit_scroll_speed_index(&self) -> usize {
        self.edit_scroll_speed_index
    }

    pub fn restore_edit_scroll_speed(&mut self, index: usize) {
        self.edit_scroll_speed_index = index.min(EDIT_SCROLL_SPEEDS.len() - 1);
    }

    pub fn editor_feedback(&mut self) -> EditorPresentation<'_> {
        EditorPresentation {
            flash: &mut self.flash,
            pending_sfx: &mut self.pending_sfx,
            music_rate_text_dirty: false,
        }
    }
}

impl<P: PracticeGameplayHud> PracticeLivePresentation<P> for PracticePresentation {
    type Feedback<'a> = EditorPresentation<'a>;
    type Effect = ThemeEffect;
    type EditSnapshot = usize;

    fn editor_feedback(&mut self) -> Self::Feedback<'_> {
        self.editor_feedback()
    }

    fn edit_snapshot(&self) -> Self::EditSnapshot {
        self.edit_scroll_speed_index()
    }

    fn restore_edit_snapshot(&mut self, snapshot: Self::EditSnapshot) {
        self.restore_edit_scroll_speed(snapshot);
    }

    fn advance_visuals(&mut self, delta_time: f32) {
        advance_flash(self, delta_time);
    }

    fn apply_playback_update(&mut self, update: PracticePlaybackUpdate) {
        apply_playback_update(self, update);
    }

    fn apply_view_request(&mut self, request: PracticeViewRequest) {
        apply_view_request(self, request);
    }

    fn append_effects(&mut self, effects: &mut Vec<Self::Effect>) {
        append_presentation_effects(self, ThemeEffect::None, effects);
    }
}

fn append_pending_effects(
    pending_rates: impl ExactSizeIterator<Item = f32>,
    pending_sfx: &mut Vec<&'static str>,
    effect: ThemeEffect,
    effects: &mut Vec<ThemeEffect>,
) {
    effects.reserve(
        pending_rates.len() + pending_sfx.len() + usize::from(!matches!(effect, ThemeEffect::None)),
    );
    effects.extend(pending_rates.map(|rate| {
        ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
            crate::SimplyLoveProfileRequest::SetMusicRate(rate),
        ))
    }));
    effects.extend(pending_sfx.drain(..).map(crate::effects::sfx));
    if !matches!(&effect, ThemeEffect::None) {
        debug_assert!(!matches!(&effect, ThemeEffect::Batch(_)));
        effects.push(effect);
    }
}

pub fn append_presentation_effects(
    presentation: &mut PracticePresentation,
    effect: ThemeEffect,
    effects: &mut Vec<ThemeEffect>,
) {
    append_pending_effects(std::iter::empty(), &mut presentation.pending_sfx, effect, effects);
}

/// Borrows only presentation storage while a game-owned editor operation runs.
pub struct EditorPresentation<'a> {
    flash: &'a mut Option<(Arc<str>, f32)>,
    pending_sfx: &'a mut Vec<&'static str>,
    music_rate_text_dirty: bool,
}

impl<P: PracticeGameplayHud> PracticeEditorPresentation<P> for EditorPresentation<'_> {
    fn emit(&mut self, game: &gameplay_screen::GameplayCoreState, feedback: PracticeEditorFeedback) {
        self.emit(game, feedback);
    }

    fn finish(self, stage: &ActiveGameplayStage, gameplay_hud: &mut P) {
        self.finish_for(stage, gameplay_hud);
    }
}

impl EditorPresentation<'_> {
    pub fn finish(
        self,
        stage: &ActiveGameplayStage,
        gameplay_hud: &mut gameplay_screen::GameplayPresentation,
    ) {
        self.finish_for(stage, gameplay_hud);
    }

    fn finish_for(self, stage: &ActiveGameplayStage, gameplay_hud: &mut impl PracticeGameplayHud) {
        if self.music_rate_text_dirty {
            gameplay_hud.music_rate_changed(stage);
        }
    }

    pub fn emit(
        &mut self,
        game: &gameplay_screen::GameplayCoreState,
        feedback: PracticeEditorFeedback,
    ) {
        match feedback {
            PracticeEditorFeedback::Sound(sound) => self.pending_sfx.push(match sound {
                PracticeEditorSound::Line => EDIT_LINE_SOUND,
                PracticeEditorSound::Marker => EDIT_MARKER_SOUND,
                PracticeEditorSound::Snap => EDIT_SNAP_SOUND,
                PracticeEditorSound::Invalid => EDIT_INVALID_SOUND,
                PracticeEditorSound::MenuOpenClose => "assets/sounds/start.ogg",
                PracticeEditorSound::MenuMove => "assets/sounds/change.ogg",
            }),
            PracticeEditorFeedback::PersistMusicRate(_) => unreachable!("session rate command"),
            PracticeEditorFeedback::SyncMusicRateText => self.music_rate_text_dirty = true,
            PracticeEditorFeedback::Flash(flash) => {
                let text = match flash {
                    PracticeEditorFlash::AreaMarkerStartSet => {
                        i18n::tr("Practice", "FlashAreaMarkerStartSet")
                    }
                    PracticeEditorFlash::AreaMarkerEndSet => {
                        i18n::tr("Practice", "FlashAreaMarkerEndSet")
                    }
                    PracticeEditorFlash::SelectionCleared => {
                        i18n::tr("Practice", "FlashSelectionCleared")
                    }
                    PracticeEditorFlash::InvalidSelectionStart => {
                        i18n::tr("Practice", "FlashInvalidSelectionStart")
                    }
                    PracticeEditorFlash::SelectionStartSet => {
                        i18n::tr("Practice", "FlashSelectionStartSet")
                    }
                    PracticeEditorFlash::InvalidSelectionEnd => {
                        i18n::tr("Practice", "FlashInvalidSelectionEnd")
                    }
                    PracticeEditorFlash::SelectionEndSet => {
                        i18n::tr("Practice", "FlashSelectionEndSet")
                    }
                    PracticeEditorFlash::MusicRate { rate } => {
                        music_rate_flash_text(game, "FlashMusicRate", rate)
                    }
                    PracticeEditorFlash::MusicRateLimit { rate } => {
                        music_rate_flash_text(game, "FlashMusicRateLimit", rate)
                    }
                    PracticeEditorFlash::Autoplay { enabled } => i18n::tr(
                        "Practice",
                        if enabled {
                            "FlashAutoplayOn"
                        } else {
                            "FlashAutoplayOff"
                        },
                    ),
                    PracticeEditorFlash::TimingTick(mode) => i18n::tr(
                        "Practice",
                        match mode {
                            GameplayTimingTickMode::Off => "FlashTicksOff",
                            GameplayTimingTickMode::Assist => "FlashTicksAssist",
                            GameplayTimingTickMode::Hit => "FlashTicksHit",
                            GameplayTimingTickMode::Beat => "FlashTicksBeat",
                        },
                    ),
                    PracticeEditorFlash::Autosync(mode) => i18n::tr(
                        "Practice",
                        match mode {
                            AutosyncMode::Off => "FlashAutosyncOff",
                            AutosyncMode::Song => "FlashAutosyncSong",
                            AutosyncMode::Machine => "FlashAutosyncMachine",
                        },
                    ),
                    PracticeEditorFlash::Offset { global, seconds } => {
                        let key = if global {
                            "FlashGlobalOffset"
                        } else {
                            "FlashSongOffset"
                        };
                        let ms = format!("{:+.0}", seconds * 1000.0);
                        normalize_flash_text(i18n::tr_fmt("Practice", key, &[("ms", &ms)]))
                    }
                };
                *self.flash = Some((text, FLASH_DURATION_SECS));
            }
        }
    }
}

fn with_session<R>(
    state: &mut State,
    operation: impl FnOnce(
        &mut PracticeSession,
        &mut gameplay_screen::GameplayCoreState,
        &mut EditorPresentation<'_>,
    ) -> R,
) -> R {
    let mut feedback = state.presentation.editor_feedback();
    let result = operation(&mut state.session, &mut state.gameplay.stage.gameplay, &mut feedback);
    feedback.finish(&state.gameplay.stage, &mut state.gameplay.presentation);
    result
}

pub fn apply_view_request(presentation: &mut PracticePresentation, request: PracticeViewRequest) {
    match request {
        PracticeViewRequest::ChangeEditScrollSpeed(delta) => change_edit_scroll_speed(presentation, delta),
        PracticeViewRequest::PlaybackStarted => presentation.flash = None,
        _ => {}
    }
}

fn apply_legacy_view_request(state: &mut State, request: PracticeViewRequest) -> ThemeEffect {
    apply_view_request(&mut state.presentation, request);
    match request {
        PracticeViewRequest::Navigate(PracticeDestination::PlayerOptions) => ThemeEffect::Navigate(Screen::PlayerOptions),
        PracticeViewRequest::Navigate(PracticeDestination::SelectMusic) => ThemeEffect::Navigate(Screen::SelectMusic),
        _ => ThemeEffect::None,
    }
}

pub fn on_enter(state: &mut State) {
    state.session.on_enter(&mut state.gameplay);
}

pub fn update(
    state: &mut State,
    delta_time: f32,
    audio_snapshot: GameplayAudioSnapshot,
    fallback_host_nanos: impl FnOnce() -> u64,
    snap_music_start: MusicStartSnap,
    effects: &mut Vec<ThemeEffect>,
) {
    advance(
        state,
        delta_time,
        audio_snapshot,
        fallback_host_nanos,
        snap_music_start,
    );
    append_pending_effects(
        state.session.drain_music_rates(),
        &mut state.presentation.pending_sfx,
        ThemeEffect::None,
        effects,
    );
}

pub fn advance(
    state: &mut State,
    delta_time: f32,
    audio_snapshot: GameplayAudioSnapshot,
    fallback_host_nanos: impl FnOnce() -> u64,
    snap_music_start: MusicStartSnap,
) {
    advance_flash(&mut state.presentation, delta_time);
    let playback = with_session(state, |session, game, presentation| {
        session.update(
            game,
            delta_time,
            audio_snapshot,
            fallback_host_nanos,
            snap_music_start,
            |game, feedback| presentation.emit(game, feedback),
        )
    });
    apply_playback_update(&mut state.presentation, playback);
}

#[inline]
pub fn advance_flash(presentation: &mut PracticePresentation, delta_time: f32) {
    if let Some((_, remaining)) = presentation.flash.as_mut() {
        *remaining -= delta_time;
        if *remaining <= 0.0 {
            presentation.flash = None;
        }
    }
}

#[inline]
pub fn apply_playback_update(presentation: &mut PracticePresentation, update: PracticePlaybackUpdate) {
    if update == PracticePlaybackUpdate::Restarted {
        presentation.flash = None;
    }
}

pub fn handle_input(
    state: &mut State,
    ev: &InputEvent,
    snap_music_start: MusicStartSnap,
    effects: &mut Vec<ThemeEffect>,
) {
    let effect = process_input(state, ev, snap_music_start);
    append_pending_effects(
        state.session.drain_music_rates(),
        &mut state.presentation.pending_sfx,
        effect,
        effects,
    );
}

pub fn process_input(
    state: &mut State,
    ev: &InputEvent,
    snap_music_start: MusicStartSnap,
) -> ThemeEffect {
    let request = with_session(state, |session, game, presentation| {
        session.handle_input(game, ev, snap_music_start, |game, feedback| {
            presentation.emit(game, feedback);
        })
    });
    apply_legacy_view_request(state, request)
}

pub fn handle_raw_key_event(
    state: &mut State,
    raw_key: &RawKeyboardEvent,
    snap_music_start: MusicStartSnap,
    effects: &mut Vec<ThemeEffect>,
) -> bool {
    let start_len = effects.len();
    let (consumed, effect) = process_key(
        state,
        &PracticeKeyEvent::from_raw(raw_key),
        snap_music_start,
    );
    append_pending_effects(
        state.session.drain_music_rates(),
        &mut state.presentation.pending_sfx,
        effect,
        effects,
    );
    consumed || effects.len() != start_len
}

pub fn process_key(
    state: &mut State,
    event: &PracticeKeyEvent,
    snap_music_start: MusicStartSnap,
) -> (bool, ThemeEffect) {
    let (consumed, request) = with_session(state, |session, game, presentation| {
        session.handle_raw_key(
            game,
            event,
            snap_music_start,
            |game, feedback| {
                presentation.emit(game, feedback);
            },
        )
    });
    let effect = apply_legacy_view_request(state, request);
    (consumed, effect)
}

struct PracticeOverlay<'a> {
    gameplay: &'a ActiveGameplayStage,
    machine_font: deadsync_config::theme::MachineFont,
    session: &'a PracticeSession,
    presentation: &'a mut PracticePresentation,
}

pub fn push_overlay(
    actors: &mut Vec<Actor>,
    gameplay: &gameplay_screen::State,
    session: &PracticeSession,
    presentation: &mut PracticePresentation,
) {
    push_overlay_for(actors, &gameplay.stage, gameplay.presentation.machine_font(), session, presentation);
}

fn push_overlay_for(
    actors: &mut Vec<Actor>,
    gameplay: &ActiveGameplayStage,
    machine_font: deadsync_config::theme::MachineFont,
    session: &PracticeSession,
    presentation: &mut PracticePresentation,
) {
    let mut state = PracticeOverlay { gameplay, machine_font, session, presentation };
    if !session.is_playing() {
        append_edit_markers(&state, actors);
        append_density_graph(&state, actors);
        append_edit_overlay(&mut state, actors);
    }
    if session.menu().is_some() {
        append_main_menu(&mut state, actors);
    }
    // Music-rate changes and other feedback are also visible during playback.
    append_flash_overlay(&state, actors);
}

pub fn practice_view(
    session: &PracticeSession,
    presentation: &PracticePresentation,
) -> gameplay_screen::ActorViewOverride {
    let mut notefield = if session.is_playing() {
        gameplay_screen::NotefieldViewOverride::default()
    } else {
        practice_notefield_view(presentation)
    };
    notefield.hide_display_mods = true;
    gameplay_screen::ActorViewOverride {
        notefield,
        hide_gameplay_hud: true,
        ..Default::default()
    }
}

impl<P> PracticeStagePresenter<GameplayLobbySession, P> for PracticePresentation
where
    P: PracticeGameplayHud + deadsync_assets::gameplay_stage::GameplayStagePresenter<
        GameplayLobbySession, View = gameplay_screen::ActorViewOverride,
    >,
{
    fn frame_view(&self, session: &PracticeSession) -> gameplay_screen::ActorViewOverride {
        practice_view(session, self)
    }

    fn append_practice_overlay(
        &mut self,
        actors: &mut Vec<Actor>,
        gameplay: &deadsync_assets::gameplay_stage::GameplayHost<GameplayLobbySession, P>,
        session: &PracticeSession,
    ) {
        push_overlay_for(actors, &gameplay.stage, gameplay.presentation.machine_font(), session, self);
    }
}

fn practice_notefield_view(presentation: &PracticePresentation) -> gameplay_screen::NotefieldViewOverride {
    gameplay_screen::NotefieldViewOverride {
        field_zoom: Some(practice_edit_field_zoom()),
        scroll_speed: Some(practice_edit_scroll_speed(presentation)),
        // ITG ScreenEdit uses separate editor options, never the player's scroll mods.
        scroll: Some(ScrollEffects::default()),
        force_center_1player: true,
        receptor_y: Some(practice_edit_cursor_y()),
        edit_beat_bars: true,
        hide_combo: true,
        ..gameplay_screen::NotefieldViewOverride::default()
    }
}

fn practice_edit_field_zoom() -> f32 {
    screen_height() / 480.0 * EDIT_FIELD_ZOOM_AT_480P
}

fn practice_edit_cursor_y() -> f32 {
    (screen_height() / 480.0 * EDIT_FIELD_HEIGHT_AT_480P).mul_add(-0.5, screen_center_y())
}

const fn practice_edit_scroll_speed(presentation: &PracticePresentation) -> ScrollSpeedSetting {
    ScrollSpeedSetting::XMod(EDIT_SCROLL_SPEEDS[presentation.edit_scroll_speed_index])
}

fn practice_marker_bar_height() -> f32 {
    ScrollSpeedSetting::ARROW_SPACING * practice_edit_field_zoom()
}

fn change_edit_scroll_speed(presentation: &mut PracticePresentation, delta: isize) {
    let last = EDIT_SCROLL_SPEEDS.len() as isize - 1;
    let next = (presentation.edit_scroll_speed_index as isize + delta).clamp(0, last) as usize;
    if next == presentation.edit_scroll_speed_index {
        return;
    }
    presentation.edit_scroll_speed_index = next;
    presentation.flash = Some((i18n::tr("Practice", "FlashZoomChanged"), FLASH_DURATION_SECS));
    presentation.pending_sfx.push(EDIT_MARKER_SOUND);
}

fn music_rate_flash_text(
    game: &gameplay_screen::GameplayCoreState,
    key: &str,
    rate: f32,
) -> Arc<str> {
    let bpm_str = effective_bpm_str(game, rate);
    normalize_flash_text(i18n::tr_fmt(
        "Practice",
        key,
        &[("rate", &fmt_music_rate(rate)), ("bpm", &bpm_str)],
    ))
}

fn normalize_flash_text(text: Arc<str>) -> Arc<str> {
    if text.contains("\\n") {
        Arc::from(text.replace("\\n", "\n"))
    } else {
        text
    }
}

fn effective_bpm_str(game: &gameplay_screen::GameplayCoreState, rate: f32) -> String {
    let song = game.song();
    let chart = game.charts().first().map(std::convert::AsRef::as_ref);
    let is_random = chart
        .is_some_and(|c| matches!(c.display_bpm, Some(deadsync_chart::ChartDisplayBpm::Random)));
    if is_random {
        return "???".to_string();
    }
    let reference_bpm = song
        .chart_display_bpm_range(chart)
        .map(|(_, hi)| hi as f32)
        .unwrap_or(song.max_bpm as f32);
    let reference_bpm = if reference_bpm.is_finite() && reference_bpm > 0.0 {
        reference_bpm
    } else {
        120.0
    };
    let effective_bpm = f64::from(reference_bpm) * f64::from(rate);
    if (effective_bpm - effective_bpm.round()).abs() < 0.05 {
        format!("{}", effective_bpm.round() as i32)
    } else {
        format!("{effective_bpm:.1}")
    }
}

fn fmt_music_rate(rate: f32) -> String {
    let scaled = (rate * 100.0).round() as i32;
    let int_part = scaled / 100;
    let frac2 = (scaled % 100).abs();
    if frac2 == 0 {
        format!("{int_part}")
    } else if frac2 % 10 == 0 {
        format!("{}.{}", int_part, frac2 / 10)
    } else {
        format!("{int_part}.{frac2:02}")
    }
}

fn append_edit_markers(state: &PracticeOverlay<'_>, actors: &mut Vec<Actor>) {
    let hud = &state.presentation.hud;
    let play_style = hud.play_style;
    let is_p2_single = profile_data::is_single_p2_side(play_style, hud.player_side);

    match play_style {
        profile_data::PlayStyle::Versus
        | profile_data::PlayStyle::PumpVersus
        | profile_data::PlayStyle::Couple
        | profile_data::PlayStyle::PumpCouple
        | profile_data::PlayStyle::Routine
        | profile_data::PlayStyle::PumpRoutine => {
            append_player_markers(state, actors, 0, MarkerPlacement::P1, play_style, false);
            append_player_markers(state, actors, 1, MarkerPlacement::P2, play_style, false);
        }
        profile_data::PlayStyle::Single
        | profile_data::PlayStyle::Double
        | profile_data::PlayStyle::PumpSingle
        | profile_data::PlayStyle::PumpDouble => {
            let placement = if is_p2_single {
                MarkerPlacement::P2
            } else {
                MarkerPlacement::P1
            };
            append_player_markers(state, actors, 0, placement, play_style, true);
        }
    }
}

fn append_player_markers(
    state: &PracticeOverlay<'_>,
    actors: &mut Vec<Actor>,
    player_idx: usize,
    placement: MarkerPlacement,
    play_style: profile_data::PlayStyle,
    center_1player_notefield: bool,
) {
    if player_idx >= state.gameplay.num_players() {
        return;
    }

    let col_start = player_idx * state.gameplay.cols_per_player();
    let col_end = (col_start + state.gameplay.cols_per_player())
        .min(state.gameplay.num_cols())
        .min(state.gameplay.notefield_column_scroll_dir_count());
    let num_cols = col_end.saturating_sub(col_start);
    if num_cols == 0 {
        return;
    }

    let profile = &state.gameplay.profiles()[player_idx];
    let offset_sign = match placement {
        MarkerPlacement::P1 => -1.0,
        MarkerPlacement::P2 => 1.0,
    };
    let offset_x = offset_sign * profile.note_field_offset_x.clamp(0, 50) as f32;
    let offset_y = profile.note_field_offset_y.clamp(-50, 50) as f32;
    let clamped_width = screen_width().clamp(640.0, 854.0);
    let centered_one_side =
        state.gameplay.num_players() == 1 && play_style.is_single() && center_1player_notefield;
    let centered_both_sides = state.gameplay.num_players() == 1 && play_style.is_double();
    // Routine's two fields share the centered pads.
    let base_x = if play_style.is_routine() {
        screen_center_x()
    } else if state.gameplay.num_players() == 2 {
        match placement {
            MarkerPlacement::P1 => screen_center_x() - clamped_width * 0.25,
            MarkerPlacement::P2 => screen_center_x() + clamped_width * 0.25,
        }
    } else if centered_one_side || centered_both_sides {
        screen_center_x()
    } else {
        match placement {
            MarkerPlacement::P1 => screen_center_x() - clamped_width * 0.25,
            MarkerPlacement::P2 => screen_center_x() + clamped_width * 0.25,
        }
    };
    let center_x = base_x + offset_x;
    let spacing_mult = if player_idx < state.gameplay.num_players() {
        spacing_multiplier_for_percent(state.gameplay.profiles()[player_idx].spacing_percent)
    } else {
        1.0
    };
    let field_zoom = practice_edit_field_zoom();
    let width = (num_cols as f32 * ScrollSpeedSetting::ARROW_SPACING * spacing_mult * field_zoom)
        .max(ScrollSpeedSetting::ARROW_SPACING);
    let marker_phase = (state.gameplay.total_elapsed_in_screen() * std::f32::consts::PI).sin();
    let marker_shade = marker_phase.mul_add(0.25, 0.75);
    let editor = state.session.editor();
    let cursor_y = marker_y_for_beat(state, player_idx, offset_y, editor.cursor_beat());
    append_field_cursor(
        actors,
        center_x,
        cursor_y,
        width,
        field_zoom,
        editor.snap_index(),
    );

    match (editor.selection_anchor(), editor.selection_end()) {
        (Some(begin), Some(end)) if end > begin => {
            let y1 = marker_y_for_beat(state, player_idx, offset_y, begin);
            let y2 = marker_y_for_beat(state, player_idx, offset_y, end);
            append_marker_area(actors, center_x, y1, y2, width);
        }
        (Some(begin), _) => {
            let y = marker_y_for_beat(state, player_idx, offset_y, begin);
            append_marker_bar(actors, center_x, y, width, marker_shade);
        }
        (None, Some(end)) => {
            let y = marker_y_for_beat(state, player_idx, offset_y, end);
            append_marker_bar(actors, center_x, y, width, marker_shade);
        }
        (None, None) => {}
    }
}

fn compile_timing_labels(timing: &TimingSegments) -> Box<[PracticeTimingLabel]> {
    let capacity = timing.scrolls.len()
        + timing.bpms.len()
        + timing.stops.len()
        + timing.delays.len()
        + timing.warps.len()
        + timing.time_signatures.len()
        + timing.speeds.len()
        + timing.fakes.len();
    let mut labels = Vec::with_capacity(capacity);
    let mut push = |text: String, beat, style| {
        labels.push(PracticeTimingLabel {
            text: Arc::from(text),
            beat,
            style,
        });
    };

    // PARITY[ITGmania NoteField::DrawPrimitives]: segment text draw order.
    for seg in &timing.scrolls {
        push(fmt_itg_float(seg.ratio), seg.beat, SCROLL_LABEL_STYLE);
    }
    for &(beat, bpm) in &timing.bpms {
        push(fmt_itg_float(bpm), beat, BPM_LABEL_STYLE);
    }
    for seg in &timing.stops {
        push(fmt_itg_float(seg.duration), seg.beat, STOP_LABEL_STYLE);
    }
    for seg in &timing.delays {
        push(fmt_itg_float(seg.duration), seg.beat, DELAY_LABEL_STYLE);
    }
    for seg in &timing.warps {
        push(fmt_itg_float(seg.length), seg.beat, WARP_LABEL_STYLE);
    }
    for seg in &timing.time_signatures {
        push(
            format!("{}\n--\n{}", seg.numerator, seg.denominator),
            seg.beat,
            TIME_SIG_LABEL_STYLE,
        );
    }
    for seg in &timing.speeds {
        push(timing_speed_label(*seg), seg.beat, SPEED_LABEL_STYLE);
    }
    for seg in &timing.fakes {
        push(fmt_itg_float(seg.length), seg.beat, FAKE_LABEL_STYLE);
    }
    labels.into_boxed_slice()
}

fn timing_speed_label(seg: SpeedSegment) -> String {
    let unit = match seg.unit {
        SpeedUnit::Seconds => "S",
        SpeedUnit::Beats => "B",
    };
    format!(
        "{}\n{}\n{}",
        fmt_itg_float(seg.ratio),
        unit,
        fmt_itg_float(seg.delay)
    )
}

fn fmt_itg_float(value: f32) -> String {
    format!("{value:.6}")
}

fn marker_y_for_beat(state: &PracticeOverlay<'_>, player_idx: usize, offset_y: f32, beat: f32) -> f32 {
    let receptor_y = practice_edit_cursor_y() + offset_y;
    let field_zoom = practice_edit_field_zoom();
    let scroll_speed = practice_edit_scroll_speed(state.presentation);
    let current_beat = state.gameplay.visible_beat(player_idx);
    let travel = practice_edit_beat_travel(
        beat,
        current_beat,
        field_zoom,
        scroll_speed,
        state.gameplay.scroll_reference_bpm(),
        state.gameplay.music_rate(),
    );
    receptor_y + travel
}

fn practice_edit_beat_travel(
    beat: f32,
    current_beat: f32,
    field_zoom: f32,
    scroll_speed: ScrollSpeedSetting,
    reference_bpm: f32,
    music_rate: f32,
) -> f32 {
    let player_multiplier = scroll_speed.beat_multiplier(reference_bpm, music_rate);
    (beat - current_beat) * ScrollSpeedSetting::ARROW_SPACING * field_zoom * player_multiplier
}

fn append_marker_area(actors: &mut Vec<Actor>, center_x: f32, y1: f32, y2: f32, width: f32) {
    if !y1.is_finite() || !y2.is_finite() {
        return;
    }
    let marker_height = practice_marker_bar_height();
    let top = y1.min(y2).max(-marker_height);
    let bottom = y1.max(y2).min(screen_height() + marker_height);
    if bottom - top <= 1.0 {
        return;
    }
    actors.push(act!(quad:
        align(0.5, 0.0):
        xy(center_x, top):
        zoomto(width, bottom - top):
        diffuse(1.0, 0.0, 0.0, 0.3):
        z(MARKER_Z - 1.0)
    ));
}

fn append_marker_bar(actors: &mut Vec<Actor>, center_x: f32, y: f32, width: f32, shade: f32) {
    let marker_height = practice_marker_bar_height();
    if !y.is_finite() || y < -marker_height || y > screen_height() + marker_height {
        return;
    }
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(center_x, y):
        zoomto(width, marker_height):
        diffuse(shade, shade, shade, 0.5):
        z(MARKER_Z)
    ));
}

fn append_field_cursor(
    actors: &mut Vec<Actor>,
    center_x: f32,
    y: f32,
    width: f32,
    zoom: f32,
    snap_index: usize,
) {
    let marker_height = practice_marker_bar_height();
    if !y.is_finite() || y < -marker_height || y > screen_height() + marker_height {
        return;
    }
    let side_gap = ScrollSpeedSetting::ARROW_SPACING * 0.5 * zoom;
    let frame = snap_index.min(SNAP_LABELS.len().saturating_sub(1)) as u32;
    append_snap_cursor_heart(
        actors,
        width.mul_add(-0.5, center_x) - side_gap,
        y,
        zoom,
        frame,
    );
    append_snap_cursor_heart(
        actors,
        width.mul_add(0.5, center_x) + side_gap,
        y,
        zoom,
        frame,
    );
}

fn append_snap_cursor_heart(actors: &mut Vec<Actor>, x: f32, y: f32, zoom: f32, frame: u32) {
    actors.push(act!(sprite_static(EDIT_FIELD_CURSOR_TEX):
        align(0.5, 0.5):
        xy(x, y):
        zoom(zoom * EDIT_SNAP_CURSOR_ZOOM):
        setstate(frame):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(EDIT_FIELD_CURSOR_Z)
    ));
}

fn build_practice_density_graph(gameplay: &ActiveGameplayStage) -> PracticeDensityGraph {
    let chart = gameplay.chart(0);
    let chart_type = chart.map_or("", |chart| chart.chart_type.as_str());
    let notefield_width = practice_style_notefield_width(chart_type, gameplay.cols_per_player());
    let geom = practice_density_geom(
        screen_width(),
        screen_height(),
        notefield_width,
        gameplay.num_players() == 1 && gameplay.cols_per_player() >= 8,
    );
    let first_second = gameplay.music_time_for_beat(0.0).min(0.0);
    let last_second = gameplay.song().precise_last_second();
    let duration = (last_second - first_second).max(0.001);
    let mesh = chart.and_then(|chart| {
        let mut vertices = density::build_density_histogram_mesh(
            &chart.measure_nps_vec,
            chart.max_nps,
            &chart.measure_seconds_vec,
            first_second,
            last_second,
            geom.length,
            geom.thickness,
            0.0,
            geom.length,
            None,
            1.0,
        );
        rotate_practice_density_mesh(&mut vertices, geom.thickness);
        (!vertices.is_empty()).then(|| Arc::from(vertices))
    });
    PracticeDensityGraph {
        mesh,
        geom,
        duration,
    }
}

fn practice_style_notefield_width(chart_type: &str, cols_per_player: usize) -> f32 {
    let column_width = if chart_type.starts_with("pump-") {
        50.0
    } else {
        64.0
    };
    cols_per_player as f32 * column_width
}

fn practice_density_geom(
    screen_w: f32,
    screen_h: f32,
    notefield_w: f32,
    double: bool,
) -> PracticeDensityGeom {
    // PARITY[Simply Love ScreenPractice overlay.lua]: the horizontal graph is
    // rotated into this strip after applying its 4:3 and double-play offsets.
    let is_4_3 = screen_w / screen_h.max(1.0) <= 1.4;
    let mut length = (screen_h - 100.0).max(1.0);
    let mut thickness = 50.0;
    let mut y = 80.0;
    let mut x_offset = notefield_w / if double { 2.2 } else { 1.3 };
    if is_4_3 {
        thickness = 25.0;
        x_offset = notefield_w / 1.5 - 1.0;
        if double {
            length = (screen_h - 200.0).max(1.0);
            x_offset = notefield_w / 2.7;
            y = 180.0;
        }
    }
    PracticeDensityGeom {
        x: screen_w.mul_add(0.5, x_offset - thickness),
        y,
        length,
        thickness,
    }
}

fn rotate_practice_density_mesh(vertices: &mut [MeshVertex], thickness: f32) {
    for vertex in vertices {
        let [along, height] = vertex.pos;
        vertex.pos = [thickness - height, along];
    }
}

fn append_density_graph(state: &PracticeOverlay<'_>, actors: &mut Vec<Actor>) {
    let graph = &state.presentation.density_graph;
    if let Some(mesh) = &graph.mesh {
        actors.push(Actor::Mesh {
            align: [0.0, 0.0],
            offset: [graph.geom.x, graph.geom.y],
            size: [
                SizeSpec::Px(graph.geom.thickness),
                SizeSpec::Px(graph.geom.length),
            ],
            tint: [1.0; 4],
            vertices: Arc::clone(mesh),
            vertex_color_encoding: Default::default(),
            visible: true,
            blend: BlendMode::Alpha,
            z: PRACTICE_DENSITY_Z,
        });
    }

    // SL's position line scales the current second across LastSecond-FirstSecond.
    let progress = (state.gameplay.current_music_time_display() / graph.duration).clamp(0.0, 1.0);
    actors.push(act!(quad:
        align(0.0, 0.5):
        xy(graph.geom.x, graph.geom.length.mul_add(progress, graph.geom.y)):
        zoomto(graph.geom.thickness, PRACTICE_DENSITY_LINE_WIDTH):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(PRACTICE_DENSITY_LINE_Z)
    ));
}

fn append_edit_overlay(state: &mut PracticeOverlay<'_>, actors: &mut Vec<Actor>) {
    let editor = state.session.editor();
    let source = EditInfoSource {
        cursor_beat: editor.cursor_beat(),
        current_second: state.gameplay.music_time_for_beat(editor.cursor_beat()),
        selection_anchor: editor.selection_anchor(),
        selection_end: editor.selection_end(),
        snap_index: editor.snap_index(),
    };
    state.presentation.edit_text.sync(state.gameplay, source);
    let labels = state.presentation.edit_text
        .labels
        .as_ref()
        .expect("Practice edit labels sync before actor construction");
    let status = &state.presentation.edit_text
        .info
        .as_ref()
        .expect("Practice edit info sync before actor construction")
        .1;
    let pc = practice_player_color(state);
    let header_font = machine_font_key(state.machine_font, FontRole::Header);
    actors.push(act!(text:
        font(header_font):
        settext(&labels.title):
        align(1.0, 0.5):
        xy(screen_width() - 35.0, 10.0):
        zoom(EDIT_HELP_HEADER_ZOOM):
        diffuse(pc[0], pc[1], pc[2], 1.0):
        z(3000)
    ));
    actors.push(act!(quad:
        align(1.0, 0.5):
        xy(screen_width(), 10.0):
        zoomto(30.0, 1.0):
        diffuse(1.0, 1.0, 1.0, 0.75):
        z(2999)
    ));

    append_help_section(
        actors,
        &labels.help_titles[0],
        &labels.help_bodies[0],
        0.0,
        pc,
        header_font,
    );
    append_help_section(
        actors,
        &labels.help_titles[1],
        &labels.help_bodies[1],
        EDIT_HELP_MENU_Y,
        pc,
        header_font,
    );
    append_help_section(
        actors,
        &labels.help_titles[2],
        &labels.help_bodies[2],
        EDIT_HELP_MISC_Y,
        pc,
        header_font,
    );
    actors.push(act!(text:
        font("miso"):
        settext(status):
        align(0.0, 0.0):
        xy(screen_width() - 150.0, 20.0):
        zoom(0.625):
        maxwidth(145.0):
        shadowlength(1.0):
        z(3000)
    ));
}

fn append_flash_overlay(state: &PracticeOverlay<'_>, actors: &mut Vec<Actor>) {
    if let Some((text, remaining)) = state.presentation.flash.as_ref() {
        let alpha = remaining.clamp(0.0, 1.0);
        let label = text.clone();
        actors.push(act!(text:
            font("miso"):
            settext(label):
            align(0.5, 0.5):
            xy(screen_width() * 0.5, screen_height() - 80.0):
            zoom(0.9):
            diffuse(1.0, 1.0, 1.0, alpha):
            shadowlength(2.0):
            z(3001)
        ));
    }
}

fn practice_player_color(state: &PracticeOverlay<'_>) -> [f32; 4] {
    color::simply_love_rgba(state.gameplay.active_color_index())
}

enum PracticeNumber {
    Inline(InlineText),
    Heap(String),
}

impl PracticeNumber {
    fn format(args: std::fmt::Arguments<'_>) -> Self {
        InlineText::format(args)
            .map(Self::Inline)
            .unwrap_or_else(|| Self::Heap(args.to_string()))
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Inline(text) => text.as_str(),
            Self::Heap(text) => text,
        }
    }
}

fn build_edit_info_text_into(out: &mut String, source: EditInfoSource, suffix: &str) {
    let snap = SNAP_LABELS[source.snap_index];
    let cursor_beat = PracticeNumber::format(format_args!("{:.3}", source.cursor_beat));
    let current_second = PracticeNumber::format(format_args!("{:.6}", source.current_second));
    i18n::tr_fmt_into(
        out,
        "Practice",
        "InfoCurrentBeat",
        &[("beat", cursor_beat.as_str())],
    );
    out.push('\n');
    i18n::tr_fmt_into(
        out,
        "Practice",
        "InfoCurrentSecond",
        &[("sec", current_second.as_str())],
    );
    out.push('\n');
    i18n::tr_fmt_into(out, "Practice", "InfoSnapTo", &[("snap", snap)]);
    out.push('\n');
    if push_selection_info(out, source.selection_anchor, source.selection_end) {
        out.push('\n');
    }
    out.push_str(suffix);
}

fn build_edit_info_suffix(gameplay: &ActiveGameplayStage) -> String {
    let chart = &gameplay.charts()[0];
    let song = gameplay.song();
    let difficulty = color::difficulty_display_name_for_song(&chart.difficulty, &song.title, true);
    let mut status = String::new();
    i18n::tr_fmt_into(
        &mut status,
        "Practice",
        "InfoDifficulty",
        &[
            ("difficulty", difficulty),
            (
                "meter",
                PracticeNumber::format(format_args!("{}", chart.meter)).as_str(),
            ),
        ],
    );
    status.push_str("\n\n");
    push_info_line(
        &mut status,
        &i18n::tr("Practice", "InfoMainTitle"),
        &song.title,
    );
    push_info_line(
        &mut status,
        &i18n::tr("Practice", "InfoSubtitle"),
        &song.subtitle,
    );
    push_info_line(
        &mut status,
        &i18n::tr("Practice", "InfoDescription"),
        &chart.description,
    );
    push_info_line(
        &mut status,
        &i18n::tr("Practice", "InfoChartName"),
        &chart.chart_name,
    );
    push_info_line(
        &mut status,
        &i18n::tr("Practice", "InfoStepAuthor"),
        &chart.step_artist,
    );
    push_info_line(
        &mut status,
        &i18n::tr("Practice", "InfoChartStyle"),
        &chart.chart_type,
    );
    status.push('\n');
    let totals = gameplay.display_totals_for_player(0);
    let stat_lines: [(&str, PracticeNumber); 8] = [
        (
            "InfoNumSteps",
            PracticeNumber::format(format_args!("{}", chart.stats.total_steps)),
        ),
        (
            "InfoNumJumps",
            PracticeNumber::format(format_args!("{}", chart.stats.jumps)),
        ),
        (
            "InfoNumHands",
            PracticeNumber::format(format_args!("{}", chart.stats.hands)),
        ),
        (
            "InfoNumHolds",
            PracticeNumber::format(format_args!("{}", totals.holds_total)),
        ),
        (
            "InfoNumMines",
            PracticeNumber::format(format_args!("{}", totals.mines_total)),
        ),
        (
            "InfoNumRolls",
            PracticeNumber::format(format_args!("{}", totals.rolls_total)),
        ),
        (
            "InfoNumLifts",
            PracticeNumber::format(format_args!("{}", chart.stats.lifts)),
        ),
        (
            "InfoNumFakes",
            PracticeNumber::format(format_args!("{}", chart.stats.fakes)),
        ),
    ];
    for (idx, (key, count)) in stat_lines.iter().enumerate() {
        if idx > 0 {
            status.push('\n');
        }
        i18n::tr_fmt_into(&mut status, "Practice", key, &[("count", count.as_str())]);
    }
    status
}

fn push_selection_info(
    out: &mut String,
    selection_anchor: Option<f32>,
    selection_end: Option<f32>,
) -> bool {
    match (selection_anchor, selection_end) {
        (Some(start), Some(stop)) if stop > start => {
            let start = PracticeNumber::format(format_args!("{start:.3}"));
            let stop = PracticeNumber::format(format_args!("{stop:.3}"));
            i18n::tr_fmt_into(
                out,
                "Practice",
                "InfoSelectionBeatRange",
                &[("start", start.as_str()), ("stop", stop.as_str())],
            );
            true
        }
        (Some(start), None) => {
            let start = PracticeNumber::format(format_args!("{start:.3}"));
            i18n::tr_fmt_into(
                out,
                "Practice",
                "InfoSelectionBeatStart",
                &[("start", start.as_str())],
            );
            true
        }
        (None, Some(stop)) => {
            let stop = PracticeNumber::format(format_args!("{stop:.3}"));
            i18n::tr_fmt_into(
                out,
                "Practice",
                "InfoSelectionBeatEnd",
                &[("stop", stop.as_str())],
            );
            true
        }
        _ => false,
    }
}

fn push_info_line(status: &mut String, label: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    status.push_str(label);
    status.push_str(":\n  ");
    push_clipped(status, value, EDIT_INFO_VALUE_CHARS);
    status.push('\n');
}

fn push_clipped(status: &mut String, value: &str, max_chars: usize) {
    for (idx, ch) in value.chars().enumerate() {
        if idx == max_chars {
            status.push_str("...");
            return;
        }
        status.push(ch);
    }
}

fn append_main_menu(state: &mut PracticeOverlay<'_>, actors: &mut Vec<Actor>) {
    let Some(menu) = state.session.menu() else {
        return;
    };
    let row_count = menu.kind.row_count();
    let selected_color = practice_player_color(state);
    state.presentation.menu_text.sync(menu.kind);
    let labels = state.presentation.menu_text
        .labels
        .as_deref()
        .expect("Practice menu labels sync before actor construction");
    debug_assert_eq!(labels.len(), row_count);
    for (idx, label) in labels.iter().enumerate() {
        append_menu_row(
            actors,
            idx,
            row_count,
            menu.selected == idx,
            label,
            selected_color,
        );
    }
}

fn append_menu_row(
    actors: &mut Vec<Actor>,
    idx: usize,
    row_count: usize,
    selected: bool,
    label: &Arc<str>,
    selected_color: [f32; 4],
) {
    let y = menu_row_y(idx, row_count);
    let bg_x = screen_center_x();
    let bg_w = widescale(543.0, 720.0);
    let (bg, fg) = if selected {
        ([0.161, 0.196, 0.22, 0.95], selected_color)
    } else {
        ([0.027, 0.063, 0.086, 0.95], [1.0, 1.0, 1.0, 1.0])
    };
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(bg_x, y):
        zoomto(bg_w, EDIT_MENU_ROW_BG_HEIGHT):
        diffuse(bg[0], bg[1], bg[2], bg[3]):
        z(3100)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(label):
        align(0.0, 0.5):
        xy(screen_center_x() - EDIT_MENU_TITLE_X_OFFSET, y):
        zoom(EDIT_MENU_TEXT_ZOOM):
        diffuse(fg[0], fg[1], fg[2], fg[3]):
        shadowlength(1.0):
        z(3101)
    ));
}

fn menu_row_y(idx: usize, row_count: usize) -> f32 {
    if row_count.is_multiple_of(2) {
        EDIT_MENU_ROW_HEIGHT.mul_add(
            0.5,
            EDIT_MENU_ROW_HEIGHT.mul_add(idx as f32 - (row_count / 2) as f32, screen_center_y()),
        )
    } else {
        EDIT_MENU_ROW_HEIGHT.mul_add(idx as f32 - (row_count / 2) as f32, screen_center_y())
    }
}

fn append_help_section(
    actors: &mut Vec<Actor>,
    label: &Arc<str>,
    body: &Arc<str>,
    y: f32,
    player_color: [f32; 4],
    header_font: &'static str,
) {
    actors.push(act!(text:
        font(header_font):
        settext(label):
        align(0.0, 0.5):
        xy(35.0, y + 10.0):
        zoom(EDIT_HELP_HEADER_ZOOM):
        diffuse(player_color[0], player_color[1], player_color[2], 1.0):
        z(3000)
    ));
    actors.push(act!(quad:
        align(0.0, 0.5):
        xy(0.0, y + 10.0):
        zoomto(30.0, 1.0):
        diffuse(1.0, 1.0, 1.0, 0.75):
        z(2999)
    ));
    actors.push(act!(text:
        font("miso"):
        settext(body):
        align(0.0, 0.0):
        xy(10.0, y + 20.0):
        zoom(0.55):
        diffuse(1.0, 1.0, 1.0, 1.0):
        shadowlength(1.0):
        z(3000)
    ));
}

#[cfg(test)]
mod tests {
    use super::{
        EditInfoSource, HELP_MENU_LABELS, PracticeMainMenuItem, PracticeMenuKind, PracticeMenuText, PracticeNumber,
        append_pending_effects, build_edit_info_text_into, compile_timing_labels,
        fmt_itg_float, fmt_music_rate, normalize_flash_text,
        practice_density_geom, practice_edit_beat_travel, practice_style_notefield_width,
        push_selection_info, rotate_practice_density_mesh, timing_speed_label,
    };
    use crate::SimplyLoveEffect as ThemeEffect;
    use crate::SimplyLoveRuntimeRequest;
    use crate::i18n;
    use crate::screens::Screen;
    use deadlib_render_core::MeshVertex;
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::timing::{
        DelaySegment, FakeSegment, ScrollSegment, SpeedSegment, SpeedUnit, StopSegment,
        TimeSignatureSegment, TimingSegments, WarpSegment,
    };
    use deadsync_theme::AudioRequest;
    use std::sync::Arc;

    /// Every i18n key the practice screen looks up at runtime, outside of the
    /// menu definitions (which already have their own coverage tests). Keep
    /// this in sync with call sites — the test below proves each key resolves.
    const PRACTICE_RUNTIME_KEYS: &[&str] = &[
        "TitlePracticeMode",
        "HelpSidebarNavigatingTitle",
        "HelpSidebarNavigatingBody",
        "HelpSidebarMenusTitle",
        "HelpSidebarMenusBody",
        "HelpSidebarMiscTitle",
        "HelpSidebarMiscBody",
        "FlashZoomChanged",
        "FlashMusicRate",
        "FlashMusicRateLimit",
        "FlashAreaMarkerStartSet",
        "FlashAreaMarkerEndSet",
        "FlashInvalidSelectionStart",
        "FlashSelectionStartSet",
        "FlashInvalidSelectionEnd",
        "FlashSelectionEndSet",
        "FlashSelectionCleared",
        "FlashAutoplayOn",
        "FlashAutoplayOff",
        "FlashTicksAssist",
        "FlashTicksHit",
        "FlashTicksOff",
        "FlashAutosyncOff",
        "FlashAutosyncSong",
        "FlashAutosyncMachine",
        "FlashGlobalOffset",
        "FlashSongOffset",
        "InfoCurrentBeat",
        "InfoCurrentSecond",
        "InfoSnapTo",
        "InfoDifficulty",
        "InfoSelectionBeatRange",
        "InfoSelectionBeatStart",
        "InfoSelectionBeatEnd",
        "InfoMainTitle",
        "InfoSubtitle",
        "InfoDescription",
        "InfoChartName",
        "InfoStepAuthor",
        "InfoChartStyle",
        "InfoNumSteps",
        "InfoNumJumps",
        "InfoNumHands",
        "InfoNumHolds",
        "InfoNumMines",
        "InfoNumRolls",
        "InfoNumLifts",
        "InfoNumFakes",
    ];

    #[test]
    fn edit_info_key_tracks_every_dynamic_text_source() {
        let source = EditInfoSource {
            cursor_beat: 12.0,
            current_second: 6.0,
            selection_anchor: Some(8.0),
            selection_end: Some(16.0),
            snap_index: 3,
        };
        let key = source.key();
        assert_eq!(key, source.key());
        for changed in [
            EditInfoSource {
                cursor_beat: 12.25,
                ..source
            },
            EditInfoSource {
                current_second: 6.125,
                ..source
            },
            EditInfoSource {
                selection_anchor: None,
                ..source
            },
            EditInfoSource {
                selection_end: None,
                ..source
            },
            EditInfoSource {
                snap_index: 4,
                ..source
            },
        ] {
            assert_ne!(key, changed.key());
        }
    }

    #[test]
    fn edit_info_text_preserves_numeric_and_selection_formatting() {
        i18n::init_for_tests();
        let mut text = String::new();
        build_edit_info_text_into(
            &mut text,
            EditInfoSource {
                cursor_beat: 128.375,
                current_second: 63.125,
                selection_anchor: Some(96.0),
                selection_end: Some(144.5),
                snap_index: 3,
            },
            "suffix",
        );
        for expected in ["128.375", "63.125000", "96.000", "144.500", "16th"] {
            assert!(text.contains(expected), "missing {expected:?} in {text:?}");
        }
        assert!(text.ends_with("suffix"));
        assert!(!text.contains('{') && !text.contains('}'));

        let huge = f32::MAX;
        let expected = format!("{huge:.3}");
        let formatted = PracticeNumber::format(format_args!("{huge:.3}"));
        assert_eq!(formatted.as_str(), expected);

        let mut invalid = String::new();
        assert!(!push_selection_info(&mut invalid, Some(12.0), Some(8.0)));
        assert!(invalid.is_empty());
    }

    #[test]
    fn flash_text_reuses_plain_translations_and_normalizes_escaped_lines() {
        let plain: Arc<str> = Arc::from("Music rate 1.25x");
        let normalized = normalize_flash_text(Arc::clone(&plain));
        assert!(Arc::ptr_eq(&plain, &normalized));

        let multiline = normalize_flash_text(Arc::from("Music rate 1.25x\\n180 BPM"));
        assert_eq!(multiline.as_ref(), "Music rate 1.25x\n180 BPM");
    }

    #[test]
    fn pending_practice_sfx_keep_order_before_navigation() {
        let mut pending = vec!["assets/sounds/change.ogg", "assets/sounds/start.ogg"];
        let mut effects = Vec::new();
        append_pending_effects(
            std::iter::empty(),
            &mut pending,
            ThemeEffect::Navigate(Screen::PlayerOptions),
            &mut effects,
        );
        let [first, second, ThemeEffect::Navigate(Screen::PlayerOptions)] = effects.as_slice()
        else {
            panic!("Practice SFX should precede navigation");
        };
        for (effect, expected) in [
            (first, "assets/sounds/change.ogg"),
            (second, "assets/sounds/start.ogg"),
        ] {
            let ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(path))) =
                effect
            else {
                panic!("queued Practice sound should become an audio request");
            };
            assert_eq!(*path, expected);
        }
        assert!(pending.is_empty());
    }

    #[test]
    fn practice_rate_persistence_precedes_sound_and_navigation() {
        let mut sounds = vec!["assets/sounds/change.ogg"];
        let mut effects = Vec::new();
        append_pending_effects(
            [1.25].into_iter(),
            &mut sounds,
            ThemeEffect::Navigate(Screen::PlayerOptions),
            &mut effects,
        );

        assert!(matches!(
            &effects[0],
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Profile(
                crate::SimplyLoveProfileRequest::SetMusicRate(rate)
            )) if (*rate - 1.25).abs() < f32::EPSILON
        ));
        assert!(matches!(
            &effects[1],
            ThemeEffect::Runtime(SimplyLoveRuntimeRequest::Audio(AudioRequest::PlaySfx(path)))
                if *path == "assets/sounds/change.ogg"
        ));
        assert!(matches!(
            effects[2],
            ThemeEffect::Navigate(Screen::PlayerOptions)
        ));
        assert!(sounds.is_empty());
    }

    #[test]
    fn fmt_music_rate_matches_player_options_format() {
        assert_eq!(fmt_music_rate(1.0), "1");
        assert_eq!(fmt_music_rate(1.5), "1.5");
        assert_eq!(fmt_music_rate(0.85), "0.85");
        assert_eq!(fmt_music_rate(2.05), "2.05");
        assert_eq!(fmt_music_rate(0.5), "0.5");
    }

    #[test]
    fn timing_label_numbers_match_itg_to_string_shape() {
        assert_eq!(fmt_itg_float(120.0), "120.000000");
        assert_eq!(fmt_itg_float(0.5), "0.500000");
    }

    #[test]
    fn timing_speed_label_matches_itg_multiline_value() {
        let beats = SpeedSegment {
            beat: 16.0,
            ratio: 2.0,
            delay: 0.5,
            unit: SpeedUnit::Beats,
        };
        let seconds = SpeedSegment {
            unit: SpeedUnit::Seconds,
            ..beats
        };
        assert_eq!(timing_speed_label(beats), "2.000000\nB\n0.500000");
        assert_eq!(timing_speed_label(seconds), "2.000000\nS\n0.500000");
    }

    #[test]
    fn compiled_timing_labels_preserve_draw_order_and_text() {
        let timing = TimingSegments {
            scrolls: vec![ScrollSegment {
                beat: 1.0,
                ratio: 0.5,
            }],
            bpms: vec![(2.0, 120.0)],
            stops: vec![StopSegment {
                beat: 3.0,
                duration: 0.25,
            }],
            delays: vec![DelaySegment {
                beat: 4.0,
                duration: 0.125,
            }],
            warps: vec![WarpSegment {
                beat: 5.0,
                length: 8.0,
            }],
            time_signatures: vec![TimeSignatureSegment {
                beat: 6.0,
                numerator: 3,
                denominator: 4,
            }],
            speeds: vec![SpeedSegment {
                beat: 7.0,
                ratio: 2.0,
                delay: 0.5,
                unit: SpeedUnit::Beats,
            }],
            fakes: vec![FakeSegment {
                beat: 8.0,
                length: 4.0,
            }],
            ..TimingSegments::default()
        };

        let labels = compile_timing_labels(&timing);
        let actual: Vec<_> = labels
            .iter()
            .map(|label| (label.beat, label.text.as_ref()))
            .collect();
        assert_eq!(
            actual,
            [
                (1.0, "0.500000"),
                (2.0, "120.000000"),
                (3.0, "0.250000"),
                (4.0, "0.125000"),
                (5.0, "8.000000"),
                (6.0, "3\n--\n4"),
                (7.0, "2.000000\nB\n0.500000"),
                (8.0, "4.000000"),
            ]
        );
    }

    #[test]
    fn practice_edit_travel_uses_step_editor_beat_spacing() {
        let travel =
            practice_edit_beat_travel(44.0, 40.0, 0.5, ScrollSpeedSetting::XMod(1.5), 180.0, 1.0);

        assert!((4.0_f32 * 64.0 * 0.5).mul_add(-1.5, travel).abs() <= 0.001);
    }

    #[test]
    fn practice_density_graph_matches_wide_single_layout() {
        let geom = practice_density_geom(854.0, 480.0, 256.0, false);

        assert!((geom.x - 573.923_1).abs() < 0.001);
        assert_eq!(geom.y, 80.0);
        assert_eq!(geom.length, 380.0);
        assert_eq!(geom.thickness, 50.0);
    }

    #[test]
    fn practice_density_graph_matches_four_three_double_layout() {
        let geom = practice_density_geom(640.0, 480.0, 512.0, true);

        assert!((geom.x - 484.629_64).abs() < 0.001);
        assert_eq!(geom.y, 180.0);
        assert_eq!(geom.length, 280.0);
        assert_eq!(geom.thickness, 25.0);
    }

    #[test]
    fn practice_density_graph_uses_game_specific_column_widths() {
        assert_eq!(practice_style_notefield_width("dance-single", 4), 256.0);
        assert_eq!(practice_style_notefield_width("pump-single", 5), 250.0);
        assert_eq!(practice_style_notefield_width("pump-double", 10), 500.0);
    }

    #[test]
    fn practice_density_mesh_rotates_into_right_facing_strip() {
        let mut vertices = [
            MeshVertex {
                pos: [10.0, 50.0],
                color: [1.0; 4],
            },
            MeshVertex {
                pos: [10.0, 20.0],
                color: [1.0; 4],
            },
        ];

        rotate_practice_density_mesh(&mut vertices, 50.0);

        assert_eq!(vertices[0].pos, [0.0, 10.0]);
        assert_eq!(vertices[1].pos, [30.0, 10.0]);
    }

    #[test]
    fn help_menu_item_keys_resolve_through_i18n() {
        i18n::init_for_tests();
        assert_menu_labels_localized(HELP_MENU_LABELS);
    }

    #[test]
    fn main_menu_item_keys_resolve_through_i18n() {
        i18n::init_for_tests();
        assert_menu_labels_localized(PracticeMainMenuItem::ALL.map(super::main_menu_label));
    }

    #[test]
    fn menu_text_reuses_stable_rows_and_rebuilds_on_source_change() {
        i18n::init_for_tests();
        let mut text = PracticeMenuText::new();

        text.sync(PracticeMenuKind::Main);
        let first_rows = text.labels.as_deref().expect("main labels compiled");
        let first_ptr = first_rows.as_ptr() as usize;
        assert_eq!(first_rows.len(), PracticeMenuKind::Main.row_count());

        text.sync(PracticeMenuKind::Main);
        assert_eq!(
            text.labels
                .as_deref()
                .expect("stable main labels retained")
                .as_ptr() as usize,
            first_ptr
        );

        text.i18n_revision = text.i18n_revision.wrapping_sub(1);
        text.sync(PracticeMenuKind::Main);
        assert_ne!(
            text.labels
                .as_deref()
                .expect("revision rebuilt main labels")
                .as_ptr() as usize,
            first_ptr
        );

        text.sync(PracticeMenuKind::Help);
        assert_eq!(
            text.labels.as_deref().expect("help labels compiled").len(),
            PracticeMenuKind::Help.row_count()
        );
        assert_eq!(text.kind, Some(PracticeMenuKind::Help));
    }

    #[test]
    fn all_practice_runtime_keys_resolve_through_i18n() {
        i18n::init_for_tests();
        for key in PRACTICE_RUNTIME_KEYS {
            let resolved = i18n::tr("Practice", key);
            assert_ne!(
                resolved.as_ref(),
                format!("Practice.{key}").as_str(),
                "missing i18n entry for Practice.{key}"
            );
        }
    }

    #[test]
    fn placeholder_keys_substitute_named_args() {
        i18n::init_for_tests();
        let rate = i18n::tr_fmt("Practice", "FlashMusicRate", &[("rate", "1.5")]);
        assert!(
            rate.contains("1.5") && !rate.contains("{rate}"),
            "FlashMusicRate did not substitute placeholder: {rate}"
        );
        let beat = i18n::tr_fmt("Practice", "InfoCurrentBeat", &[("beat", "3.000")]);
        assert!(
            beat.contains("3.000") && !beat.contains("{beat}"),
            "InfoCurrentBeat did not substitute placeholder: {beat}"
        );
    }

    fn assert_menu_labels_localized(labels: impl IntoIterator<Item = super::LookupKey>) {
        for label in labels {
            let resolved = label.get();
            let fallback = format!("{}.{}", label.section, label.key);
            assert_ne!(
                resolved.as_ref(),
                fallback.as_str(),
                "missing i18n entry for {fallback}"
            );
        }
    }
}
