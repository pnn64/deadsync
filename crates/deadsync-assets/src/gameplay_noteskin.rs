//! Game-owned noteskin selection, component composition, and feedback timing.

use crate::noteskin::{self, Noteskin};
use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    GameplayNoteskinData, GameplayNoteskinEffects, GameplayReceptorGlowBehavior,
    GameplayReceptorStepBehavior, GameplayTween, MINE_EXPLOSION_DURATION, RECEPTOR_STEP_WINDOWS,
    TAP_EXPLOSION_WINDOWS,
};
use deadsync_noteskin::{ReceptorGlowBehavior, ReceptorStepBehavior, Style, TweenType};
use deadsync_profile as profile_data;
use std::path::Path;
use std::sync::Arc;

/// Prepared noteskin components shared by gameplay feedback and any theme's
/// notefield presentation for the lifetime of a song.
#[derive(Clone)]
pub struct GameplayNoteskinAssets {
    pub noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
    pub mine_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
    pub receptor_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
    pub tap_explosion_noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS],
}

impl GameplayNoteskinAssets {
    /// Base, mine, receptor and tap-explosion components for a runtime player.
    #[must_use]
    pub fn components_for_player(&self, player: usize) -> [Option<&Noteskin>; 4] {
        [
            self.noteskin[player].as_deref(),
            self.mine_noteskin[player].as_deref(),
            self.receptor_noteskin[player].as_deref(),
            self.tap_explosion_noteskin[player].as_deref(),
        ]
    }

    /// Prepare deterministic feedback from the selected components without
    /// exposing asset loading or noteskin runtime types to the gameplay core.
    pub fn gameplay_data(
        &self,
        cols_per_player: usize,
        num_players: usize,
        runtime_profiles: &[profile_data::Profile; MAX_PLAYERS],
    ) -> GameplayNoteskinData {
        GameplayNoteskinData {
            effects: noteskin_effects_from_assets(
                self,
                runtime_profiles,
                num_players,
                cols_per_player,
            ),
        }
    }
}

impl Default for GameplayNoteskinAssets {
    fn default() -> Self {
        Self {
            noteskin: std::array::from_fn(|_| None),
            mine_noteskin: std::array::from_fn(|_| None),
            receptor_noteskin: std::array::from_fn(|_| None),
            tap_explosion_noteskin: std::array::from_fn(|_| None),
        }
    }
}

fn noteskin_effects_from_assets(
    assets: &GameplayNoteskinAssets,
    runtime_profiles: &[profile_data::Profile; MAX_PLAYERS],
    num_players: usize,
    cols_per_player: usize,
) -> GameplayNoteskinEffects {
    let mut effects = GameplayNoteskinEffects::default();
    let cols = cols_per_player.min(deadsync_core::input::MAX_COLS);
    for player in 0..num_players.min(MAX_PLAYERS) {
        let receptor_ns = assets.receptor_noteskin[player]
            .as_deref()
            .or_else(|| assets.noteskin[player].as_deref());
        if let Some(ns) = receptor_ns {
            effects.set_receptor_glow_behavior(
                player,
                gameplay_receptor_glow_behavior(ns.receptor_glow_behavior),
            );
            for col in 0..cols {
                for window in RECEPTOR_STEP_WINDOWS {
                    effects.set_receptor_step_behavior(
                        player,
                        col,
                        window,
                        gameplay_receptor_step_behavior(
                            ns.receptor_step_behavior_for_col(col, window),
                        ),
                    );
                }
            }
        }

        let tap_ns = if runtime_profiles[player].tap_explosion_noteskin_hidden() {
            None
        } else {
            assets.tap_explosion_noteskin[player]
                .as_deref()
                .or_else(|| assets.noteskin[player].as_deref())
        };
        if let Some(ns) = tap_ns {
            for col in 0..cols {
                for window in TAP_EXPLOSION_WINDOWS {
                    for bright in [false, true] {
                        effects.set_tap_explosion_duration(
                            player,
                            col,
                            window,
                            bright,
                            ns.tap_explosion_for_col_with_bright(col, window, bright)
                                .map(deadsync_noteskin::TapExplosion::duration),
                        );
                    }
                }
            }
        }

        let mine_duration = assets.mine_noteskin[player]
            .as_deref()
            .or_else(|| assets.noteskin[player].as_deref())
            .and_then(|ns| ns.mine_hit_explosion.as_ref())
            .map_or(
                MINE_EXPLOSION_DURATION,
                deadsync_noteskin::TapExplosion::duration,
            );
        effects.set_mine_explosion_duration(player, mine_duration);
    }
    effects
}

#[inline(always)]
const fn gameplay_tween(tween: TweenType) -> GameplayTween {
    match tween {
        TweenType::Linear => GameplayTween::Linear,
        TweenType::Accelerate => GameplayTween::Accelerate,
        TweenType::Decelerate => GameplayTween::Decelerate,
        TweenType::Smooth => GameplayTween::Smooth,
    }
}

#[inline(always)]
const fn gameplay_receptor_glow_behavior(
    behavior: ReceptorGlowBehavior,
) -> GameplayReceptorGlowBehavior {
    GameplayReceptorGlowBehavior {
        press_duration: behavior.press_duration,
        press_alpha_start: behavior.press_alpha_start,
        press_alpha_end: behavior.press_alpha_end,
        press_zoom_start: behavior.press_zoom_start,
        press_zoom_end: behavior.press_zoom_end,
        press_tween: gameplay_tween(behavior.press_tween),
        lift_finishes_press: behavior.lift_finishes_press,
        lift_interrupts_press: behavior.lift_interrupts_press,
        lift_alpha_start: behavior.lift_alpha_start,
        lift_zoom_start: behavior.lift_zoom_start,
        duration: behavior.duration,
        alpha_start: behavior.alpha_start,
        alpha_end: behavior.alpha_end,
        zoom_start: behavior.zoom_start,
        zoom_end: behavior.zoom_end,
        tween: gameplay_tween(behavior.tween),
        blend_add: behavior.blend_add,
    }
}

#[inline(always)]
const fn gameplay_receptor_step_behavior(
    behavior: ReceptorStepBehavior,
) -> GameplayReceptorStepBehavior {
    GameplayReceptorStepBehavior {
        duration: behavior.duration,
        zoom_start: behavior.zoom_start,
        zoom_end: behavior.zoom_end,
        tween: gameplay_tween(behavior.tween),
        interrupts: behavior.interrupts,
    }
}

/// Resolve chart overrides and player components once on the song-load worker.
/// Cached skins stay immutable; per-play receptor timing uses copy-on-write.
pub fn gameplay_noteskin_assets(
    cols_per_player: usize,
    num_players: usize,
    runtime_profiles: &[profile_data::Profile; MAX_PLAYERS],
    song_dir: &Path,
    song_skins: &[Option<String>; MAX_PLAYERS],
    first_beat: f32,
) -> GameplayNoteskinAssets {
    gameplay_noteskin_assets_with_loaders(
        cols_per_player,
        num_players,
        runtime_profiles,
        song_dir,
        song_skins,
        first_beat,
        noteskin::load_itg_skin_cached,
        noteskin::load_song_skin,
    )
}

fn gameplay_noteskin_assets_with_loaders(
    cols_per_player: usize,
    num_players: usize,
    runtime_profiles: &[profile_data::Profile; MAX_PLAYERS],
    song_dir: &Path,
    song_skins: &[Option<String>; MAX_PLAYERS],
    first_beat: f32,
    mut load_itg: impl FnMut(&Style, &str) -> Result<Arc<Noteskin>, String>,
    mut load_song: impl FnMut(&Style, &Path, &str, &str) -> Result<Arc<Noteskin>, String>,
) -> GameplayNoteskinAssets {
    use deadsync_noteskin::runtime::SkinPart;
    let style = Style {
        num_cols: cols_per_player,
        num_players: 1,
    };
    // Compose once on the song-load worker. The selected slots then share the
    // existing prewarm, upload and song-lifetime ownership paths.
    let mut noteskin: [Option<Arc<Noteskin>>; MAX_PLAYERS] = std::array::from_fn(|player| {
        if player >= num_players {
            return None;
        }
        if let Some(skin) = &song_skins[player] {
            match load_song(
                &style,
                song_dir,
                skin,
                &runtime_profiles[player].scroll_option.to_string(),
            ) {
                Ok(skin) => return Some(skin),
                Err(error) => log::warn!(
                    "Cannot load chart noteskin '{skin}': {error}; keeping the player's skin"
                ),
            }
        }
        let profile = &runtime_profiles[player];
        let mut options = profile.current_player_options();
        profile_data::migrate_noteskin_parts(&mut options);
        let skin = options.noteskin.as_str();
        let mut result = load_itg(&style, skin)
            .or_else(|error| {
                log::warn!("Cannot load noteskin '{skin}': {error}; using the bundled default");
                load_itg(&style, profile_data::NoteSkin::DEFAULT_NAME)
            })
            .ok()?;
        for (part, selection) in [
            (SkinPart::Arrows, &options.arrow_noteskin),
            (SkinPart::Receptors, &options.receptor_noteskin),
            (SkinPart::HoldActive, &options.hold_active_noteskin),
            (SkinPart::HoldInactive, &options.hold_inactive_noteskin),
            (SkinPart::RollActive, &options.roll_active_noteskin),
            (SkinPart::RollInactive, &options.roll_inactive_noteskin),
            (SkinPart::TapExplosions, &options.tap_explosion_noteskin),
            (SkinPart::HoldExplosions, &options.hold_explosion_noteskin),
            (SkinPart::Mines, &options.mine_noteskin),
            (SkinPart::Lifts, &options.lift_noteskin),
        ] {
            let Some(selection) = selection else { continue };
            if selection.is_none_choice() {
                continue;
            }
            match load_itg(&style, selection.as_str()) {
                Ok(source) => Arc::make_mut(&mut result).apply_part(&source, part),
                Err(error) => log::warn!(
                    "Cannot load {part:?} from '{selection}': {error}; using the base component"
                ),
            }
        }
        Some(result)
    });
    for skin in noteskin.iter_mut().flatten() {
        if skin
            .receptor_off
            .iter()
            .any(|slot| slot.beat_receptor_start.is_some())
        {
            for slot in &mut Arc::make_mut(skin).receptor_off {
                if slot.beat_receptor_start.is_some() {
                    slot.beat_receptor_start = Some(first_beat - 8.0);
                }
            }
        }
    }
    let tap_explosion_noteskin = std::array::from_fn(|player| {
        if runtime_profiles[player].tap_explosion_noteskin_hidden() {
            None
        } else {
            noteskin[player].clone()
        }
    });
    GameplayNoteskinAssets {
        mine_noteskin: noteskin.clone(),
        receptor_noteskin: noteskin.clone(),
        tap_explosion_noteskin,
        noteskin,
    }
}

#[cfg(test)]
mod tests;
