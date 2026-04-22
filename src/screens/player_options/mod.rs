use crate::act;
use crate::assets::i18n::{LookupKey, lookup_key, tr, tr_fmt};
use crate::assets::{self, AssetManager};
use crate::engine::audio;
use crate::engine::gfx::BlendMode;
use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{screen_center_x, screen_center_y, screen_height, widescale};
use crate::game::chart::ChartData;
use crate::game::parsing::noteskin::{
    self, NUM_QUANTIZATIONS, NoteAnimPart, Noteskin, Quantization, SpriteSlot,
};
use crate::game::song::SongData;
use crate::screens::components::shared::noteskin_model::noteskin_model_actor;
use crate::screens::components::shared::screen_bar::{
    self, AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::screens::components::shared::{heart_bg, transitions};
use crate::screens::input as screen_input;
use crate::screens::{Screen, ScreenAction};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
pub use profile::{SpeedMod, SpeedModType};
pub use render::get_actors;
pub use row::{FixedStepchart, RowId};
pub use state::State;

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
) -> State {
    let session_music_rate = crate::game::profile::get_session_music_rate();
    let allow_per_player_global_offsets =
        crate::config::get().machine_allow_per_player_global_offsets;
    let p1_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P1);
    let p2_profile = crate::game::profile::get_for_side(crate::game::profile::PlayerSide::P2);

    let speed_mod_p1 = SpeedMod::from(p1_profile.scroll_speed);
    let speed_mod_p2 = SpeedMod::from(p2_profile.scroll_speed);
    let chart_difficulty_index: [usize; PLAYER_SLOTS] = std::array::from_fn(|player_idx| {
        let steps_idx = chart_steps_index[player_idx];
        let mut diff_idx = preferred_difficulty_index[player_idx].min(
            crate::engine::present::color::FILE_DIFFICULTY_NAMES
                .len()
                .saturating_sub(1),
        );
        if steps_idx < crate::engine::present::color::FILE_DIFFICULTY_NAMES.len() {
            diff_idx = steps_idx;
        }
        diff_idx
    });

    let noteskin_names = discover_noteskin_names();
    let mut main_row_map = build_rows(
        &song,
        &speed_mod_p1,
        chart_steps_index,
        preferred_difficulty_index,
        session_music_rate,
        OptionsPane::Main,
        &noteskin_names,
        return_screen,
        fixed_stepchart.as_ref(),
    );
    let mut advanced_row_map = build_rows(
        &song,
        &speed_mod_p1,
        chart_steps_index,
        preferred_difficulty_index,
        session_music_rate,
        OptionsPane::Advanced,
        &noteskin_names,
        return_screen,
        fixed_stepchart.as_ref(),
    );
    let mut uncommon_row_map = build_rows(
        &song,
        &speed_mod_p1,
        chart_steps_index,
        preferred_difficulty_index,
        session_music_rate,
        OptionsPane::Uncommon,
        &noteskin_names,
        return_screen,
        fixed_stepchart.as_ref(),
    );
    let player_profiles = [p1_profile.clone(), p2_profile.clone()];
    // `apply_profile_defaults` populates 8 of its 17 returned masks (Scroll,
    // Insert, Remove, Holds, Accel, Effect, Appearance, EarlyDw) only when
    // the corresponding row exists in the passed `row_map`. Those rows live
    // on the Advanced and Uncommon panes, so we must call the function on
    // every pane and OR the results together. Otherwise persisted profile
    // state for those rows would silently appear empty here and get
    // overwritten the moment the user touches any choice on those rows.
    let p1_main = apply_profile_defaults(&mut main_row_map, &player_profiles[P1], P1);
    let p2_main = apply_profile_defaults(&mut main_row_map, &player_profiles[P2], P2);
    let p1_advanced = apply_profile_defaults(&mut advanced_row_map, &player_profiles[P1], P1);
    let p2_advanced = apply_profile_defaults(&mut advanced_row_map, &player_profiles[P2], P2);
    let p1_uncommon = apply_profile_defaults(&mut uncommon_row_map, &player_profiles[P1], P1);
    let p2_uncommon = apply_profile_defaults(&mut uncommon_row_map, &player_profiles[P2], P2);
    let p1_masks = p1_main.merge(p1_advanced).merge(p1_uncommon);
    let p2_masks = p2_main.merge(p2_advanced).merge(p2_uncommon);

    let cols_per_player = noteskin_cols_per_player(crate::game::profile::get_session_play_style());
    let mut initial_noteskin_names = vec![crate::game::profile::NoteSkin::DEFAULT_NAME.to_string()];
    for profile in &player_profiles {
        push_noteskin_name_once(&mut initial_noteskin_names, &profile.noteskin);
        if let Some(skin) = profile.mine_noteskin.as_ref() {
            push_noteskin_name_once(&mut initial_noteskin_names, skin);
        }
        if let Some(skin) = profile.receptor_noteskin.as_ref() {
            push_noteskin_name_once(&mut initial_noteskin_names, skin);
        }
        if let Some(skin) = profile.tap_explosion_noteskin.as_ref() {
            push_noteskin_name_once(&mut initial_noteskin_names, skin);
        }
    }
    let mut noteskin_cache = build_noteskin_cache(cols_per_player, &initial_noteskin_names);
    let noteskin_previews: [PlayerNoteskinPreviews; PLAYER_SLOTS] = std::array::from_fn(|i| {
        let profile_noteskin = &player_profiles[i].noteskin;
        PlayerNoteskinPreviews {
            base: cached_or_load_noteskin(&mut noteskin_cache, profile_noteskin, cols_per_player),
            mine: resolved_noteskin_override_preview(
                &mut noteskin_cache,
                profile_noteskin,
                player_profiles[i].mine_noteskin.as_ref(),
                cols_per_player,
            ),
            receptor: resolved_noteskin_override_preview(
                &mut noteskin_cache,
                profile_noteskin,
                player_profiles[i].receptor_noteskin.as_ref(),
                cols_per_player,
            ),
            tap_explosion: resolved_tap_explosion_preview(
                &mut noteskin_cache,
                profile_noteskin,
                player_profiles[i].tap_explosion_noteskin.as_ref(),
                cols_per_player,
            ),
        }
    });
    let active = session_active_players();
    let main_row_tweens = init_row_tweens(
        &main_row_map,
        [0; PLAYER_SLOTS],
        active,
        [p1_masks, p2_masks],
        allow_per_player_global_offsets,
    );
    let mut panes = [
        PaneState::new(main_row_map),
        PaneState::new(advanced_row_map),
        PaneState::new(uncommon_row_map),
    ];
    panes[OptionsPane::Main.index()].row_tweens = main_row_tweens;
    panes[OptionsPane::Main.index()].arcade_row_focus = [true; PLAYER_SLOTS];
    State {
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
        scroll_focus_player: P1,
        bg: heart_bg::State::new(),
        nav_input: [PlayerNavInput::default(); PLAYER_SLOTS],
        start_input: [PlayerStartInput::default(); PLAYER_SLOTS],
        allow_per_player_global_offsets,
        player_profiles,
        noteskin_cache,
        noteskin_previews,
        preview_time: 0.0,
        preview_beat: 0.0,
        help_anim_time: [0.0; PLAYER_SLOTS],
        combo_preview_count: 0,
        combo_preview_elapsed: 0.0,
        pane_transition: PaneTransition::None,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
    }
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    transitions::fade_in_black(TRANSITION_IN_DURATION, 1100)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    transitions::fade_out_black(TRANSITION_OUT_DURATION, 1200)
}

#[inline(always)]
fn session_active_players() -> [bool; PLAYER_SLOTS] {
    let play_style = crate::game::profile::get_session_play_style();
    let side = crate::game::profile::get_session_player_side();
    let joined = [
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P1),
        crate::game::profile::is_session_side_joined(crate::game::profile::PlayerSide::P2),
    ];
    let joined_count = usize::from(joined[P1]) + usize::from(joined[P2]);
    match play_style {
        crate::game::profile::PlayStyle::Versus => {
            if joined_count > 0 {
                joined
            } else {
                [true, true]
            }
        }
        crate::game::profile::PlayStyle::Single | crate::game::profile::PlayStyle::Double => {
            if joined_count == 1 {
                joined
            } else {
                match side {
                    crate::game::profile::PlayerSide::P1 => [true, false],
                    crate::game::profile::PlayerSide::P2 => [false, true],
                }
            }
        }
    }
}

#[inline(always)]
fn arcade_options_navigation_active() -> bool {
    crate::config::get().arcade_options_navigation
}

#[inline(always)]
const fn pane_uses_arcade_next_row(pane: OptionsPane) -> bool {
    !matches!(pane, OptionsPane::Main)
}

#[inline(always)]
fn session_persisted_player_idx() -> usize {
    let play_style = crate::game::profile::get_session_play_style();
    let side = crate::game::profile::get_session_player_side();
    match play_style {
        crate::game::profile::PlayStyle::Versus => P1,
        crate::game::profile::PlayStyle::Single | crate::game::profile::PlayStyle::Double => {
            match side {
                crate::game::profile::PlayerSide::P1 => P1,
                crate::game::profile::PlayerSide::P2 => P2,
            }
        }
    }
}
