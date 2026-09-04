use crate::act;
use crate::assets::i18n::{tr, tr_fmt};
use crate::assets::{self, AssetManager};
use crate::screens::components::shared::screen_bar::{
    ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::shared::{screen_bar, visual_style_bg};
use crate::screens::input as screen_input;
use crate::screens::{Screen, ThemeEffect};
use crate::views::ProfilePickerView;
use deadsync_assets::noteskin::{self, Noteskin};

use deadlib_present::actors::{self, Actor};
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y};
use deadlib_render_core::BlendMode;
use deadsync_config::prelude::GameFlag;
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_notefield::{
    ModelMeshCache, noteskin_model_actor_from_draw, noteskin_model_actor_from_draw_cached,
};
use deadsync_noteskin::{NUM_QUANTIZATIONS, Quantization, Style};
use deadsync_profile as profile_data;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/* ---------------------------- transitions ---------------------------- */
const TRANSITION_IN_DURATION: f32 = 0.4;
const TRANSITION_OUT_DURATION: f32 = 0.4;
// Simply Love:
// - PlayerFrame.lua: bouncebegin(0.35):zoom(0)
// - default.lua: OffCommand sleep(0.5) to let PlayerFrames tween out
const EXIT_ANIM_DURATION: f32 = 0.5;
const PLAYERFRAME_EXIT_ZOOM_OUT_DURATION: f32 = 0.35;

// Simply Love:
// PlayerFrame.lua: PlayerJoinedMessageCommand -> zoom(1.15):bounceend(0.175):zoom(1)
const JOIN_PULSE_ZOOM_IN: f32 = 1.15;
const JOIN_PULSE_DURATION: f32 = 0.175;

#[must_use]
pub const fn exit_anim_duration() -> f32 {
    EXIT_ANIM_DURATION
}

/* ------------------------------ layout ------------------------------- */
const ROW_H: f32 = 35.0;
const ROWS_VISIBLE: i32 = 9;
// Name scroller tween: offset (in rows) decays toward 0 with this exponential time
// constant so a navigation slides quickly but smoothly instead of snapping.
const SCROLL_TWEEN_TAU: f32 = 0.045;
// Snap the scroll offset to rest once it is within this many rows of settled.
const SCROLL_SNAP_EPS: f32 = 0.002;
const FRAME_BASE_W: f32 = 200.0;
const FRAME_W_SCROLLER: f32 = FRAME_BASE_W * 1.1;
const FRAME_W_JOIN: f32 = FRAME_BASE_W * 0.9;
const FRAME_H: f32 = 214.0;
const FRAME_BORDER: f32 = 2.0;
const FRAME_CX_OFF: f32 = 150.0;
const FRAME_IN_CROP_DUR: f32 = 0.30; // SL: smooth(0.3):cropbottom(0)
const OVERLAY_IN_DELAY: f32 = 0.30; // SL: sleep(0.3)
const OVERLAY_IN_DUR: f32 = 0.10; // SL: linear(0.1)

const INFO_W: f32 = FRAME_BASE_W * 0.475;
const INFO_X0_OFF: f32 = 15.5;
const INFO_PAD: f32 = 4.0;

const SCROLLER_W: f32 = FRAME_W_SCROLLER - INFO_W;
const SCROLLER_CX_OFF: f32 = -47.0;
const SCROLLER_TEXT_PAD_X: f32 = 6.0;

const AVATAR_BG_HEX: &str = "#283239aa";
const AVATAR_X_OFF: f32 = INFO_PAD * 1.125;
const AVATAR_Y_OFF: f32 = -103.5;
const AVATAR_HEART_X: f32 = 13.0;
const AVATAR_HEART_Y: f32 = 8.0;
const AVATAR_HEART_ZOOM: f32 = 0.09;
const AVATAR_TEXT_Y: f32 = 67.0;
const AVATAR_LABEL_ZOOM: f32 = 0.815; // SL: fallback avatar label zoom(0.815)

const INFO_LINE_Y_OFF: f32 = 18.0;
// Unified Y offset for side-by-side previews
const PREVIEW_Y_OFF: f32 = 42.0;

const TOTAL_SONGS_ZOOM: f32 = 0.65; // SL: TotalSongs zoom(0.65)
const MODS_ZOOM: f32 = 0.625; // SL: RecentMods zoom(0.625)
const MODS_Y_OFF: f32 = 47.0; // SL: RecentMods xy(...,47)

const SELECTED_NAME_Y_OFF: f32 = 160.0; // SL: SelectedProfileText y(160)
const SELECTED_NAME_ZOOM: f32 = 1.35; // SL: SelectedProfileText zoom(1.35)

const SHAKE_STEP_DUR: f32 = 0.1; // SL: bounceend(0.1) x3
const SHAKE_DUR: f32 = SHAKE_STEP_DUR * 3.0;

#[derive(Clone)]
struct Choice {
    kind: profile_data::ActiveProfile,
    display_name: Arc<str>,
    avatar_key: Option<Arc<str>>,
    total_songs: Arc<str>,
    recent_mods: Arc<str>,
    noteskin: profile_data::NoteSkin,
    judgment: profile_data::JudgmentGraphic,
    judgment_texture: Option<&'static str>,
}

pub struct State {
    pub active_color_index: i32,
    fast_switch: bool,
    p1_joined: bool,
    p2_joined: bool,
    p1_ready: bool,
    p2_ready: bool,
    p1_selected_index: usize,
    p2_selected_index: usize,
    exit_anim: bool,
    choices: Vec<Choice>,
    three_key_navigation: bool,
    bg: visual_style_bg::State,
    noteskin_cache: NoteskinCache,
    /// Picker-lifetime model geometry retained by stable noteskin slot ID.
    /// The cache grows only when a newly selected profile introduces a model,
    /// never evicts, and drops with the picker. Ordinary frames clone geometry
    /// handles while still evaluating the animated draw state every frame.
    model_mesh_cache: RefCell<ModelMeshCache>,
    /// Marker-expanded translations retained for this picker lifetime. Language
    /// changes rebuild screens, so render frames only clone these handles.
    join_text: Arc<str>,
    waiting_text: Arc<str>,
    p1_preview_noteskin: Option<Arc<Noteskin>>,
    p2_preview_noteskin: Option<Arc<Noteskin>>,
    preview_time: f32,
    preview_beat: f32,
    p1_join_pulse_t: f32,
    p2_join_pulse_t: f32,
    p1_shake_t: f32,
    p2_shake_t: f32,
    // Animated vertical offset (in rows) of the name scroller, lagging behind the
    // logical selected index and decaying to 0 to produce the scroll tween.
    p1_scroll_anim: f32,
    p2_scroll_anim: f32,
    menu_lr_chord: screen_input::MenuLrChordTracker,
    menu_lr_undo: [i8; 2],
}

/// Logic-thread-only cache owned for one profile-picker screen lifetime.
/// It is sized for the profile choices plus the default skin, warms the
/// default during screen init, and loads at most one skin on a menu-frame miss.
/// Entries are never evicted and are dropped with the screen; this cache is not
/// used during gameplay. There is no instrumentation because the bounded set is
/// visible in the picker, and its worst-case frame work is one noteskin load.
struct NoteskinCache {
    cache: HashMap<String, Arc<Noteskin>>,
    style: Style,
}

impl NoteskinCache {
    fn new(game: GameFlag, choice_count: usize) -> Self {
        let style = Style {
            num_cols: match game {
                GameFlag::Dance => 4,
                GameFlag::Pump => 5,
            },
            num_players: 1,
        };
        let mut cache = HashMap::with_capacity(choice_count.saturating_add(1));
        if let Ok(default_skin) =
            noteskin::load_itg_skin_cached(&style, profile_data::NoteSkin::DEFAULT_NAME)
        {
            cache.insert(
                profile_data::NoteSkin::DEFAULT_NAME.to_string(),
                default_skin,
            );
        }
        Self { cache, style }
    }

    fn get(&mut self, kind: &profile_data::NoteSkin) -> Option<Arc<Noteskin>> {
        let requested = kind.as_str();
        if let Some(cached) = self.cache.get(requested) {
            return Some(cached.clone());
        }

        if let Ok(loaded) = noteskin::load_itg_skin_cached(&self.style, requested) {
            self.cache.insert(requested.to_string(), loaded.clone());
            return Some(loaded);
        }

        if let Some(default_cached) = self.cache.get(profile_data::NoteSkin::DEFAULT_NAME) {
            return Some(default_cached.clone());
        }

        if let Ok(default_loaded) =
            noteskin::load_itg_skin_cached(&self.style, profile_data::NoteSkin::DEFAULT_NAME)
        {
            self.cache.insert(
                profile_data::NoteSkin::DEFAULT_NAME.to_string(),
                default_loaded.clone(),
            );
            return Some(default_loaded);
        }

        self.cache.values().next().cloned()
    }
}

#[inline(always)]
const fn preview_col(style: Style) -> usize {
    if style.is_pump() { 3 } else { 2 }
}

fn preview_noteskin_for_choice(
    cache: &mut NoteskinCache,
    choices: &[Choice],
    selected_index: usize,
) -> Option<Arc<Noteskin>> {
    let choice = choices.get(selected_index)?;
    match choice.kind {
        profile_data::ActiveProfile::Guest => None,
        profile_data::ActiveProfile::Local { .. } => cache.get(&choice.noteskin),
    }
}

#[inline(always)]
fn format_total_songs_played(count: u32) -> String {
    let count_str = count.to_string();
    if count == 1 {
        tr_fmt(
            "SelectProfile",
            "SongPlayedSingular",
            &[("count", &count_str)],
        )
        .to_string()
    } else {
        tr_fmt(
            "SelectProfile",
            "SongPlayedPlural",
            &[("count", &count_str)],
        )
        .to_string()
    }
}

#[inline(always)]
fn format_recent_mods(
    speed_mod: &str,
    scroll: profile_data::ScrollOption,
    mini_indicator: profile_data::MiniIndicator,
    noteskin: &profile_data::NoteSkin,
) -> String {
    let mut out = String::new();
    let mut first = true;

    let mut push = |s: &str| {
        if s.is_empty() {
            return;
        }
        if !first {
            out.push_str(", ");
        }
        first = false;
        out.push_str(s);
    };

    push(speed_mod.trim());
    if scroll.contains(profile_data::ScrollOption::Reverse) {
        let s = tr("SelectProfile", "Reverse");
        push(&s);
    }
    if scroll.contains(profile_data::ScrollOption::Split) {
        let s = tr("SelectProfile", "Split");
        push(&s);
    }
    if scroll.contains(profile_data::ScrollOption::Alternate) {
        let s = tr("SelectProfile", "Alternate");
        push(&s);
    }
    if scroll.contains(profile_data::ScrollOption::Cross) {
        let s = tr("SelectProfile", "Cross");
        push(&s);
    }
    if scroll.contains(profile_data::ScrollOption::Centered) {
        let s = tr("SelectProfile", "Centered");
        push(&s);
    }
    let overhead = tr("SelectProfile", "Overhead");
    push(&overhead);
    push(noteskin.as_str());
    let mini_indicator_label = match mini_indicator {
        profile_data::MiniIndicator::None => None,
        profile_data::MiniIndicator::SubtractiveScoring => {
            Some(tr("SelectProfile", "SubtractiveScoring"))
        }
        profile_data::MiniIndicator::PredictiveScoring => {
            Some(tr("SelectProfile", "PredictiveScoring"))
        }
        profile_data::MiniIndicator::PaceScoring => Some(tr("SelectProfile", "PaceScoring")),
        profile_data::MiniIndicator::RivalScoring => Some(tr("SelectProfile", "RivalScoring")),
        profile_data::MiniIndicator::Pacemaker => Some(tr("SelectProfile", "Pacemaker")),
        profile_data::MiniIndicator::StreamProg => Some(tr("SelectProfile", "StreamProgress")),
    };
    if let Some(label) = mini_indicator_label {
        push(&label);
    }
    out
}

fn build_choices(
    guest: crate::views::ProfilePickerEntryView,
    profiles: Vec<crate::views::ProfilePickerEntryView>,
) -> Vec<Choice> {
    let mut out = Vec::with_capacity(profiles.len() + 1);
    let guest_mods = format_recent_mods(
        &guest.speed_mod,
        guest.scroll_option,
        guest.mini_indicator,
        &guest.noteskin,
    );
    let guest_judgment_texture = assets::resolve_texture_choice(
        guest.judgment.texture_key(),
        assets::judgment_texture_choices(),
    );
    out.push(Choice {
        kind: profile_data::ActiveProfile::Guest,
        display_name: tr("SelectProfile", "GuestLabel"),
        avatar_key: None,
        total_songs: Arc::from(""),
        recent_mods: guest_mods.into(),
        noteskin: guest.noteskin,
        judgment: guest.judgment,
        judgment_texture: guest_judgment_texture,
    });
    for profile in profiles {
        let recent_mods = format_recent_mods(
            &profile.speed_mod,
            profile.scroll_option,
            profile.mini_indicator,
            &profile.noteskin,
        );
        let judgment_texture = assets::resolve_texture_choice(
            profile.judgment.texture_key(),
            assets::judgment_texture_choices(),
        );
        out.push(Choice {
            kind: profile_data::ActiveProfile::Local { id: profile.id },
            display_name: profile.display_name.into(),
            avatar_key: profile.avatar_key.map(Arc::from),
            total_songs: format_total_songs_played(profile.total_songs_played).into(),
            recent_mods: recent_mods.into(),
            noteskin: profile.noteskin,
            judgment: profile.judgment,
            judgment_texture,
        });
    }
    out
}

fn selected_index_for(choices: &[Choice], active: profile_data::ActiveProfile) -> usize {
    match active {
        profile_data::ActiveProfile::Guest => 0,
        profile_data::ActiveProfile::Local { id } => choices
            .iter()
            .position(|c| match &c.kind {
                profile_data::ActiveProfile::Local { id: cid } => cid == &id,
                profile_data::ActiveProfile::Guest => false,
            })
            .unwrap_or(0),
    }
}

fn prepared_translation(section: &str, key: &str) -> Arc<str> {
    let text = tr(section, key);
    if !text.as_bytes().contains(&b'&') {
        return text;
    }
    deadlib_present::font::replace_markers(text.as_ref())
        .into_owned()
        .into()
}

fn init_with_profiles(
    view: ProfilePickerView,
    p1_profile: profile_data::ActiveProfile,
    p2_profile: profile_data::ActiveProfile,
) -> State {
    let ProfilePickerView {
        game,
        guest,
        profiles,
        three_key_navigation,
        ..
    } = view;
    let choices = build_choices(guest, profiles);
    let noteskin_cache = NoteskinCache::new(game, choices.len());
    let p1_selected_index = selected_index_for(&choices, p1_profile);
    let p2_selected_index = selected_index_for(&choices, p2_profile);

    let mut state = State {
        active_color_index: color::DEFAULT_COLOR_INDEX,
        fast_switch: false,
        p1_joined: true,
        p2_joined: false,
        p1_ready: false,
        p2_ready: false,
        p1_selected_index,
        p2_selected_index,
        exit_anim: false,
        choices,
        three_key_navigation,
        bg: visual_style_bg::State::new(),
        noteskin_cache,
        model_mesh_cache: RefCell::new(ModelMeshCache::with_capacity(8)),
        join_text: prepared_translation("SelectProfile", "JoinText"),
        waiting_text: prepared_translation("SelectProfile", "WaitingText"),
        p1_preview_noteskin: None,
        p2_preview_noteskin: None,
        preview_time: 0.0,
        preview_beat: 0.0,
        p1_join_pulse_t: JOIN_PULSE_DURATION,
        p2_join_pulse_t: JOIN_PULSE_DURATION,
        p1_shake_t: SHAKE_DUR,
        p2_shake_t: SHAKE_DUR,
        p1_scroll_anim: 0.0,
        p2_scroll_anim: 0.0,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        menu_lr_undo: [0; 2],
    };
    state.p1_preview_noteskin = preview_noteskin_for_choice(
        &mut state.noteskin_cache,
        &state.choices,
        state.p1_selected_index,
    );
    state.p2_preview_noteskin = preview_noteskin_for_choice(
        &mut state.noteskin_cache,
        &state.choices,
        state.p2_selected_index,
    );
    state
}

#[must_use]
pub fn init(view: ProfilePickerView) -> State {
    let mut view = view;
    let [p1, p2] = std::mem::replace(
        &mut view.default_profiles,
        std::array::from_fn(|_| profile_data::ActiveProfile::Guest),
    );
    init_with_profiles(view, p1, p2)
}

#[must_use]
pub fn init_active(
    view: ProfilePickerView,
    active_profiles: [profile_data::ActiveProfile; 2],
) -> State {
    let [p1, p2] = active_profiles;
    init_with_profiles(view, p1, p2)
}

#[must_use]
pub fn init_late_join(
    view: ProfilePickerView,
    joining_side: profile_data::PlayerSide,
    active_profiles: [profile_data::ActiveProfile; 2],
) -> State {
    let mut view = view;
    let defaults = std::mem::replace(
        &mut view.default_profiles,
        std::array::from_fn(|_| profile_data::ActiveProfile::Guest),
    );
    let [p1_active, p2_active] = active_profiles;
    let [p1_default, p2_default] = defaults;
    let p1_profile = match joining_side {
        profile_data::PlayerSide::P1 => p1_default,
        profile_data::PlayerSide::P2 => p1_active,
    };
    let p2_profile = match joining_side {
        profile_data::PlayerSide::P1 => p2_active,
        profile_data::PlayerSide::P2 => p2_default,
    };
    init_with_profiles(view, p1_profile, p2_profile)
}

pub fn set_joined(state: &mut State, p1_joined: bool, p2_joined: bool) {
    state.p1_joined = p1_joined;
    state.p2_joined = p2_joined;
    state.p1_ready = false;
    state.p2_ready = false;
    state.p1_join_pulse_t = JOIN_PULSE_DURATION;
    state.p2_join_pulse_t = JOIN_PULSE_DURATION;

    state.p1_preview_noteskin = preview_noteskin_for_choice(
        &mut state.noteskin_cache,
        &state.choices,
        state.p1_selected_index,
    );
    state.p2_preview_noteskin = preview_noteskin_for_choice(
        &mut state.noteskin_cache,
        &state.choices,
        state.p2_selected_index,
    );
}

#[inline(always)]
pub const fn set_fast_switch(state: &mut State, enabled: bool) {
    state.fast_switch = enabled;
}

/// Configure the overlay for a late-join scenario: the existing player is
/// pre-readied with their current profile, and only `joining_side` needs to
/// pick a profile. Used when a second player presses Start mid-set on a
/// screen with an embedded profile-select overlay.
pub fn enter_late_join(state: &mut State, joining_side: profile_data::PlayerSide) {
    match joining_side {
        profile_data::PlayerSide::P1 => {
            state.p1_joined = true;
            state.p1_ready = false;
            state.p1_join_pulse_t = 0.0;
            state.p2_joined = true;
            state.p2_ready = true;
            state.p2_join_pulse_t = JOIN_PULSE_DURATION;
        }
        profile_data::PlayerSide::P2 => {
            state.p1_joined = true;
            state.p1_ready = true;
            state.p1_join_pulse_t = JOIN_PULSE_DURATION;
            state.p2_joined = true;
            state.p2_ready = false;
            state.p2_join_pulse_t = 0.0;
        }
    }

    state.p1_preview_noteskin = preview_noteskin_for_choice(
        &mut state.noteskin_cache,
        &state.choices,
        state.p1_selected_index,
    );
    state.p2_preview_noteskin = preview_noteskin_for_choice(
        &mut state.noteskin_cache,
        &state.choices,
        state.p2_selected_index,
    );
}

pub fn update(state: &mut State, dt: f32) {
    const BPM: f32 = 120.0;
    let dt = dt.max(0.0);
    state.preview_time += dt;
    state.preview_beat += dt * (BPM / 60.0);

    state.p1_join_pulse_t = (state.p1_join_pulse_t + dt).min(JOIN_PULSE_DURATION);
    state.p2_join_pulse_t = (state.p2_join_pulse_t + dt).min(JOIN_PULSE_DURATION);
    state.p1_shake_t = (state.p1_shake_t + dt).min(SHAKE_DUR);
    state.p2_shake_t = (state.p2_shake_t + dt).min(SHAKE_DUR);

    // Decay the name-scroller offset toward 0 (frame-rate independent).
    let scroll_decay = (-dt / SCROLL_TWEEN_TAU).exp();
    state.p1_scroll_anim *= scroll_decay;
    state.p2_scroll_anim *= scroll_decay;
    if state.p1_scroll_anim.abs() < SCROLL_SNAP_EPS {
        state.p1_scroll_anim = 0.0;
    }
    if state.p2_scroll_anim.abs() < SCROLL_SNAP_EPS {
        state.p2_scroll_anim = 0.0;
    }
}

#[must_use]
pub fn in_transition() -> (Vec<Actor>, f32) {
    super::transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

#[must_use]
pub fn out_transition() -> (Vec<Actor>, f32) {
    super::transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

#[inline(always)]
const fn both_ready(state: &State) -> bool {
    (state.p1_ready || !state.p1_joined) && (state.p2_ready || !state.p2_joined)
}

#[inline(always)]
fn active_choices(state: &State) -> (profile_data::ActiveProfile, profile_data::ActiveProfile) {
    let p1 = if state.p1_joined {
        state
            .choices
            .get(state.p1_selected_index)
            .map_or(profile_data::ActiveProfile::Guest, |c| c.kind.clone())
    } else {
        profile_data::ActiveProfile::Guest
    };
    let p2 = if state.p2_joined {
        state
            .choices
            .get(state.p2_selected_index)
            .map_or(profile_data::ActiveProfile::Guest, |c| c.kind.clone())
    } else {
        profile_data::ActiveProfile::Guest
    };
    (p1, p2)
}

const fn trigger_invalid_choice(state: &mut State, is_p1: bool) -> ThemeEffect {
    if is_p1 {
        state.p1_shake_t = 0.0;
        // Simply Love `InvalidChoiceMessageCommand` starts with `finishtweening()`,
        // so ensure any join pulse is fully settled before we shake.
        state.p1_join_pulse_t = JOIN_PULSE_DURATION;
    } else {
        state.p2_shake_t = 0.0;
        state.p2_join_pulse_t = JOIN_PULSE_DURATION;
    }
    crate::effects::sfx("assets/sounds/boom.ogg")
}

fn shift_choice(state: &mut State, side: profile_data::PlayerSide, dir: i32) -> bool {
    let (joined, ready, selected_index, preview_slot) = match side {
        profile_data::PlayerSide::P1 => (
            state.p1_joined,
            state.p1_ready,
            &mut state.p1_selected_index,
            &mut state.p1_preview_noteskin,
        ),
        profile_data::PlayerSide::P2 => (
            state.p2_joined,
            state.p2_ready,
            &mut state.p2_selected_index,
            &mut state.p2_preview_noteskin,
        ),
    };
    if !joined || ready {
        return false;
    }
    let old_index = *selected_index;
    if dir < 0 {
        if *selected_index > 0 {
            *selected_index -= 1;
        }
    } else if *selected_index + 1 < state.choices.len() {
        *selected_index += 1;
    }
    if *selected_index == old_index {
        return false;
    }
    let new_index = *selected_index;
    *preview_slot =
        preview_noteskin_for_choice(&mut state.noteskin_cache, &state.choices, new_index);
    // Seed the scroller offset so the rows visually start at the old position and
    // tween toward the new selection. Accumulates across rapid presses.
    let delta = new_index as f32 - old_index as f32;
    match side {
        profile_data::PlayerSide::P1 => state.p1_scroll_anim += delta,
        profile_data::PlayerSide::P2 => state.p2_scroll_anim += delta,
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputOutcome {
    Continue,
    Selected,
    Cancelled(Screen),
}

const MAX_INPUT_EFFECTS: usize = 2;

fn handle_cancel(
    state: &mut State,
    side: profile_data::PlayerSide,
    effects: &mut Vec<ThemeEffect>,
) -> InputOutcome {
    match side {
        profile_data::PlayerSide::P1 => {
            if state.p1_joined && state.p1_ready {
                state.p1_ready = false;
                effects.push(crate::effects::sfx("assets/sounds/unjoin.ogg"));
                return InputOutcome::Continue;
            }
            if state.p1_joined {
                state.p1_joined = false;
                state.p1_ready = false;
                effects.push(crate::effects::sfx("assets/sounds/unjoin.ogg"));
                return InputOutcome::Continue;
            }
            if state.p2_joined {
                return InputOutcome::Continue;
            }
            state.exit_anim = true;
            let _ = exit_anim_t(true);
            InputOutcome::Cancelled(if state.fast_switch {
                Screen::SelectMusic
            } else {
                Screen::Menu
            })
        }
        profile_data::PlayerSide::P2 => {
            if state.p2_joined && state.p2_ready {
                state.p2_ready = false;
                effects.push(crate::effects::sfx("assets/sounds/unjoin.ogg"));
                return InputOutcome::Continue;
            }
            if state.p2_joined {
                state.p2_joined = false;
                state.p2_ready = false;
                effects.push(crate::effects::sfx("assets/sounds/unjoin.ogg"));
                return InputOutcome::Continue;
            }
            if state.p1_joined {
                return InputOutcome::Continue;
            }
            state.exit_anim = true;
            let _ = exit_anim_t(true);
            InputOutcome::Cancelled(if state.fast_switch {
                Screen::SelectMusic
            } else {
                Screen::Menu
            })
        }
    }
}

pub fn handle_input(
    state: &mut State,
    ev: &InputEvent,
    effects: &mut Vec<ThemeEffect>,
) -> InputOutcome {
    let start_len = effects.len();
    let outcome = handle_input_impl(state, ev, effects);
    debug_assert!(effects.len() - start_len <= MAX_INPUT_EFFECTS);
    outcome
}

fn handle_input_impl(
    state: &mut State,
    ev: &InputEvent,
    effects: &mut Vec<ThemeEffect>,
) -> InputOutcome {
    let chord_side = if state.three_key_navigation {
        state.menu_lr_chord.update(ev)
    } else {
        None
    };
    if !ev.pressed {
        if let Some(side) = screen_input::menu_lr_side(ev.action) {
            state.menu_lr_undo[profile_data::player_side_index(side)] = 0;
        }
        return InputOutcome::Continue;
    }
    if state.exit_anim {
        return InputOutcome::Continue;
    }
    if let Some(side) = chord_side {
        let undo = state.menu_lr_undo[profile_data::player_side_index(side)];
        state.menu_lr_undo[profile_data::player_side_index(side)] = 0;
        if undo != 0 {
            let _ = shift_choice(state, side, i32::from(undo));
        }
        return handle_cancel(state, side, effects);
    }

    match ev.action {
        VirtualAction::p1_up
        | VirtualAction::p1_menu_up
        | VirtualAction::p1_left
        | VirtualAction::p1_menu_left => {
            let shifted = shift_choice(state, profile_data::PlayerSide::P1, -1);
            state.menu_lr_undo[profile_data::player_side_index(profile_data::PlayerSide::P1)] =
                if shifted { 1 } else { 0 };
            if shifted {
                effects.push(crate::effects::sfx("assets/sounds/expand.ogg"));
            }
            InputOutcome::Continue
        }
        VirtualAction::p1_down
        | VirtualAction::p1_menu_down
        | VirtualAction::p1_right
        | VirtualAction::p1_menu_right => {
            let shifted = shift_choice(state, profile_data::PlayerSide::P1, 1);
            state.menu_lr_undo[profile_data::player_side_index(profile_data::PlayerSide::P1)] =
                if shifted { -1 } else { 0 };
            if shifted {
                effects.push(crate::effects::sfx("assets/sounds/expand.ogg"));
            }
            InputOutcome::Continue
        }
        VirtualAction::p1_start => {
            if !state.p1_joined {
                state.p1_joined = true;
                state.p1_ready = false;
                state.p1_join_pulse_t = 0.0;
                state.p1_preview_noteskin = preview_noteskin_for_choice(
                    &mut state.noteskin_cache,
                    &state.choices,
                    state.p1_selected_index,
                );
                effects.push(crate::effects::sfx("assets/sounds/start.ogg"));
                return InputOutcome::Continue;
            }

            if state.p1_ready {
                return InputOutcome::Continue;
            }

            if state.p2_joined
                && state.p2_ready
                && state.choices.get(state.p1_selected_index).is_some_and(|c| {
                    !matches!(&c.kind, profile_data::ActiveProfile::Guest)
                        && state
                            .choices
                            .get(state.p2_selected_index)
                            .is_some_and(|o| o.kind == c.kind)
                })
            {
                effects.push(trigger_invalid_choice(state, true));
                return InputOutcome::Continue;
            }

            state.p1_ready = true;
            if both_ready(state) {
                state.exit_anim = true;
                let _ = exit_anim_t(true);
                let (p1, p2) = active_choices(state);
                effects.extend([
                    crate::effects::sfx("assets/sounds/start.ogg"),
                    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
                        crate::SimplyLoveProfileRequest::Select {
                            p1,
                            p2,
                            p1_joined: state.p1_joined,
                            p2_joined: state.p2_joined,
                            fast_switch: state.fast_switch,
                        },
                    )),
                ]);
                return InputOutcome::Selected;
            }
            InputOutcome::Continue
        }
        VirtualAction::p1_back | VirtualAction::p1_select => {
            handle_cancel(state, profile_data::PlayerSide::P1, effects)
        }
        VirtualAction::p2_up
        | VirtualAction::p2_menu_up
        | VirtualAction::p2_left
        | VirtualAction::p2_menu_left => {
            let shifted = shift_choice(state, profile_data::PlayerSide::P2, -1);
            state.menu_lr_undo[profile_data::player_side_index(profile_data::PlayerSide::P2)] =
                if shifted { 1 } else { 0 };
            if shifted {
                effects.push(crate::effects::sfx("assets/sounds/expand.ogg"));
            }
            InputOutcome::Continue
        }
        VirtualAction::p2_down
        | VirtualAction::p2_menu_down
        | VirtualAction::p2_right
        | VirtualAction::p2_menu_right => {
            let shifted = shift_choice(state, profile_data::PlayerSide::P2, 1);
            state.menu_lr_undo[profile_data::player_side_index(profile_data::PlayerSide::P2)] =
                if shifted { -1 } else { 0 };
            if shifted {
                effects.push(crate::effects::sfx("assets/sounds/expand.ogg"));
            }
            InputOutcome::Continue
        }
        VirtualAction::p2_start => {
            if !state.p2_joined {
                state.p2_joined = true;
                state.p2_ready = false;
                state.p2_join_pulse_t = 0.0;
                state.p2_preview_noteskin = preview_noteskin_for_choice(
                    &mut state.noteskin_cache,
                    &state.choices,
                    state.p2_selected_index,
                );
                effects.push(crate::effects::sfx("assets/sounds/start.ogg"));
                return InputOutcome::Continue;
            }

            if state.p2_ready {
                return InputOutcome::Continue;
            }

            if state.p1_joined
                && state.p1_ready
                && state.choices.get(state.p2_selected_index).is_some_and(|c| {
                    !matches!(&c.kind, profile_data::ActiveProfile::Guest)
                        && state
                            .choices
                            .get(state.p1_selected_index)
                            .is_some_and(|o| o.kind == c.kind)
                })
            {
                effects.push(trigger_invalid_choice(state, false));
                return InputOutcome::Continue;
            }

            state.p2_ready = true;
            if both_ready(state) {
                state.exit_anim = true;
                let _ = exit_anim_t(true);
                let (p1, p2) = active_choices(state);
                effects.extend([
                    crate::effects::sfx("assets/sounds/start.ogg"),
                    ThemeEffect::Runtime(crate::SimplyLoveRuntimeRequest::Profile(
                        crate::SimplyLoveProfileRequest::Select {
                            p1,
                            p2,
                            p1_joined: state.p1_joined,
                            p2_joined: state.p2_joined,
                            fast_switch: state.fast_switch,
                        },
                    )),
                ]);
                return InputOutcome::Selected;
            }
            InputOutcome::Continue
        }
        VirtualAction::p2_back | VirtualAction::p2_select => {
            handle_cancel(state, profile_data::PlayerSide::P2, effects)
        }
        _ => InputOutcome::Continue,
    }
}

#[inline(always)]
fn exit_anim_t(exiting: bool) -> f32 {
    static STEPS: std::sync::OnceLock<Vec<deadlib_present::anim::Step>> =
        std::sync::OnceLock::new();
    super::transitions::linear_elapsed(
        exiting,
        EXIT_ANIM_DURATION,
        &STEPS,
        0x5345_4C50_5245_5849_u64, // "SELPREXI"
    )
}

#[inline(always)]
fn exit_zoom(exit_t: f32) -> f32 {
    let p = deadlib_present::anim::bouncebegin_p(
        (exit_t / PLAYERFRAME_EXIT_ZOOM_OUT_DURATION).clamp(0.0, 1.0),
    );
    (1.0 - p).max(0.0)
}

#[inline(always)]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

#[inline(always)]
fn join_pulse_zoom(join_t: f32) -> f32 {
    if join_t >= JOIN_PULSE_DURATION {
        return 1.0;
    }
    let p = deadlib_present::anim::bounceend_p((join_t / JOIN_PULSE_DURATION).clamp(0.0, 1.0));
    lerp(JOIN_PULSE_ZOOM_IN, 1.0, p).max(0.0)
}

#[inline(always)]
fn shake_x(shake_t: f32) -> f32 {
    if shake_t >= SHAKE_DUR {
        return 0.0;
    }
    let p = deadlib_present::anim::bounceend_p((shake_t / SHAKE_STEP_DUR).clamp(0.0, 1.0));
    if shake_t < SHAKE_STEP_DUR {
        lerp(0.0, 5.0, p)
    } else if shake_t < SHAKE_STEP_DUR * 2.0 {
        let t = (shake_t - SHAKE_STEP_DUR).clamp(0.0, SHAKE_STEP_DUR);
        let p = deadlib_present::anim::bounceend_p((t / SHAKE_STEP_DUR).clamp(0.0, 1.0));
        lerp(5.0, -5.0, p)
    } else {
        let t = SHAKE_STEP_DUR
            .mul_add(-2.0, shake_t)
            .clamp(0.0, SHAKE_STEP_DUR);
        let p = deadlib_present::anim::bounceend_p((t / SHAKE_STEP_DUR).clamp(0.0, 1.0));
        lerp(-5.0, 0.0, p)
    }
}

#[inline(always)]
fn scale_about(v: f32, pivot: f32, zoom: f32) -> f32 {
    (v - pivot).mul_add(zoom, pivot)
}

fn apply_zoom_to_actor(actor: &mut Actor, pivot: [f32; 2], zoom: f32) {
    match actor {
        Actor::Sprite {
            offset,
            size,
            scale,
            ..
        } => {
            offset[0] = scale_about(offset[0], pivot[0], zoom);
            offset[1] = scale_about(offset[1], pivot[1], zoom);
            for s in size.iter_mut() {
                if let actors::SizeSpec::Px(v) = s {
                    *v *= zoom;
                }
            }
            scale[0] *= zoom;
            scale[1] *= zoom;
        }
        Actor::Mesh {
            offset,
            size,
            vertices,
            ..
        } => {
            offset[0] = scale_about(offset[0], pivot[0], zoom);
            offset[1] = scale_about(offset[1], pivot[1], zoom);
            for s in size.iter_mut() {
                if let actors::SizeSpec::Px(v) = s {
                    *v *= zoom;
                }
            }
            let mut out: Vec<deadlib_render_core::MeshVertex> = Vec::with_capacity(vertices.len());
            for v in vertices.iter() {
                out.push(deadlib_render_core::MeshVertex {
                    pos: [v.pos[0] * zoom, v.pos[1] * zoom],
                    color: v.color,
                });
            }
            *vertices = std::sync::Arc::from(out);
        }
        Actor::ReusableMesh {
            offset,
            size,
            vertices,
            ..
        } => {
            offset[0] = scale_about(offset[0], pivot[0], zoom);
            offset[1] = scale_about(offset[1], pivot[1], zoom);
            for s in size.iter_mut() {
                if let actors::SizeSpec::Px(v) = s {
                    *v *= zoom;
                }
            }
            let mut out = Vec::with_capacity(vertices.len());
            for v in vertices.iter() {
                out.push(deadlib_render_core::MeshVertex {
                    pos: [v.pos[0] * zoom, v.pos[1] * zoom],
                    color: v.color,
                });
            }
            *vertices = std::sync::Arc::new(out);
        }
        Actor::TexturedMesh {
            offset,
            size,
            vertices,
            ..
        } => {
            offset[0] = scale_about(offset[0], pivot[0], zoom);
            offset[1] = scale_about(offset[1], pivot[1], zoom);
            for s in size.iter_mut() {
                if let actors::SizeSpec::Px(v) = s {
                    *v *= zoom;
                }
            }
            let mut out: Vec<deadlib_render_core::TexturedMeshVertex> =
                Vec::with_capacity(vertices.len());
            for v in vertices.iter() {
                out.push(deadlib_render_core::TexturedMeshVertex {
                    pos: [v.pos[0] * zoom, v.pos[1] * zoom, v.pos[2] * zoom],
                    uv: v.uv,
                    tex_matrix_scale: v.tex_matrix_scale,
                    color: v.color,
                });
            }
            *vertices = std::sync::Arc::from(out);
        }
        Actor::ReusableTexturedMesh {
            offset,
            size,
            local_transform,
            ..
        } => {
            offset[0] = scale_about(offset[0], pivot[0], zoom);
            offset[1] = scale_about(offset[1], pivot[1], zoom);
            for s in size.iter_mut() {
                if let actors::SizeSpec::Px(v) = s {
                    *v *= zoom;
                }
            }
            *local_transform *= glam::Mat4::from_scale(glam::Vec3::splat(zoom));
        }
        Actor::Text {
            offset,
            scale,
            max_width,
            max_height,
            max_w_pre_zoom,
            max_h_pre_zoom,
            clip,
            ..
        } => {
            offset[0] = scale_about(offset[0], pivot[0], zoom);
            offset[1] = scale_about(offset[1], pivot[1], zoom);
            scale[0] *= zoom;
            scale[1] *= zoom;

            if let Some(r) = clip.as_mut() {
                r[0] = scale_about(r[0], pivot[0], zoom);
                r[1] = scale_about(r[1], pivot[1], zoom);
                r[2] *= zoom;
                r[3] *= zoom;
            }

            if !*max_w_pre_zoom && let Some(w) = max_width {
                *max_width = Some(*w * zoom);
            }
            if !*max_h_pre_zoom && let Some(h) = max_height {
                *max_height = Some(*h * zoom);
            }
        }
        Actor::Frame {
            offset,
            size,
            children,
            ..
        } => {
            offset[0] = scale_about(offset[0], pivot[0], zoom);
            offset[1] = scale_about(offset[1], pivot[1], zoom);
            for s in size.iter_mut() {
                if let actors::SizeSpec::Px(v) = s {
                    *v *= zoom;
                }
            }
            for child in children {
                apply_zoom_to_actor(child, pivot, zoom);
            }
        }
        Actor::SharedFrame {
            offset,
            size,
            children,
            ..
        } => {
            offset[0] = scale_about(offset[0], pivot[0], zoom);
            offset[1] = scale_about(offset[1], pivot[1], zoom);
            for s in size.iter_mut() {
                if let actors::SizeSpec::Px(v) = s {
                    *v *= zoom;
                }
            }
            if let Some(children) = std::sync::Arc::get_mut(children) {
                for child in children {
                    apply_zoom_to_actor(child, pivot, zoom);
                }
            }
        }
        Actor::SharedTransform { transform, .. } => {
            let world_pivot = glam::Vec3::new(
                pivot[0] - screen_center_x(),
                screen_center_y() - pivot[1],
                0.0,
            );
            *transform = glam::Mat4::from_translation(world_pivot)
                * glam::Mat4::from_scale(glam::Vec3::new(zoom, zoom, 1.0))
                * glam::Mat4::from_translation(-world_pivot)
                * *transform;
        }
        Actor::RetainedFrame { offset, size, .. } => {
            offset[0] = scale_about(offset[0], pivot[0], zoom);
            offset[1] = scale_about(offset[1], pivot[1], zoom);
            for s in size.iter_mut() {
                if let actors::SizeSpec::Px(v) = s {
                    *v *= zoom;
                }
            }
        }
        Actor::Camera { children, .. } => {
            for child in children {
                apply_zoom_to_actor(child, pivot, zoom);
            }
        }
        Actor::RenderTarget { .. } | Actor::CameraPush { .. } | Actor::CameraPop => {}
        Actor::Shadow { len, child, .. } => {
            len[0] *= zoom;
            len[1] *= zoom;
            apply_zoom_to_actor(child, pivot, zoom);
        }
    }
}

fn apply_offset_to_actor(actor: &mut Actor, dx: f32, dy: f32) {
    match actor {
        Actor::Sprite { offset, .. } => {
            offset[0] += dx;
            offset[1] += dy;
        }
        Actor::Mesh { offset, .. } | Actor::ReusableMesh { offset, .. } => {
            offset[0] += dx;
            offset[1] += dy;
        }
        Actor::TexturedMesh { offset, .. } | Actor::ReusableTexturedMesh { offset, .. } => {
            offset[0] += dx;
            offset[1] += dy;
        }
        Actor::Text { offset, clip, .. } => {
            offset[0] += dx;
            offset[1] += dy;
            if let Some(r) = clip.as_mut() {
                r[0] += dx;
                r[1] += dy;
            }
        }
        // Frame children are already in the frame's coordinate space; shifting the
        // frame moves the whole subtree in compose.
        Actor::Frame { offset, .. } => {
            offset[0] += dx;
            offset[1] += dy;
        }
        Actor::SharedFrame { offset, .. } | Actor::RetainedFrame { offset, .. } => {
            offset[0] += dx;
            offset[1] += dy;
        }
        Actor::SharedTransform { transform, .. } => {
            *transform = glam::Mat4::from_translation(glam::Vec3::new(dx, -dy, 0.0)) * *transform;
        }
        Actor::Camera { children, .. } => {
            for child in children {
                apply_offset_to_actor(child, dx, dy);
            }
        }
        Actor::RenderTarget { .. } | Actor::CameraPush { .. } | Actor::CameraPop => {}
        Actor::Shadow { child, .. } => apply_offset_to_actor(child, dx, dy),
    }
}

fn apply_z_offset(actor: &mut Actor, dz: i16) {
    match actor {
        Actor::Sprite { z, .. }
        | Actor::Text { z, .. }
        | Actor::Mesh { z, .. }
        | Actor::ReusableMesh { z, .. }
        | Actor::TexturedMesh { z, .. }
        | Actor::ReusableTexturedMesh { z, .. }
        | Actor::Frame { z, .. }
        | Actor::SharedFrame { z, .. }
        | Actor::SharedTransform { z, .. }
        | Actor::RetainedFrame { z, .. } => *z = z.saturating_add(dz),
        Actor::Camera { .. }
        | Actor::RenderTarget { .. }
        | Actor::CameraPush { .. }
        | Actor::CameraPop
        | Actor::Shadow { .. } => {}
    }
    match actor {
        Actor::Frame { children, .. } | Actor::Camera { children, .. } => {
            for child in children {
                apply_z_offset(child, dz);
            }
        }
        Actor::SharedFrame { children, .. } | Actor::SharedTransform { children, .. } => {
            if let Some(children) = std::sync::Arc::get_mut(children) {
                for child in children {
                    apply_z_offset(child, dz);
                }
            }
        }
        Actor::Shadow { child, .. } => apply_z_offset(child, dz),
        Actor::RetainedFrame { .. } | Actor::CameraPush { .. } | Actor::CameraPop => {}
        _ => {}
    }
}

fn apply_clip_rect_to_actor(actor: &mut Actor, rect: [f32; 4]) {
    match actor {
        Actor::Text { clip, .. } => *clip = Some(rect),
        Actor::Frame { children, .. } => {
            for child in children {
                apply_clip_rect_to_actor(child, rect);
            }
        }
        Actor::SharedFrame { children, .. } | Actor::SharedTransform { children, .. } => {
            if let Some(children) = std::sync::Arc::get_mut(children) {
                for child in children {
                    apply_clip_rect_to_actor(child, rect);
                }
            }
        }
        Actor::Camera { children, .. } => {
            for child in children {
                apply_clip_rect_to_actor(child, rect);
            }
        }
        Actor::Shadow { child, .. } => apply_clip_rect_to_actor(child, rect),
        Actor::Sprite { .. }
        | Actor::Mesh { .. }
        | Actor::ReusableMesh { .. }
        | Actor::TexturedMesh { .. }
        | Actor::ReusableTexturedMesh { .. }
        | Actor::RetainedFrame { .. }
        | Actor::RenderTarget { .. }
        | Actor::CameraPush { .. }
        | Actor::CameraPop => {}
    }
}

#[inline(always)]
fn box_inner_alpha() -> f32 {
    use deadlib_present::{anim, runtime};
    static STEPS: std::sync::OnceLock<Vec<anim::Step>> = std::sync::OnceLock::new();

    let steps = STEPS.get_or_init(|| {
        vec![
            anim::sleep(FRAME_IN_CROP_DUR),
            anim::linear(OVERLAY_IN_DUR).x(1.0).build(),
        ]
    });

    let mut init = anim::TweenState::default();
    init.x = 0.0;
    const SITE_BASE: u64 = runtime::site_base(file!(), line!(), column!());
    let sid = runtime::site_id(SITE_BASE, 0x5345_4C50_524F_4649_u64); // "SELPROFI"
    runtime::materialize(sid, init, steps).x.clamp(0.0, 1.0)
}

fn push_join_prompt(
    out: &mut Vec<Actor>,
    cx: f32,
    cy: f32,
    frame_h: f32,
    border_rgba: [f32; 4],
    inner_alpha: f32,
    time: f32,
    text: std::sync::Arc<str>,
) {
    // ITGmania diffuse_shift: period=1, color1=white, color2=gray.
    // f = sin((t + 0.25) * 2π) / 2 + 0.5
    let t = time.rem_euclid(1.0);
    let f = ((t + 0.25) * std::f32::consts::PI * 2.0)
        .sin()
        .mul_add(0.5, 0.5);
    let shade = 0.5f32.mul_add(f, 0.5);
    let salt = u64::from(cx.to_bits());

    out.push(act!(quad:
        tweensalt(salt):
        align(0.5, 0.5):
        xy(cx, cy):
        zoomto(FRAME_W_JOIN + FRAME_BORDER, frame_h + FRAME_BORDER):
        diffuse(border_rgba[0], border_rgba[1], border_rgba[2], border_rgba[3]):
        cropbottom(1.0):
        smooth(FRAME_IN_CROP_DUR): cropbottom(0.0):
        z(100)
    ));
    out.push(act!(quad:
        tweensalt(salt):
        align(0.5, 0.5):
        xy(cx, cy):
        zoomto(FRAME_W_JOIN, frame_h):
        diffuse(0.0, 0.0, 0.0, 1.0):
        cropbottom(1.0):
        smooth(FRAME_IN_CROP_DUR): cropbottom(0.0):
        z(101)
    ));
    out.push(act!(text:
        align(0.5, 0.5):
        xy(cx, cy):
        font("miso"):
        zoomtoheight(18.0):
        maxwidth(FRAME_W_JOIN - 20.0):
        settext(text):
        diffuse(shade, shade, shade, inner_alpha):
        z(103)
    ));
}

#[allow(clippy::too_many_arguments)]
fn push_scroller_frame(
    out: &mut Vec<Actor>,
    _asset_manager: &AssetManager,
    choices: &[Choice],
    selected_index: usize,
    scroll_anim: f32,
    preview_noteskin: Option<&Noteskin>,
    preview_col: usize,
    preview_time: f32,
    preview_beat: f32,
    frame_cx: f32,
    frame_cy: f32,
    frame_y0: f32,
    frame_h: f32,
    color_index: i32,
    inner_alpha: f32,
    border_rgba: [f32; 4],
    col_overlay: [f32; 4],
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
    mut model_mesh_cache: Option<&mut ModelMeshCache>,
    retain_static_payloads: bool,
) {
    // Simply Love parity:
    // - Frame bg uses PlayerColor(P1) => SL.Colors[ActiveColorIndex]
    // - Top edge is LightenColor(c) (rgb * 1.25), producing a subtle vertical gradient
    // - Scroller highlight + info pane use semi-transparent black overlays (alpha 0.5)
    let col_frame = color::simply_love_rgba(color_index);
    let col_frame_top = color::lighten_rgba(col_frame);
    let salt = u64::from(frame_cx.to_bits());

    // Frame border.
    out.push(act!(quad:
        tweensalt(salt):
        align(0.5, 0.5):
        xy(frame_cx, frame_cy):
        zoomto(FRAME_W_SCROLLER + FRAME_BORDER, frame_h + FRAME_BORDER):
        diffuse(border_rgba[0], border_rgba[1], border_rgba[2], border_rgba[3]):
        cropbottom(1.0):
        smooth(FRAME_IN_CROP_DUR): cropbottom(0.0):
        z(100)
    ));
    // Base fill.
    out.push(act!(quad:
        tweensalt(salt):
        align(0.5, 0.5):
        xy(frame_cx, frame_cy):
        zoomto(FRAME_W_SCROLLER, frame_h):
        diffuse(col_frame[0], col_frame[1], col_frame[2], col_frame[3]):
        cropbottom(1.0):
        smooth(FRAME_IN_CROP_DUR): cropbottom(0.0):
        z(101)
    ));
    // Top-edge lighten gradient (approx for diffusetopedge()).
    out.push(act!(quad:
        tweensalt(salt):
        align(0.5, 0.5):
        xy(frame_cx, frame_cy):
        zoomto(FRAME_W_SCROLLER, frame_h):
        diffuse(col_frame_top[0], col_frame_top[1], col_frame_top[2], col_frame_top[3]):
        fadebottom(1.0):
        cropbottom(1.0):
        smooth(FRAME_IN_CROP_DUR): cropbottom(0.0):
        z(101)
    ));

    // Info pane background (semi-transparent black overlay).
    let info_x0 = frame_cx + INFO_X0_OFF;
    let info_text_x = INFO_PAD.mul_add(1.25, info_x0);
    let info_max_w = INFO_PAD.mul_add(-2.5, INFO_W);

    out.push(act!(quad:
        tweensalt(salt):
        align(0.0, 0.0):
        xy(info_x0, frame_y0):
        zoomto(INFO_W, frame_h):
        diffuse(0.0, 0.0, 0.0, 0.0):
        sleep(OVERLAY_IN_DELAY):
        linear(OVERLAY_IN_DUR): diffusealpha(col_overlay[3]):
        z(102)
    ));

    // Scroller highlight bar.
    let scroller_cx = frame_cx + SCROLLER_CX_OFF;
    out.push(act!(quad:
        tweensalt(salt):
        align(0.5, 0.5):
        xy(scroller_cx, frame_cy):
        zoomto(SCROLLER_W, ROW_H):
        diffuse(0.0, 0.0, 0.0, 0.0):
        sleep(OVERLAY_IN_DELAY):
        linear(OVERLAY_IN_DUR): diffusealpha(col_overlay[3]):
        z(102)
    ));

    // Scroller rows.
    let scroller_clip = [
        SCROLLER_W.mul_add(-0.5, scroller_cx),
        frame_y0,
        SCROLLER_W,
        frame_h,
    ];
    let rows_half = ROWS_VISIBLE / 2;
    // Render one extra row on each side so a row sliding in during the scroll tween
    // does not pop into existence at the clipped edge.
    for d in -(rows_half + 1)..=(rows_half + 1) {
        let idx_i = selected_index as i32 + d;
        if idx_i < 0 || idx_i >= choices.len() as i32 {
            continue;
        }
        let choice = &choices[idx_i as usize];
        let y = (d as f32 + scroll_anim).mul_add(ROW_H, frame_cy);

        let mut row = act!(text:
            align(0.5, 0.5):
            xy(scroller_cx, y):
            font("miso"):
            maxwidth(SCROLLER_TEXT_PAD_X.mul_add(-2.0, SCROLLER_W)):
            zoom(1.0):
            settext(choice.display_name.clone()):
            diffuse(1.0, 1.0, 1.0, inner_alpha):
            shadowlength(0.5):
            z(103):
            horizalign(center)
        );
        apply_clip_rect_to_actor(&mut row, scroller_clip);
        out.push(row);
    }

    let selected = choices.get(selected_index);
    let selected_is_local =
        selected.is_some_and(|c| matches!(&c.kind, profile_data::ActiveProfile::Local { .. }));

    // Avatar slot (SL-style): show profile.png if present, else heart + text.
    let avatar_dim = INFO_PAD.mul_add(-2.25, INFO_W);
    let avatar_x = info_x0 + AVATAR_X_OFF;
    let avatar_y = frame_cy + AVATAR_Y_OFF;

    if let Some(choice) = selected {
        let is_guest = matches!(&choice.kind, profile_data::ActiveProfile::Guest);
        let show_fallback = is_guest || choice.avatar_key.is_none();
        if show_fallback {
            let bg = color::rgba_hex(AVATAR_BG_HEX);
            out.push(act!(quad:
                align(0.0, 0.0):
                xy(avatar_x, avatar_y):
                zoomto(avatar_dim, avatar_dim):
                diffuse(bg[0], bg[1], bg[2], bg[3] * inner_alpha):
                z(103)
            ));
            let texture = visual_policy.assets.select_color;
            let zoom = AVATAR_HEART_ZOOM
                * (566.0 / visual_policy.assets.select_color_size[1].max(1) as f32);
            let actor = if retain_static_payloads {
                act!(sprite_static(texture):
                    align(0.0, 0.0):
                    xy(avatar_x + AVATAR_HEART_X, avatar_y + AVATAR_HEART_Y):
                    zoom(zoom):
                    diffuse(1.0, 1.0, 1.0, 0.9 * inner_alpha):
                    z(104)
                )
            } else {
                act!(sprite(texture):
                    align(0.0, 0.0):
                    xy(avatar_x + AVATAR_HEART_X, avatar_y + AVATAR_HEART_Y):
                    zoom(zoom):
                    diffuse(1.0, 1.0, 1.0, 0.9 * inner_alpha):
                    z(104)
                )
            };
            out.push(actor);

            let label = if is_guest {
                tr("SelectProfile", "GuestLabel")
            } else {
                tr("SelectProfile", "NoAvatar")
            };
            out.push(act!(text:
                align(0.5, 0.0):
                xy(avatar_x + avatar_dim * 0.5, avatar_y + AVATAR_TEXT_Y):
                font("miso"):
                maxwidth(avatar_dim - 8.0):
                zoom(AVATAR_LABEL_ZOOM):
                settext(label):
                diffuse(1.0, 1.0, 1.0, 0.9 * inner_alpha):
                z(105):
                horizalign(center)
            ));
        } else if let Some(key) = &choice.avatar_key {
            out.push(act!(sprite(key.clone()):
                align(0.0, 0.0):
                xy(avatar_x, avatar_y):
                zoomto(avatar_dim, avatar_dim):
                diffusealpha(inner_alpha):
                z(104)
            ));
        }
    }

    if selected_is_local {
        out.push(act!(text:
            align(0.0, 0.0):
            xy(info_text_x, frame_cy):
            font("miso"):
            zoom(TOTAL_SONGS_ZOOM):
            maxwidth(info_max_w):
            settext(selected.unwrap().total_songs.clone()):
            diffuse(1.0, 1.0, 1.0, inner_alpha):
            z(103)
        ));
    }

    // Thin white line separating stats from mods (SL-style).
    out.push(act!(quad:
        align(0.0, 0.0):
        xy(INFO_PAD.mul_add(1.25, info_x0), frame_cy + INFO_LINE_Y_OFF):
        zoomto(info_max_w, 1.0):
        diffuse(1.0, 1.0, 1.0, 0.5 * inner_alpha):
        z(103)
    ));

    // NoteSkin + JudgmentGraphic previews (SL-style placement).
    if selected_is_local {
        let selected_mods = selected
            .map(|choice| Arc::clone(&choice.recent_mods))
            .unwrap_or_default();
        let preview_y = frame_cy + PREVIEW_Y_OFF;

        if let Some(ns) = preview_noteskin {
            let note_idx = preview_col * NUM_QUANTIZATIONS + Quantization::Q4th as usize;
            const TARGET_ARROW_PIXEL_SIZE: f32 = 40.0;
            const PREVIEW_SCALE: f32 = 0.4;
            let target_height = TARGET_ARROW_PIXEL_SIZE * PREVIEW_SCALE;
            let ns_x = INFO_W.mul_add(0.13, info_x0);
            let ns_y = preview_y - 10.0;
            let center = [ns_x, ns_y];
            let note_uv_phase = ns.tap_note_uv_phase(preview_time, preview_beat, 0.0);
            if let Some(note_slots) = ns.note_layers.get(note_idx) {
                let primary_h = note_slots
                    .first()
                    .map(|slot| slot.logical_size()[1].max(1.0))
                    .unwrap_or(1.0);
                let note_scale = if primary_h > f32::EPSILON {
                    target_height / primary_h
                } else {
                    PREVIEW_SCALE
                };
                for (layer_idx, note_slot) in note_slots.iter().enumerate() {
                    let draw = note_slot.model_draw_at(preview_time, preview_beat);
                    if !draw.visible {
                        continue;
                    }
                    let frame = note_slot.frame_index_from_phase(note_uv_phase);
                    let uv_elapsed = if note_slot.model.is_some() {
                        note_uv_phase
                    } else {
                        preview_time
                    };
                    let uv = note_slot.uv_for_frame_at(frame, uv_elapsed);
                    let slot_size = note_slot.logical_size();
                    let base_size = [slot_size[0] * note_scale, slot_size[1] * note_scale];
                    let rot_rad = (-note_slot.def.rotation_deg as f32).to_radians();
                    let (sin_r, cos_r) = rot_rad.sin_cos();
                    let ox = draw.pos[0] * note_scale;
                    let oy = draw.pos[1] * note_scale;
                    let layer_center = [
                        center[0] + ox * cos_r - oy * sin_r,
                        center[1] + ox * sin_r + oy * cos_r,
                    ];
                    let size = [
                        base_size[0] * draw.zoom[0].max(0.0),
                        base_size[1] * draw.zoom[1].max(0.0),
                    ];
                    if size[0] <= f32::EPSILON || size[1] <= f32::EPSILON {
                        continue;
                    }
                    let color = [
                        draw.tint[0],
                        draw.tint[1],
                        draw.tint[2],
                        draw.tint[3] * inner_alpha,
                    ];
                    let blend = if draw.blend_add {
                        BlendMode::Add
                    } else {
                        BlendMode::Alpha
                    };
                    let z = 104 + layer_idx as i32;
                    let model_actor = model_mesh_cache.as_deref_mut().map_or_else(
                        || {
                            noteskin_model_actor_from_draw(
                                note_slot,
                                draw,
                                layer_center,
                                size,
                                uv,
                                -note_slot.def.rotation_deg as f32,
                                color,
                                blend,
                                z as i16,
                            )
                        },
                        |cache| {
                            noteskin_model_actor_from_draw_cached(
                                note_slot,
                                draw,
                                layer_center,
                                size,
                                uv,
                                -note_slot.def.rotation_deg as f32,
                                color,
                                blend,
                                z as i16,
                                cache,
                            )
                        },
                    );
                    if let Some(model_actor) = model_actor {
                        out.push(model_actor);
                    } else if draw.blend_add {
                        out.push(act!(sprite(note_slot.texture_key_shared()):
                            align(0.5, 0.5):
                            xy(layer_center[0], layer_center[1]):
                            setsize(size[0], size[1]):
                            rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(color[0], color[1], color[2], color[3]):
                            blend(add):
                            z(z)
                        ));
                    } else {
                        out.push(act!(sprite(note_slot.texture_key_shared()):
                            align(0.5, 0.5):
                            xy(layer_center[0], layer_center[1]):
                            setsize(size[0], size[1]):
                            rotationz(draw.rot[2] - note_slot.def.rotation_deg as f32):
                            customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                            diffuse(color[0], color[1], color[2], color[3]):
                            blend(normal):
                            z(z)
                        ));
                    }
                }
            } else if let Some(note_slot) = ns.notes.get(note_idx) {
                let frame = note_slot.frame_index_from_phase(note_uv_phase);
                let uv_elapsed = if note_slot.model.is_some() {
                    note_uv_phase
                } else {
                    preview_time
                };
                let uv = note_slot.uv_for_frame_at(frame, uv_elapsed);
                let size = note_slot.logical_size();
                let width = size[0].max(1.0);
                let height = size[1].max(1.0);
                let scale = if height > 0.0 {
                    target_height / height
                } else {
                    PREVIEW_SCALE
                };
                let preview_size = [width * scale, target_height];
                let draw = note_slot.model_draw_at(preview_time, preview_beat);
                let model_actor = model_mesh_cache.map_or_else(
                    || {
                        noteskin_model_actor_from_draw(
                            note_slot,
                            draw,
                            center,
                            preview_size,
                            uv,
                            -note_slot.def.rotation_deg as f32,
                            [1.0, 1.0, 1.0, inner_alpha],
                            BlendMode::Alpha,
                            104,
                        )
                    },
                    |cache| {
                        noteskin_model_actor_from_draw_cached(
                            note_slot,
                            draw,
                            center,
                            preview_size,
                            uv,
                            -note_slot.def.rotation_deg as f32,
                            [1.0, 1.0, 1.0, inner_alpha],
                            BlendMode::Alpha,
                            104,
                            cache,
                        )
                    },
                );
                if let Some(model_actor) = model_actor {
                    out.push(model_actor);
                } else {
                    out.push(act!(sprite(note_slot.texture_key_shared()):
                        align(0.5, 0.5):
                        xy(center[0], center[1]):
                        setsize(preview_size[0], preview_size[1]):
                        rotationz(-note_slot.def.rotation_deg as f32):
                        customtexturerect(uv[0], uv[1], uv[2], uv[3]):
                        diffusealpha(inner_alpha):
                        z(104)
                    ));
                }
            }
        }

        let judgment_texture = selected.and_then(|choice| {
            if retain_static_payloads {
                choice.judgment_texture
            } else {
                assets::resolve_texture_choice(
                    choice.judgment.texture_key(),
                    assets::judgment_texture_choices(),
                )
            }
        });

        if let Some(texture) = judgment_texture {
            let jd_x = INFO_W.mul_add(0.61, info_x0);
            let jd_y = preview_y - 10.0;
            let actor = if retain_static_payloads {
                act!(sprite_static(texture):
                    align(0.5, 0.5):
                    xy(jd_x, jd_y):
                    setstate(0):
                    zoom(0.160):
                    diffusealpha(inner_alpha):
                    z(104)
                )
            } else {
                act!(sprite(texture):
                    align(0.5, 0.5):
                    xy(jd_x, jd_y):
                    setstate(0):
                    zoom(0.160):
                    diffusealpha(inner_alpha):
                    z(104)
                )
            };
            out.push(actor);
        }

        let mut mods_actor = act!(text:
            align(0.0, 0.0):
            xy(info_text_x, frame_cy + MODS_Y_OFF):
            font("miso"):
            zoom(MODS_ZOOM):
            wrapwidthpixels(info_max_w / MODS_ZOOM):
            maxwidth(info_max_w):
            settext(selected_mods):
            diffuse(1.0, 1.0, 1.0, inner_alpha):
            z(103)
        );
        apply_clip_rect_to_actor(&mut mods_actor, [info_x0, frame_y0, INFO_W, frame_h]);
        out.push(mods_actor);
    }
}

fn push_box_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    let mut model_mesh_cache = state.model_mesh_cache.borrow_mut();
    model_mesh_cache.begin_frame();
    push_box_actors_with_model_cache(
        actors,
        state,
        asset_manager,
        alpha_multiplier,
        visual_policy,
        Some(&mut model_mesh_cache),
        true,
    );
}

fn push_box_actors_with_model_cache(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
    mut model_mesh_cache: Option<&mut ModelMeshCache>,
    retain_static_payloads: bool,
) {
    if alpha_multiplier <= 0.0 {
        return;
    }
    actors.reserve(96);
    let box_start = actors.len();
    let inner_alpha = box_inner_alpha();
    let exit_t = exit_anim_t(state.exit_anim);
    let exit_zoom = if state.exit_anim {
        exit_zoom(exit_t)
    } else {
        1.0
    };

    let frame_h = FRAME_H;
    let cx = screen_center_x();
    let cy = screen_center_y();

    let frame_y0 = frame_h.mul_add(-0.5, cy);

    // IMPORTANT: Apply shake as a post-transform so the frame's explicit tween salt
    // stays stable and the crop-in tweens do not restart every frame.
    let p1_cx = cx - FRAME_CX_OFF;
    let p2_cx = cx + FRAME_CX_OFF;
    let p1_shake_dx = shake_x(state.p1_shake_t);
    let p2_shake_dx = shake_x(state.p2_shake_t);

    let col_overlay = [0.0, 0.0, 0.0, 0.5];
    let border_rgba = [1.0, 1.0, 1.0, 1.0];
    let preview_col = preview_col(state.noteskin_cache.style);

    // P1: keep both frames alive (visibility via alpha) so tween state doesn't reset.
    {
        let side_start = actors.len();
        let show_scroller = state.p1_joined && !state.p1_ready;
        let show_join = !state.p1_joined || state.p1_ready;
        let show_selected_name = state.p1_joined && state.p1_ready;

        let scroller_start = actors.len();
        push_scroller_frame(
            actors,
            asset_manager,
            &state.choices,
            state.p1_selected_index,
            state.p1_scroll_anim,
            state.p1_preview_noteskin.as_deref(),
            preview_col,
            state.preview_time,
            state.preview_beat,
            p1_cx,
            cy,
            frame_y0,
            frame_h,
            state.active_color_index,
            inner_alpha,
            border_rgba,
            col_overlay,
            visual_policy,
            model_mesh_cache.as_deref_mut(),
            retain_static_payloads,
        );
        for a in &mut actors[scroller_start..] {
            a.mul_alpha(if show_scroller { 1.0 } else { 0.0 });
        }

        let join_start = actors.len();
        let join_text = if retain_static_payloads {
            Arc::clone(if state.p1_ready {
                &state.waiting_text
            } else {
                &state.join_text
            })
        } else if state.p1_ready {
            tr("SelectProfile", "WaitingText")
        } else {
            tr("SelectProfile", "JoinText")
        };
        push_join_prompt(
            actors,
            p1_cx,
            cy,
            frame_h,
            border_rgba,
            inner_alpha,
            state.preview_time,
            join_text,
        );
        for a in &mut actors[join_start..] {
            a.mul_alpha(if show_join { 1.0 } else { 0.0 });
        }

        if show_selected_name {
            let name = state.choices.get(state.p1_selected_index).map_or_else(
                || tr("SelectProfile", "GuestLabel"),
                |c| c.display_name.clone(),
            );
            let a = act!(text:
                align(0.5, 0.5):
                xy(p1_cx, cy + SELECTED_NAME_Y_OFF):
                font("miso"):
                zoom(SELECTED_NAME_ZOOM):
                maxwidth(FRAME_W_SCROLLER):
                settext(name):
                diffuse(1.0, 1.0, 1.0, inner_alpha):
                shadowlength(0.5):
                z(106):
                horizalign(center)
            );
            actors.push(a);
        }

        let zoom = exit_zoom * join_pulse_zoom(state.p1_join_pulse_t);
        if (zoom - 1.0).abs() > f32::EPSILON {
            for a in &mut actors[side_start..] {
                apply_zoom_to_actor(a, [p1_cx, cy], zoom);
            }
        }
        if p1_shake_dx != 0.0 {
            for a in &mut actors[side_start..] {
                apply_offset_to_actor(a, p1_shake_dx, 0.0);
            }
        }
    }

    // P2
    {
        let side_start = actors.len();
        let show_scroller = state.p2_joined && !state.p2_ready;
        let show_join = !state.p2_joined || state.p2_ready;
        let show_selected_name = state.p2_joined && state.p2_ready;

        let scroller_start = actors.len();
        push_scroller_frame(
            actors,
            asset_manager,
            &state.choices,
            state.p2_selected_index,
            state.p2_scroll_anim,
            state.p2_preview_noteskin.as_deref(),
            preview_col,
            state.preview_time,
            state.preview_beat,
            p2_cx,
            cy,
            frame_y0,
            frame_h,
            state.active_color_index - 2,
            inner_alpha,
            border_rgba,
            col_overlay,
            visual_policy,
            model_mesh_cache,
            retain_static_payloads,
        );
        for a in &mut actors[scroller_start..] {
            a.mul_alpha(if show_scroller { 1.0 } else { 0.0 });
        }

        let join_start = actors.len();
        let join_text = if retain_static_payloads {
            Arc::clone(if state.p2_ready {
                &state.waiting_text
            } else {
                &state.join_text
            })
        } else if state.p2_ready {
            tr("SelectProfile", "WaitingText")
        } else {
            tr("SelectProfile", "JoinText")
        };
        push_join_prompt(
            actors,
            p2_cx,
            cy,
            frame_h,
            border_rgba,
            inner_alpha,
            state.preview_time,
            join_text,
        );
        for a in &mut actors[join_start..] {
            a.mul_alpha(if show_join { 1.0 } else { 0.0 });
        }

        if show_selected_name {
            let name = state.choices.get(state.p2_selected_index).map_or_else(
                || tr("SelectProfile", "GuestLabel"),
                |c| c.display_name.clone(),
            );
            let a = act!(text:
                align(0.5, 0.5):
                xy(p2_cx, cy + SELECTED_NAME_Y_OFF):
                font("miso"):
                zoom(SELECTED_NAME_ZOOM):
                maxwidth(FRAME_W_SCROLLER):
                settext(name):
                diffuse(1.0, 1.0, 1.0, inner_alpha):
                shadowlength(0.5):
                z(106):
                horizalign(center)
            );
            actors.push(a);
        }

        let zoom = exit_zoom * join_pulse_zoom(state.p2_join_pulse_t);
        if (zoom - 1.0).abs() > f32::EPSILON {
            for a in &mut actors[side_start..] {
                apply_zoom_to_actor(a, [p2_cx, cy], zoom);
            }
        }
        if p2_shake_dx != 0.0 {
            for a in &mut actors[side_start..] {
                apply_offset_to_actor(a, p2_shake_dx, 0.0);
            }
        }
    }

    for a in &mut actors[box_start..] {
        a.mul_alpha(alpha_multiplier);
    }
}

pub fn push_box_actors_with_z(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
    z_offset: i16,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    let start = actors.len();
    actors.reserve(96);
    push_box_actors(
        actors,
        state,
        asset_manager,
        alpha_multiplier,
        visual_policy,
    );
    if z_offset != 0 {
        for actor in &mut actors[start..] {
            apply_z_offset(actor, z_offset);
        }
    }
}

#[cfg(test)]
fn get_box_actors_with_z(
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
    z_offset: i16,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(96);
    push_box_actors_with_z(
        &mut actors,
        state,
        asset_manager,
        alpha_multiplier,
        z_offset,
        visual_policy,
    );
    actors
}

pub fn push_actors(
    actors: &mut Vec<Actor>,
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    actors.reserve(160);

    state.bg.push(
        actors,
        visual_style_bg::Params {
            active_color_index: state.active_color_index,
            backdrop_rgba: [0.0, 0.0, 0.0, 1.0],
            alpha_mul: 1.0,
            visual_policy,
        },
    );

    let fg = [1.0, 1.0, 1.0, 1.0];
    let title = tr("ScreenTitles", "SelectProfile");
    actors.push(screen_bar::build_cached(ScreenBarParams {
        title: &title,
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: fg,
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
        visual_policy,
    }));

    let press_start = tr("Common", "PressStart");
    let not_present = tr("SelectProfile", "NotPresent");
    let (footer_left, footer_right) = match (state.p1_joined, state.p2_joined) {
        (false, false) => (Some(press_start.as_ref()), Some(press_start.as_ref())),
        (true, false) => (None, Some(not_present.as_ref())),
        (false, true) => (Some(not_present.as_ref()), None),
        (true, true) => (None, None),
    };
    let event_mode = tr("Common", "EventMode");
    actors.push(screen_bar::build_cached(ScreenBarParams {
        title: &event_mode,
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: fg,
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar: None,
        right_avatar: None,
        visual_policy,
    }));
    push_box_actors(
        actors,
        state,
        asset_manager,
        alpha_multiplier,
        visual_policy,
    );
}

pub fn get_actors(
    state: &State,
    asset_manager: &AssetManager,
    alpha_multiplier: f32,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(160);
    push_actors(
        &mut actors,
        state,
        asset_manager,
        alpha_multiplier,
        crate::views::SimplyLoveVisualPolicyView::default(),
    );
    actors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::ProfilePickerEntryView;

    fn picker_fixture() -> ProfilePickerView {
        ProfilePickerView {
            game: GameFlag::Dance,
            guest: ProfilePickerEntryView {
                id: String::new(),
                display_name: String::new(),
                speed_mod: "M325".to_owned(),
                avatar_key: None,
                total_songs_played: 0,
                scroll_option: profile_data::ScrollOption::Reverse,
                mini_indicator: profile_data::MiniIndicator::None,
                noteskin: profile_data::NoteSkin::new("metal"),
                judgment: profile_data::JudgmentGraphic::new("Love"),
            },
            profiles: vec![ProfilePickerEntryView {
                id: "alice".to_owned(),
                display_name: "Alice".to_owned(),
                speed_mod: "C650".to_owned(),
                avatar_key: Some("alice.png".to_owned()),
                total_songs_played: 12,
                scroll_option: profile_data::ScrollOption::Reverse,
                mini_indicator: profile_data::MiniIndicator::Pacemaker,
                noteskin: profile_data::NoteSkin::new("cel"),
                judgment: profile_data::JudgmentGraphic::new("Love"),
            }],
            default_profiles: [
                profile_data::ActiveProfile::Local {
                    id: "alice".to_owned(),
                },
                profile_data::ActiveProfile::Guest,
            ],
            three_key_navigation: true,
        }
    }

    #[test]
    fn picker_uses_shell_prepared_choices_and_default_selection() {
        let state = init(picker_fixture());

        assert_eq!(state.choices.len(), 2);
        assert_eq!(state.p1_selected_index, 1);
        assert_eq!(state.p2_selected_index, 0);
        assert!(state.three_key_navigation);
        assert!(state.choices[0].recent_mods.contains("M325"));
        assert_eq!(state.choices[0].noteskin.as_str(), "metal");
        assert_eq!(state.choices[1].display_name.as_ref(), "Alice");
        assert_eq!(state.choices[1].avatar_key.as_deref(), Some("alice.png"));
        assert!(state.choices[1].recent_mods.contains("C650"));
        assert!(state.choices[1].recent_mods.contains("cel"));
    }

    #[test]
    fn profile_frame_keeps_selected_avatar_and_prepared_text() {
        let state = init(picker_fixture());
        let actors = get_box_actors_with_z(
            &state,
            &AssetManager::new(),
            1.0,
            0,
            crate::views::SimplyLoveVisualPolicyView::default(),
        );
        let selected = &state.choices[1];
        let mut texts = actors.iter().filter_map(|actor| match actor {
            Actor::Text { content, .. } => Some(content.as_str()),
            _ => None,
        });
        assert!(
            texts
                .clone()
                .any(|text| text == selected.display_name.as_ref())
        );
        assert!(
            texts
                .clone()
                .any(|text| text == selected.total_songs.as_ref())
        );
        assert!(texts.any(|text| text == selected.recent_mods.as_ref()));
        assert!(actors.iter().any(|actor| matches!(
            actor,
            Actor::Sprite { source, .. }
                if source.texture_key() == selected.avatar_key.as_deref()
        )));
    }

    #[test]
    fn pump_picker_uses_pump_noteskin_preview_style() {
        let mut view = ProfilePickerView::default();
        view.game = GameFlag::Pump;

        let state = init(view);

        assert!(state.noteskin_cache.style.is_pump());
        assert_eq!(preview_col(state.noteskin_cache.style), 3);
        assert_eq!(
            deadsync_noteskin::itg::button_for_col(
                state.noteskin_cache.style.num_cols,
                preview_col(state.noteskin_cache.style),
            ),
            "UpRight"
        );
    }
}
