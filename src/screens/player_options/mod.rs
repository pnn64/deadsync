use crate::act;
use crate::assets::i18n::{LookupKey, lookup_key, tr, tr_fmt};
use crate::assets::{self, AssetManager};
use crate::engine::audio;
use crate::engine::gfx::BlendMode;
use crate::engine::input::{InputEvent, VirtualAction};
use crate::engine::present::actors::Actor;
use crate::engine::present::color;
use crate::engine::space::{
    screen_center_x, screen_center_y, screen_height, screen_width, widescale,
};
use crate::game::chart::ChartData;
use crate::game::parsing::noteskin::{
    self, NUM_QUANTIZATIONS, NoteAnimPart, Noteskin, Quantization, SpriteSlot,
};
use crate::game::song::SongData;
use crate::screens::components::shared::heart_bg;
use crate::screens::components::shared::noteskin_model::noteskin_model_actor;
use crate::screens::components::shared::screen_bar::{
    self, AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
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
mod profile;
mod render;
mod row;
mod rows;
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
use profile::*;
#[allow(unused_imports)]
use render::*;
#[allow(unused_imports)]
use row::*;
#[allow(unused_imports)]
use rows::*;
#[allow(unused_imports)]
use state::*;
#[allow(unused_imports)]
use visibility::*;

// --- External API ---
pub use input::{handle_input, update};
pub use profile::SpeedMod;
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

    let speed_mod_p1 = match p1_profile.scroll_speed {
        crate::game::scroll::ScrollSpeedSetting::CMod(bpm) => SpeedMod {
            mod_type: "C".to_string(),
            value: bpm,
        },
        crate::game::scroll::ScrollSpeedSetting::XMod(mult) => SpeedMod {
            mod_type: "X".to_string(),
            value: mult,
        },
        crate::game::scroll::ScrollSpeedSetting::MMod(bpm) => SpeedMod {
            mod_type: "M".to_string(),
            value: bpm,
        },
    };
    let speed_mod_p2 = match p2_profile.scroll_speed {
        crate::game::scroll::ScrollSpeedSetting::CMod(bpm) => SpeedMod {
            mod_type: "C".to_string(),
            value: bpm,
        },
        crate::game::scroll::ScrollSpeedSetting::XMod(mult) => SpeedMod {
            mod_type: "X".to_string(),
            value: mult,
        },
        crate::game::scroll::ScrollSpeedSetting::MMod(bpm) => SpeedMod {
            mod_type: "M".to_string(),
            value: bpm,
        },
    };
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
    let mut row_map = build_rows(
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
    let player_profiles = [p1_profile.clone(), p2_profile.clone()];
    let (
        scroll_active_mask_p1,
        hide_active_mask_p1,
        insert_active_mask_p1,
        remove_active_mask_p1,
        holds_active_mask_p1,
        accel_effects_active_mask_p1,
        visual_effects_active_mask_p1,
        appearance_effects_active_mask_p1,
        fa_plus_active_mask_p1,
        early_dw_active_mask_p1,
        gameplay_extras_active_mask_p1,
        gameplay_extras_more_active_mask_p1,
        results_extras_active_mask_p1,
        life_bar_options_active_mask_p1,
        error_bar_active_mask_p1,
        error_bar_options_active_mask_p1,
        measure_counter_options_active_mask_p1,
    ) = apply_profile_defaults(&mut row_map, &player_profiles[P1], P1);
    let (
        scroll_active_mask_p2,
        hide_active_mask_p2,
        insert_active_mask_p2,
        remove_active_mask_p2,
        holds_active_mask_p2,
        accel_effects_active_mask_p2,
        visual_effects_active_mask_p2,
        appearance_effects_active_mask_p2,
        fa_plus_active_mask_p2,
        early_dw_active_mask_p2,
        gameplay_extras_active_mask_p2,
        gameplay_extras_more_active_mask_p2,
        results_extras_active_mask_p2,
        life_bar_options_active_mask_p2,
        error_bar_active_mask_p2,
        error_bar_options_active_mask_p2,
        measure_counter_options_active_mask_p2,
    ) = apply_profile_defaults(&mut row_map, &player_profiles[P2], P2);

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
    let noteskin_previews: [Option<Arc<Noteskin>>; PLAYER_SLOTS] = std::array::from_fn(|i| {
        cached_or_load_noteskin(
            &mut noteskin_cache,
            &player_profiles[i].noteskin,
            cols_per_player,
        )
    });
    let mine_noteskin_previews: [Option<Arc<Noteskin>>; PLAYER_SLOTS] = std::array::from_fn(|i| {
        resolved_noteskin_override_preview(
            &mut noteskin_cache,
            &player_profiles[i].noteskin,
            player_profiles[i].mine_noteskin.as_ref(),
            cols_per_player,
        )
    });
    let receptor_noteskin_previews: [Option<Arc<Noteskin>>; PLAYER_SLOTS] =
        std::array::from_fn(|i| {
            resolved_noteskin_override_preview(
                &mut noteskin_cache,
                &player_profiles[i].noteskin,
                player_profiles[i].receptor_noteskin.as_ref(),
                cols_per_player,
            )
        });
    let tap_explosion_noteskin_previews: [Option<Arc<Noteskin>>; PLAYER_SLOTS] =
        std::array::from_fn(|i| {
            resolved_tap_explosion_preview(
                &mut noteskin_cache,
                &player_profiles[i].noteskin,
                player_profiles[i].tap_explosion_noteskin.as_ref(),
                cols_per_player,
            )
        });
    let active = session_active_players();
    let row_tweens = init_row_tweens(
        &row_map,
        [0; PLAYER_SLOTS],
        active,
        [hide_active_mask_p1, hide_active_mask_p2],
        [error_bar_active_mask_p1, error_bar_active_mask_p2],
        allow_per_player_global_offsets,
    );
    State {
        song,
        return_screen,
        fixed_stepchart,
        chart_steps_index,
        chart_difficulty_index,
        row_map,
        selected_row: [0; PLAYER_SLOTS],
        prev_selected_row: [0; PLAYER_SLOTS],
        scroll_active_mask: [scroll_active_mask_p1, scroll_active_mask_p2],
        hide_active_mask: [hide_active_mask_p1, hide_active_mask_p2],
        insert_active_mask: [insert_active_mask_p1, insert_active_mask_p2],
        remove_active_mask: [remove_active_mask_p1, remove_active_mask_p2],
        holds_active_mask: [holds_active_mask_p1, holds_active_mask_p2],
        accel_effects_active_mask: [accel_effects_active_mask_p1, accel_effects_active_mask_p2],
        visual_effects_active_mask: [visual_effects_active_mask_p1, visual_effects_active_mask_p2],
        appearance_effects_active_mask: [
            appearance_effects_active_mask_p1,
            appearance_effects_active_mask_p2,
        ],
        fa_plus_active_mask: [fa_plus_active_mask_p1, fa_plus_active_mask_p2],
        early_dw_active_mask: [early_dw_active_mask_p1, early_dw_active_mask_p2],
        gameplay_extras_active_mask: [
            gameplay_extras_active_mask_p1,
            gameplay_extras_active_mask_p2,
        ],
        gameplay_extras_more_active_mask: [
            gameplay_extras_more_active_mask_p1,
            gameplay_extras_more_active_mask_p2,
        ],
        results_extras_active_mask: [results_extras_active_mask_p1, results_extras_active_mask_p2],
        life_bar_options_active_mask: [
            life_bar_options_active_mask_p1,
            life_bar_options_active_mask_p2,
        ],
        error_bar_active_mask: [error_bar_active_mask_p1, error_bar_active_mask_p2],
        error_bar_options_active_mask: [
            error_bar_options_active_mask_p1,
            error_bar_options_active_mask_p2,
        ],
        measure_counter_options_active_mask: [
            measure_counter_options_active_mask_p1,
            measure_counter_options_active_mask_p2,
        ],
        active_color_index,
        speed_mod: [speed_mod_p1, speed_mod_p2],
        music_rate: session_music_rate,
        current_pane: OptionsPane::Main,
        scroll_focus_player: P1,
        bg: heart_bg::State::new(),
        nav_key_held_direction: [None; PLAYER_SLOTS],
        nav_key_held_since: [None; PLAYER_SLOTS],
        nav_key_last_scrolled_at: [None; PLAYER_SLOTS],
        start_held_since: [None; PLAYER_SLOTS],
        start_last_triggered_at: [None; PLAYER_SLOTS],
        inline_choice_x: [f32::NAN; PLAYER_SLOTS],
        arcade_row_focus: [true; PLAYER_SLOTS],
        allow_per_player_global_offsets,
        player_profiles,
        noteskin_names,
        noteskin_cache,
        noteskin: noteskin_previews,
        mine_noteskin: mine_noteskin_previews,
        receptor_noteskin: receptor_noteskin_previews,
        tap_explosion_noteskin: tap_explosion_noteskin_previews,
        preview_time: 0.0,
        preview_beat: 0.0,
        help_anim_time: [0.0; PLAYER_SLOTS],
        combo_preview_count: 0,
        combo_preview_elapsed: 0.0,
        cursor_initialized: [false; PLAYER_SLOTS],
        cursor_from_x: [0.0; PLAYER_SLOTS],
        cursor_from_y: [0.0; PLAYER_SLOTS],
        cursor_from_w: [0.0; PLAYER_SLOTS],
        cursor_from_h: [0.0; PLAYER_SLOTS],
        cursor_to_x: [0.0; PLAYER_SLOTS],
        cursor_to_y: [0.0; PLAYER_SLOTS],
        cursor_to_w: [0.0; PLAYER_SLOTS],
        cursor_to_h: [0.0; PLAYER_SLOTS],
        cursor_t: [1.0; PLAYER_SLOTS],
        row_tweens,
        pane_transition: PaneTransition::None,
        menu_lr_chord: screen_input::MenuLrChordTracker::default(),
    }
}

pub fn in_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 1.0):
        z(1100):
        linear(TRANSITION_IN_DURATION): alpha(0.0):
        linear(0.0): visible(false)
    );
    (vec![actor], TRANSITION_IN_DURATION)
}

pub fn out_transition() -> (Vec<Actor>, f32) {
    let actor = act!(quad:
        align(0.0, 0.0): xy(0.0, 0.0):
        zoomto(screen_width(), screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.0):
        z(1200):
        linear(TRANSITION_OUT_DURATION): alpha(1.0)
    );
    (vec![actor], TRANSITION_OUT_DURATION)
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
