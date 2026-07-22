use crate::act;
use crate::assets::i18n::{LookupKey, lookup_key, tr, tr_fmt};
use crate::assets::{self, AssetManager};
use crate::screens::components::shared::screen_bar::{
    self, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::shared::{transitions, visual_style_bg};
use crate::screens::input as screen_input;
use crate::screens::{Screen, ThemeEffect};
use crate::views::{PlayerOptionsInitView, PlayerOptionsPolicyView};
use deadlib_present::actors::Actor;
use deadlib_present::color;
use deadlib_present::space::{screen_center_x, screen_center_y, screen_height, widescale};
use deadlib_render::BlendMode;
use deadsync_assets::noteskin::{
    self, NUM_QUANTIZATIONS, NoteAnimPart, Noteskin, Quantization, SpriteSlot,
};
use deadsync_chart::{ChartData, STANDARD_DIFFICULTY_COUNT, SongData};
use deadsync_input::{InputEvent, VirtualAction};
use deadsync_notefield::noteskin_model_actor;
use deadsync_profile as profile_data;
use deadsync_theme::AudioRequest;
use deadsync_theme::views::{NoteskinCatalogView, SmxGifCatalogView};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

// --- Submodules ---
mod choice;
mod constants;
mod inline_nav;
mod input;
mod layout;
mod noteskins;
mod pane;
mod panes;
mod profile;
mod render;
mod row;
mod state;
mod visibility;

#[cfg(test)]
mod tests;

// Re-import every submodule so legacy code in this file resolves.
#[allow(unused_imports)]
use choice::*;
#[allow(unused_imports)]
use constants::*;
#[allow(unused_imports)]
use inline_nav::*;
#[allow(unused_imports)]
use input::*;
#[allow(unused_imports)]
use layout::*;
#[allow(unused_imports)]
use noteskins::*;
#[allow(unused_imports)]
use pane::*;
#[allow(unused_imports)]
use panes::*;
#[allow(unused_imports)]
use profile::*;
#[allow(unused_imports)]
use render::*;
#[allow(unused_imports)]
use row::*;
#[allow(unused_imports)]
use state::*;
#[allow(unused_imports)]
use visibility::*;

// --- External API ---
pub use input::{handle_input, update};
pub use profile::{
    SpeedMod, SpeedModType, apply_no_cmod_alternative, convert_speed_mod_to_type,
    effective_scroll_speed_with_alt, no_cmod_alt_speed_mod_type, scroll_speed_for_mod,
};
pub use render::{get_actors, push_actors};
pub use row::{FixedStepchart, RowId};
pub use state::State;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeartRateDeviceView {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeartRateReadingView {
    pub configured: bool,
    pub connected: bool,
    pub bpm: Option<u16>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeartRateDevicesView {
    pub supported: bool,
    pub scanning: bool,
    pub devices: Vec<HeartRateDeviceView>,
    pub error: Option<String>,
    pub readings: [HeartRateReadingView; PLAYER_SLOTS],
}

const CHANGE_VALUE_SFX: &str = "assets/sounds/change_value.ogg";
const NEXT_ROW_SFX: &str = "assets/sounds/next_row.ogg";
const PREV_ROW_SFX: &str = "assets/sounds/prev_row.ogg";
const START_SFX: &str = "assets/sounds/start.ogg";

#[inline(always)]
fn queue_audio(state: &mut State, request: AudioRequest) {
    state.pending_effects.push(ThemeEffect::Runtime(
        crate::SimplyLoveRuntimeRequest::Audio(request),
    ));
}

fn queue_profile_request(state: &mut State, request: crate::SimplyLoveProfileRequest) {
    state.pending_effects.push(ThemeEffect::Runtime(
        crate::SimplyLoveRuntimeRequest::Profile(request),
    ));
}

#[inline(always)]
fn queue_sfx(state: &mut State, path: &'static str) {
    queue_audio(state, AudioRequest::PlaySfx(path.to_owned()));
}

fn prepend_pending_effects(state: &mut State, effect: ThemeEffect) -> ThemeEffect {
    let request_count = state.pending_effects.len();
    if request_count == 0 {
        return effect;
    }

    let has_effect = !matches!(effect, ThemeEffect::None);
    let mut effects = Vec::with_capacity(request_count + usize::from(has_effect));
    effects.append(&mut state.pending_effects);
    if has_effect {
        effects.push(effect);
    }
    if effects.len() == 1 {
        effects.pop().expect("one queued PlayerOptions effect")
    } else {
        ThemeEffect::Batch(effects)
    }
}

fn queue_profile_update(state: &mut State, player_idx: usize) {
    let idx = player_idx.min(PLAYER_SLOTS - 1);
    let (should_persist, side) = choice::persist_ctx(state, idx);
    if !should_persist {
        return;
    }
    state.pending_effects.push(ThemeEffect::Runtime(
        crate::SimplyLoveRuntimeRequest::Profile(
            crate::SimplyLoveProfileRequest::UpdatePlayerOptions {
                side,
                options: Box::new(state.player_options[idx].clone()),
                heart_rate_device_id: state.heart_rate_device_ids[idx].clone(),
            },
        ),
    ));
}

#[inline(always)]
fn active_player_indices(active: [bool; PLAYER_SLOTS]) -> impl Iterator<Item = usize> {
    [P1, P2]
        .into_iter()
        .filter(move |&player_idx| active[player_idx])
}

pub fn init(
    song: Arc<SongData>,
    chart_steps_index: [usize; PLAYER_SLOTS],
    preferred_difficulty_index: [usize; PLAYER_SLOTS],
    active_color_index: i32,
    return_screen: Screen,
    fixed_stepchart: Option<FixedStepchart>,
    noteskin_catalog: NoteskinCatalogView,
    smx_gif_catalog: SmxGifCatalogView,
    heart_rate_devices: HeartRateDevicesView,
    init_view: PlayerOptionsInitView,
) -> State {
    init_with_noteskin_prewarm(
        song,
        chart_steps_index,
        preferred_difficulty_index,
        active_color_index,
        return_screen,
        fixed_stepchart,
        noteskin_catalog,
        smx_gif_catalog,
        heart_rate_devices,
        init_view,
        true,
    )
}

pub fn init_for_gameplay(
    song: Arc<SongData>,
    chart_steps_index: [usize; PLAYER_SLOTS],
    preferred_difficulty_index: [usize; PLAYER_SLOTS],
    active_color_index: i32,
    return_screen: Screen,
    fixed_stepchart: Option<FixedStepchart>,
    noteskin_catalog: NoteskinCatalogView,
    smx_gif_catalog: SmxGifCatalogView,
    heart_rate_devices: HeartRateDevicesView,
    init_view: PlayerOptionsInitView,
) -> State {
    init_with_noteskin_prewarm(
        song,
        chart_steps_index,
        preferred_difficulty_index,
        active_color_index,
        return_screen,
        fixed_stepchart,
        noteskin_catalog,
        smx_gif_catalog,
        heart_rate_devices,
        init_view,
        false,
    )
}

pub fn prewarm_noteskin_previews(state: &mut State) {
    let noteskin_names = state.panes[OptionsPane::Main.index()]
        .row_map
        .get(RowId::NoteSkin)
        .map(|row| row.choices.clone())
        .unwrap_or_default();
    state.noteskin = init_noteskin_state(
        state.cols_per_player,
        &noteskin_names,
        &state.player_options,
        true,
    );
}

fn init_with_noteskin_prewarm(
    song: Arc<SongData>,
    chart_steps_index: [usize; PLAYER_SLOTS],
    preferred_difficulty_index: [usize; PLAYER_SLOTS],
    active_color_index: i32,
    return_screen: Screen,
    fixed_stepchart: Option<FixedStepchart>,
    noteskin_catalog: NoteskinCatalogView,
    smx_gif_catalog: SmxGifCatalogView,
    heart_rate_devices: HeartRateDevicesView,
    init_view: PlayerOptionsInitView,
    prewarm_noteskin_catalog: bool,
) -> State {
    let PlayerOptionsInitView {
        policy,
        play_style,
        player_side,
        joined,
        music_rate: session_music_rate,
        players,
    } = init_view;
    let [p1, p2] = players;
    let heart_rate_device_ids = [p1.heart_rate_device_id, p2.heart_rate_device_id];
    let player_options = [p1.options, p2.options];
    let active = active_players(play_style, player_side, joined);
    let persisted_player_idx = persisted_player_idx(play_style, player_side);
    let cols_per_player = play_style.cols_per_player();
    let (heart_rate_choices, heart_rate_choice_ids) = if policy.heart_rate_monitors {
        heart_rate_choices(&heart_rate_devices, &heart_rate_device_ids)
    } else {
        (Vec::new(), Vec::new())
    };

    let speed_mod_p1 = SpeedMod::from(player_options[P1].scroll_speed);
    let speed_mod_p2 = SpeedMod::from(player_options[P2].scroll_speed);
    let chart_difficulty_index: [usize; PLAYER_SLOTS] = std::array::from_fn(|player_idx| {
        let steps_idx = chart_steps_index[player_idx];
        let mut diff_idx =
            preferred_difficulty_index[player_idx].min(STANDARD_DIFFICULTY_COUNT.saturating_sub(1));
        if steps_idx < STANDARD_DIFFICULTY_COUNT {
            diff_idx = steps_idx;
        }
        diff_idx
    });

    let noteskin_names = noteskin_catalog.names;
    let smx_bg_pack_names = smx_gif_catalog.background_packs;
    let smx_judge_pack_names = smx_gif_catalog.judgment_packs;
    let mut main_row_map = build_rows(
        &song,
        &speed_mod_p1,
        chart_steps_index,
        preferred_difficulty_index,
        session_music_rate,
        OptionsPane::Main,
        &noteskin_names,
        &smx_bg_pack_names,
        &smx_judge_pack_names,
        &heart_rate_choices,
        return_screen,
        fixed_stepchart.as_ref(),
        play_style,
        persisted_player_idx,
        policy.scorebox_available,
    );
    let mut display_row_map = build_rows(
        &song,
        &speed_mod_p1,
        chart_steps_index,
        preferred_difficulty_index,
        session_music_rate,
        OptionsPane::Display,
        &noteskin_names,
        &smx_bg_pack_names,
        &smx_judge_pack_names,
        &heart_rate_choices,
        return_screen,
        fixed_stepchart.as_ref(),
        play_style,
        persisted_player_idx,
        policy.scorebox_available,
    );
    let mut advanced_row_map = build_rows(
        &song,
        &speed_mod_p1,
        chart_steps_index,
        preferred_difficulty_index,
        session_music_rate,
        OptionsPane::Advanced,
        &noteskin_names,
        &smx_bg_pack_names,
        &smx_judge_pack_names,
        &heart_rate_choices,
        return_screen,
        fixed_stepchart.as_ref(),
        play_style,
        persisted_player_idx,
        policy.scorebox_available,
    );
    let mut uncommon_row_map = build_rows(
        &song,
        &speed_mod_p1,
        chart_steps_index,
        preferred_difficulty_index,
        session_music_rate,
        OptionsPane::Uncommon,
        &noteskin_names,
        &smx_bg_pack_names,
        &smx_judge_pack_names,
        &heart_rate_choices,
        return_screen,
        fixed_stepchart.as_ref(),
        play_style,
        persisted_player_idx,
        policy.scorebox_available,
    );
    // Each `BitmaskBinding` lives on exactly one pane's row, and
    // `apply_derived_masks` is a pure function of `profile`, so calling
    // `apply_profile_defaults` once per (pane, player) with the same
    // `&mut PlayerOptionMasks` accumulates writes safely without needing
    // a per-pane merge step.
    let mut p1_masks = PlayerOptionMasks::default();
    let mut p2_masks = PlayerOptionMasks::default();
    apply_profile_defaults(&mut main_row_map, &player_options[P1], P1, &mut p1_masks);
    apply_profile_defaults(&mut main_row_map, &player_options[P2], P2, &mut p2_masks);
    apply_profile_defaults(&mut display_row_map, &player_options[P1], P1, &mut p1_masks);
    apply_profile_defaults(&mut display_row_map, &player_options[P2], P2, &mut p2_masks);
    apply_profile_defaults(
        &mut advanced_row_map,
        &player_options[P1],
        P1,
        &mut p1_masks,
    );
    apply_profile_defaults(
        &mut advanced_row_map,
        &player_options[P2],
        P2,
        &mut p2_masks,
    );
    apply_profile_defaults(
        &mut uncommon_row_map,
        &player_options[P1],
        P1,
        &mut p1_masks,
    );
    apply_profile_defaults(
        &mut uncommon_row_map,
        &player_options[P2],
        P2,
        &mut p2_masks,
    );

    // Only real Player Options entry runs the catalog warmup while the previous
    // screen shows "Entering Options...". Direct song starts leave this empty;
    // Gameplay loads and prewarms only the active players' resolved settings.
    let noteskin = init_noteskin_state(
        cols_per_player,
        &noteskin_names,
        &player_options,
        prewarm_noteskin_catalog,
    );
    let main_row_tweens = init_row_tweens(
        &main_row_map,
        [0; PLAYER_SLOTS],
        active,
        [p1_masks, p2_masks],
        policy,
    );
    let mut panes = [
        PaneState::new(main_row_map),
        PaneState::new(display_row_map),
        PaneState::new(advanced_row_map),
        PaneState::new(uncommon_row_map),
    ];
    panes[OptionsPane::Main.index()].row_tweens = main_row_tweens;
    panes[OptionsPane::Main.index()].arcade_row_focus = [true; PLAYER_SLOTS];
    let mut state = State {
        song,
        return_screen,
        fixed_stepchart,
        chart_steps_index,
        chart_difficulty_index,
        panes,
        option_masks: [p1_masks, p2_masks],
        active_color_index,
        speed_mod: [speed_mod_p1, speed_mod_p2],
        music_rate: session_music_rate,
        current_pane: OptionsPane::Main,
        bg: visual_style_bg::State::new(),
        nav_input: [PlayerNavInput::default(); PLAYER_SLOTS],
        start_input: [PlayerStartInput::default(); PLAYER_SLOTS],
        policy,
        play_style,
        active,
        persisted_player_idx,
        cols_per_player,
        player_options,
        heart_rate_device_ids,
        heart_rate_choice_ids,
        heart_rate_readings: heart_rate_devices.readings,
        noteskin,
        preview_time: 0.0,
        preview_beat: 0.0,
        help_anim_time: [0.0; PLAYER_SLOTS],
        combo_preview_count: 0,
        combo_preview_elapsed: 0.0,
        pane_transition: PaneTransition::None,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
        pending_effects: Vec::with_capacity(4),
    };
    sync_speed_mod_type_rows(&mut state);
    sync_heart_rate_selections(&mut state);
    state
}

fn heart_rate_choices(
    devices: &HeartRateDevicesView,
    selected_ids: &[Option<String>; PLAYER_SLOTS],
) -> (Vec<String>, Vec<Option<String>>) {
    let mut choices = vec![tr("Common", "Off").to_string()];
    let mut ids = vec![None];
    for device in &devices.devices {
        if ids.iter().flatten().any(|id| id == &device.id) {
            continue;
        }
        choices.push(device.label.clone());
        ids.push(Some(device.id.clone()));
    }
    for selected_id in selected_ids {
        let Some(id) = selected_id.as_ref() else {
            continue;
        };
        if ids.iter().flatten().any(|known| known == id) {
            continue;
        }
        choices.push("Saved HRM".to_owned());
        ids.push(Some(id.clone()));
    }
    (choices, ids)
}

fn sync_heart_rate_selections(state: &mut State) {
    let ids = state.heart_rate_choice_ids.clone();
    let Some(row) = state.panes[OptionsPane::Main.index()]
        .row_map
        .get_mut(RowId::HeartRateMonitor)
    else {
        return;
    };
    for player in 0..PLAYER_SLOTS {
        row.selected_choice_index[player] = ids
            .iter()
            .position(|id| id == &state.heart_rate_device_ids[player])
            .unwrap_or(0);
    }
}

pub fn set_heart_rate_devices(state: &mut State, devices: &HeartRateDevicesView) {
    if !state.policy.heart_rate_monitors {
        return;
    }
    state.heart_rate_readings = devices.readings;
    let (choices, ids) = heart_rate_choices(devices, &state.heart_rate_device_ids);
    let Some(row) = state.panes[OptionsPane::Main.index()]
        .row_map
        .get_mut(RowId::HeartRateMonitor)
    else {
        return;
    };
    if row.choices == choices && state.heart_rate_choice_ids == ids {
        return;
    }
    row.choices = choices;
    state.heart_rate_choice_ids = ids;
    sync_heart_rate_selections(state);
}

fn sync_speed_mod_type_row(row_map: &mut RowMap, speed_mod: &[SpeedMod; PLAYER_SLOTS]) {
    let Some(row) = row_map.get_mut(RowId::TypeOfSpeedMod) else {
        return;
    };
    for player_idx in 0..PLAYER_SLOTS {
        row.selected_choice_index[player_idx] = speed_mod[player_idx]
            .mod_type
            .choice_index()
            .min(row.choices.len().saturating_sub(1));
    }
}

pub fn sync_speed_mod_type_rows(state: &mut State) {
    let speed_mod = state.speed_mod.clone();
    for pane in &mut state.panes {
        sync_speed_mod_type_row(&mut pane.row_map, &speed_mod);
    }
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

/// Per-side live state for the Pad Light Brightness preview, indexed by player
/// slot (0 = P1, 1 = P2): `Some(percent)` for an active player whose cursor is
/// currently on the Pad Light Brightness row, else `None`. The app loop drives a
/// rainbow preview on that side's pad while the value is being tuned. The row's
/// choices are `0..=100%`, so the selected choice index is the percent.
pub fn pad_light_brightness_preview(state: &State) -> [Option<u8>; PLAYER_SLOTS] {
    let pane = state.pane();
    let active = state.active;
    let mut preview: [Option<u8>; PLAYER_SLOTS] = std::array::from_fn(|idx| {
        if !active[idx] {
            return None;
        }
        let row = pane.row_map.get_at(pane.selected_row[idx])?;
        (row.id == RowId::PadLightBrightness).then(|| row.selected_choice_index[idx].min(100) as u8)
    });
    // In Doubles a single player owns BOTH pads, so the brightness value applies
    // to both (see `pad_light_brightness_for_pad`). Mirror the lone active side's
    // preview onto the other pad so both light up while the value is being tuned.
    if matches!(state.play_style, profile_data::PlayStyle::Double)
        && let Some(pct) = preview.iter().flatten().copied().next()
    {
        preview = [Some(pct); PLAYER_SLOTS];
    }
    preview
}

#[inline(always)]
const fn active_players(
    play_style: profile_data::PlayStyle,
    side: profile_data::PlayerSide,
    joined: [bool; PLAYER_SLOTS],
) -> [bool; PLAYER_SLOTS] {
    let joined_count = joined[P1] as usize + joined[P2] as usize;
    match play_style {
        profile_data::PlayStyle::Versus => {
            if joined_count > 0 {
                joined
            } else {
                [true, true]
            }
        }
        profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => {
            if joined_count == 1 {
                joined
            } else {
                match side {
                    profile_data::PlayerSide::P1 => [true, false],
                    profile_data::PlayerSide::P2 => [false, true],
                }
            }
        }
    }
}

#[inline(always)]
const fn pane_uses_arcade_next_row(pane: OptionsPane) -> bool {
    !matches!(pane, OptionsPane::Main)
}

#[inline(always)]
const fn persisted_player_idx(
    play_style: profile_data::PlayStyle,
    side: profile_data::PlayerSide,
) -> usize {
    match play_style {
        profile_data::PlayStyle::Versus => P1,
        profile_data::PlayStyle::Single | profile_data::PlayStyle::Double => match side {
            profile_data::PlayerSide::P1 => P1,
            profile_data::PlayerSide::P2 => P2,
        },
    }
}
