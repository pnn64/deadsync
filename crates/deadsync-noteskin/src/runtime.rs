use crate::explosion::{
    ITG_TAP_EXPLOSION_WINDOWS, ItgTapExplosionMode, ItgTapExplosionSource,
    itg_hit_mine_command_with_init, itg_mine_explosion_commands,
    itg_partition_tap_explosion_sources, itg_tap_explosion_command_for_window,
    itg_tap_explosion_command_with_init, itg_tap_explosion_key,
    itg_tap_explosion_sources_for_window, parse_explosion_animation,
};
use crate::{
    ExplosionAnimation, NoteAnimPart, NoteColorType, NoteDisplayMetrics, NotePartTextureTranslate,
    ReceptorGlowBehavior, ReceptorPulse, ReceptorReverseBehavior, ReceptorStepBehavior,
    ReceptorStepBehaviors,
};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TapExplosionLayer<T> {
    pub slot: T,
    pub animation: ExplosionAnimation,
}

#[derive(Debug, Clone)]
pub struct TapExplosion<T> {
    pub slot: T,
    pub animation: ExplosionAnimation,
    pub layers: Arc<[TapExplosionLayer<T>]>,
}

impl<T: Clone> TapExplosion<T> {
    pub fn from_single(slot: T, animation: ExplosionAnimation) -> Self {
        Self::from_layers(vec![TapExplosionLayer { slot, animation }])
            .expect("single tap explosion layer must build")
    }

    pub fn from_layers(layers: Vec<TapExplosionLayer<T>>) -> Option<Self> {
        let first = layers.first()?.clone();
        Some(Self {
            slot: first.slot,
            animation: first.animation,
            layers: Arc::from(layers),
        })
    }

    pub fn duration(&self) -> f32 {
        self.layers
            .iter()
            .map(|layer| layer.animation.duration())
            .fold(0.0, f32::max)
    }
}

#[derive(Debug, Clone)]
pub struct HoldVisuals<T> {
    pub head_inactive: Option<T>,
    pub head_active: Option<T>,
    pub head_inactive_layers: Option<Arc<[T]>>,
    pub head_active_layers: Option<Arc<[T]>>,
    pub body_inactive: Option<T>,
    pub body_active: Option<T>,
    pub topcap_inactive: Option<T>,
    pub topcap_active: Option<T>,
    pub bottomcap_inactive: Option<T>,
    pub bottomcap_active: Option<T>,
    pub explosion: Option<T>,
}

#[derive(Debug, Clone)]
pub struct HoldVisualParts<T> {
    pub head_inactive: Option<T>,
    pub head_active: Option<T>,
    pub head_inactive_layers: Option<Arc<[T]>>,
    pub head_active_layers: Option<Arc<[T]>>,
    pub body_inactive: Option<T>,
    pub body_active: Option<T>,
    pub topcap_inactive: Option<T>,
    pub topcap_active: Option<T>,
    pub bottomcap_inactive: Option<T>,
    pub bottomcap_active: Option<T>,
}

#[derive(Debug, Clone)]
pub struct ItgTapNoteColumn<T> {
    pub notes: Vec<T>,
    pub note_layers: Vec<Arc<[T]>>,
    pub layers: Vec<T>,
    pub base: T,
}

impl<T> Default for HoldVisualParts<T> {
    fn default() -> Self {
        Self {
            head_inactive: None,
            head_active: None,
            head_inactive_layers: None,
            head_active_layers: None,
            body_inactive: None,
            body_active: None,
            topcap_inactive: None,
            topcap_active: None,
            bottomcap_inactive: None,
            bottomcap_active: None,
        }
    }
}

impl<T> Default for HoldVisuals<T> {
    fn default() -> Self {
        Self {
            head_inactive: None,
            head_active: None,
            head_inactive_layers: None,
            head_active_layers: None,
            body_inactive: None,
            body_active: None,
            topcap_inactive: None,
            topcap_active: None,
            bottomcap_inactive: None,
            bottomcap_active: None,
            explosion: None,
        }
    }
}

pub fn itg_hold_visuals_from_parts<T: Clone>(parts: HoldVisualParts<T>) -> HoldVisuals<T> {
    let head_active_layers = if parts.head_active.is_some() {
        parts.head_active_layers.clone()
    } else {
        parts
            .head_active_layers
            .clone()
            .or_else(|| parts.head_inactive_layers.clone())
    };
    HoldVisuals {
        head_inactive: parts.head_inactive.clone(),
        head_active: parts.head_active.or(parts.head_inactive),
        head_inactive_layers: parts.head_inactive_layers,
        head_active_layers,
        body_inactive: parts.body_inactive.clone(),
        body_active: parts.body_active.or(parts.body_inactive),
        topcap_inactive: parts.topcap_inactive.clone(),
        topcap_active: parts.topcap_active.or(parts.topcap_inactive),
        bottomcap_inactive: parts.bottomcap_inactive.clone(),
        bottomcap_active: parts.bottomcap_active.or(parts.bottomcap_inactive),
        explosion: None,
    }
}

pub fn itg_roll_visuals_from_parts<T: Clone>(
    parts: HoldVisualParts<T>,
    hold: &HoldVisuals<T>,
) -> HoldVisuals<T> {
    let head_inactive_has_slot = parts.head_inactive.is_some();
    let head_active_has_slot = parts.head_active.is_some();
    let head_inactive_layers = if head_inactive_has_slot {
        parts.head_inactive_layers.clone()
    } else {
        parts
            .head_inactive_layers
            .clone()
            .or_else(|| hold.head_inactive_layers.clone())
    };
    let head_active_layers = if head_active_has_slot {
        parts.head_active_layers.clone()
    } else if head_inactive_has_slot {
        parts.head_inactive_layers.clone()
    } else {
        parts
            .head_active_layers
            .clone()
            .or(parts.head_inactive_layers.clone())
            .or_else(|| hold.head_active_layers.clone())
            .or_else(|| hold.head_inactive_layers.clone())
    };
    HoldVisuals {
        head_inactive: parts.head_inactive.clone().or(hold.head_inactive.clone()),
        head_active: parts
            .head_active
            .or(parts.head_inactive)
            .or(hold.head_active.clone())
            .or(hold.head_inactive.clone()),
        head_inactive_layers,
        head_active_layers,
        body_inactive: parts.body_inactive.clone().or(hold.body_inactive.clone()),
        body_active: parts
            .body_active
            .or(parts.body_inactive)
            .or(hold.body_active.clone())
            .or(hold.body_inactive.clone()),
        topcap_inactive: parts
            .topcap_inactive
            .clone()
            .or(hold.topcap_inactive.clone()),
        topcap_active: parts
            .topcap_active
            .or(parts.topcap_inactive)
            .or(hold.topcap_active.clone())
            .or(hold.topcap_inactive.clone()),
        bottomcap_inactive: parts
            .bottomcap_inactive
            .clone()
            .or(hold.bottomcap_inactive.clone()),
        bottomcap_active: parts
            .bottomcap_active
            .or(parts.bottomcap_inactive)
            .or(hold.bottomcap_active.clone())
            .or(hold.bottomcap_inactive.clone()),
        explosion: None,
    }
}

pub fn default_hold_visuals<T: Clone>(
    hold_columns: &[HoldVisuals<T>],
    roll_columns: &[HoldVisuals<T>],
    down_col: usize,
) -> (HoldVisuals<T>, HoldVisuals<T>) {
    let hold = hold_columns
        .get(down_col)
        .cloned()
        .or_else(|| hold_columns.first().cloned())
        .unwrap_or_default();
    let roll = roll_columns
        .get(down_col)
        .cloned()
        .or_else(|| roll_columns.first().cloned())
        .unwrap_or_else(|| HoldVisuals {
            head_inactive: hold.head_inactive.clone(),
            head_active: hold.head_active.clone(),
            head_inactive_layers: hold.head_inactive_layers.clone(),
            head_active_layers: hold.head_active_layers.clone(),
            body_inactive: hold.body_inactive.clone(),
            body_active: hold.body_active.clone(),
            topcap_inactive: hold.topcap_inactive.clone(),
            topcap_active: hold.topcap_active.clone(),
            bottomcap_inactive: hold.bottomcap_inactive.clone(),
            bottomcap_active: hold.bottomcap_active.clone(),
            explosion: None,
        });
    (hold, roll)
}

pub fn default_tap_explosions<T: Clone>(
    tap_explosions_by_col: &[HashMap<String, TapExplosion<T>>],
    down_col: usize,
) -> HashMap<String, TapExplosion<T>> {
    tap_explosions_by_col
        .get(down_col)
        .filter(|by_window| !by_window.is_empty())
        .cloned()
        .or_else(|| {
            tap_explosions_by_col
                .iter()
                .find(|by_window| !by_window.is_empty())
                .cloned()
        })
        .unwrap_or_default()
}

pub fn itg_is_common_fallback_hold_explosion_key(texture_key: &str) -> bool {
    texture_key
        .to_ascii_lowercase()
        .contains("noteskins/common/common/fallback hold explosion")
}

pub fn itg_is_common_noteskin_key(texture_key: &str) -> bool {
    texture_key
        .to_ascii_lowercase()
        .contains("noteskins/common/common/")
}

pub fn itg_roll_explosion_should_use_hold(roll_key: &str, hold_key: &str) -> bool {
    itg_is_common_fallback_hold_explosion_key(roll_key) && !itg_is_common_noteskin_key(hold_key)
}

pub fn itg_roll_explosion_commands(
    actor_commands: Option<&HashMap<String, String>>,
    mut metric: impl FnMut(&str) -> Option<String>,
) -> Option<HashMap<String, String>> {
    actor_commands.cloned().or_else(|| {
        let mut metric_commands = HashMap::new();
        if let Some(v) = metric("RollOnCommand") {
            metric_commands.insert("rolloncommand".to_string(), v);
        }
        if let Some(v) = metric("RollOffCommand") {
            metric_commands.insert("rolloffcommand".to_string(), v);
        }
        (!metric_commands.is_empty()).then_some(metric_commands)
    })
}

pub fn itg_mine_explosion_from_commands<T: Clone>(
    slot: T,
    commands: &HashMap<String, String>,
) -> Option<TapExplosion<T>> {
    let layers = itg_mine_explosion_commands(commands)
        .into_iter()
        .map(|command_with_init| TapExplosionLayer {
            slot: slot.clone(),
            animation: parse_explosion_animation(&command_with_init),
        })
        .collect();
    TapExplosion::from_layers(layers)
}

pub fn itg_hit_mine_explosion_from_slot<T: Clone>(
    slot: T,
    commands: Option<&HashMap<String, String>>,
    metric_command: Option<String>,
) -> TapExplosion<T> {
    let command = itg_hit_mine_command_with_init(commands, metric_command);
    TapExplosion::from_single(
        slot,
        command
            .as_deref()
            .map(parse_explosion_animation)
            .unwrap_or_default(),
    )
}

pub fn itg_tap_explosion_map_from_sources<T: Clone>(
    sources: impl IntoIterator<Item = ItgTapExplosionSource<T>>,
    mut metric_command: impl FnMut(ItgTapExplosionMode, &str) -> Option<String>,
) -> HashMap<String, TapExplosion<T>> {
    let (dim_sprites, bright_sprites) = itg_partition_tap_explosion_sources(sources);
    if dim_sprites.is_empty() && bright_sprites.is_empty() {
        return HashMap::new();
    }

    let mut tap_explosions = HashMap::new();
    for window in ITG_TAP_EXPLOSION_WINDOWS {
        let key = format!("{}command", window.to_ascii_lowercase());
        for mode in [ItgTapExplosionMode::Dim, ItgTapExplosionMode::Bright] {
            if mode == ItgTapExplosionMode::Bright && bright_sprites.is_empty() {
                continue;
            }
            let sources = itg_tap_explosion_sources_for_window(
                &dim_sprites,
                &bright_sprites,
                window,
                &key,
                mode,
            );
            if sources.is_empty() {
                continue;
            }
            let layers = sources
                .into_iter()
                .filter_map(|source| {
                    let command = itg_tap_explosion_command_for_window(
                        source,
                        window,
                        &key,
                        &mut metric_command,
                    )?;
                    let command_with_init =
                        itg_tap_explosion_command_with_init(source, mode, &command)?;
                    Some(TapExplosionLayer {
                        slot: source.payload.clone(),
                        animation: parse_explosion_animation(&command_with_init),
                    })
                })
                .collect();
            if let Some(explosion) = TapExplosion::from_layers(layers) {
                tap_explosions.insert(itg_tap_explosion_key(window, mode).to_string(), explosion);
            }
        }
    }
    tap_explosions
}

pub fn itg_mine_visuals_from_layers<T: Clone>(
    layers: &[T],
    fallback: Option<T>,
) -> (Option<T>, Option<T>) {
    let fill = layers
        .first()
        .cloned()
        .or_else(|| layers.get(1).cloned())
        .or(fallback);
    let frame = if layers.len() > 1 {
        layers.get(1).cloned()
    } else {
        None
    };
    (fill, frame)
}

pub fn itg_tap_note_layer_priority(has_model: bool, uv_velocity: [f32; 2]) -> u8 {
    if !has_model {
        return 2;
    }
    if uv_velocity[0].abs() > f32::EPSILON || uv_velocity[1].abs() > f32::EPSILON {
        0
    } else {
        1
    }
}

pub fn itg_tap_note_base_layer<T: Clone>(
    layers: &[T],
    mut layer_info: impl FnMut(&T) -> (bool, [f32; 2]),
) -> Option<T> {
    layers
        .iter()
        .find(|layer| {
            let (has_model, uv_velocity) = layer_info(layer);
            has_model
                && (uv_velocity[0].abs() > f32::EPSILON || uv_velocity[1].abs() > f32::EPSILON)
        })
        .cloned()
        .or_else(|| layers.iter().find(|layer| layer_info(layer).0).cloned())
        .or_else(|| layers.first().cloned())
}

pub fn itg_tap_note_column<T: Clone>(
    mut layers: Vec<T>,
    quantizations: usize,
    mut layer_info: impl FnMut(&T) -> (bool, [f32; 2]),
) -> Option<ItgTapNoteColumn<T>> {
    if layers.len() > 1 {
        layers.sort_by_key(|layer| {
            let (has_model, uv_velocity) = layer_info(layer);
            itg_tap_note_layer_priority(has_model, uv_velocity)
        });
    }
    let base = itg_tap_note_base_layer(&layers, &mut layer_info)?;
    let mut notes = Vec::with_capacity(quantizations);
    let mut note_layers = Vec::with_capacity(quantizations);
    for _ in 0..quantizations {
        notes.push(layers.first().cloned().unwrap_or_else(|| base.clone()));
        note_layers.push(Arc::from(layers.clone()));
    }
    Some(ItgTapNoteColumn {
        notes,
        note_layers,
        layers,
        base,
    })
}

pub fn itg_lift_layers_for_col<T: Clone>(lift_layers: Vec<T>, note_layers: &[T]) -> Arc<[T]> {
    if lift_layers.is_empty() {
        Arc::from(note_layers.to_vec())
    } else {
        Arc::from(lift_layers)
    }
}

#[derive(Debug)]
pub struct NoteskinRuntime<T> {
    pub notes: Vec<T>,
    pub note_layers: Vec<Arc<[T]>>,
    pub lift_note_layers: Vec<Arc<[T]>>,
    pub receptor_off: Vec<T>,
    pub receptor_glow: Vec<Option<T>>,
    pub receptor_off_reverse: Vec<ReceptorReverseBehavior>,
    pub receptor_glow_reverse: Vec<ReceptorReverseBehavior>,
    pub receptor_step_behaviors: Vec<ReceptorStepBehaviors>,
    pub mines: Vec<Option<T>>,
    pub mine_fill_slots: Vec<Option<T>>,
    pub mine_frames: Vec<Option<T>>,
    pub column_xs: Vec<i32>,
    pub tap_explosions: HashMap<String, TapExplosion<T>>,
    pub tap_explosions_by_col: Vec<HashMap<String, TapExplosion<T>>>,
    pub mine_hit_explosion: Option<TapExplosion<T>>,
    pub receptor_glow_behavior: ReceptorGlowBehavior,
    pub receptor_pulse: ReceptorPulse,
    pub hold_let_go_gray_percent: f32,
    pub hold_columns: Vec<HoldVisuals<T>>,
    pub roll_columns: Vec<HoldVisuals<T>>,
    pub hold: HoldVisuals<T>,
    pub roll: HoldVisuals<T>,
    pub animation_is_beat_based: bool,
    pub note_display_metrics: NoteDisplayMetrics,
}

impl<T> NoteskinRuntime<T> {
    #[inline(always)]
    pub fn tap_explosion_for_col(&self, col: usize, window: &str) -> Option<&TapExplosion<T>> {
        self.tap_explosion_for_col_with_bright(col, window, false)
    }

    #[inline(always)]
    pub fn tap_explosion_for_col_with_bright(
        &self,
        col: usize,
        window: &str,
        bright: bool,
    ) -> Option<&TapExplosion<T>> {
        if bright
            && let Some(key) = bright_tap_explosion_key(window)
            && let Some(explosion) = self.tap_explosion_for_col_key(col, key)
        {
            return Some(explosion);
        }
        self.tap_explosion_for_col_key(col, window)
    }

    #[inline(always)]
    fn tap_explosion_for_col_key(&self, col: usize, key: &str) -> Option<&TapExplosion<T>> {
        self.tap_explosions_by_col
            .get(col)
            .and_then(|by_window| by_window.get(key))
            .or_else(|| self.tap_explosions.get(key))
    }

    #[inline(always)]
    pub fn for_each_slot(&self, mut visit: impl FnMut(&T)) {
        for slot in &self.notes {
            visit(slot);
        }
        for layer in &self.note_layers {
            for slot in layer.iter() {
                visit(slot);
            }
        }
        for layer in &self.lift_note_layers {
            for slot in layer.iter() {
                visit(slot);
            }
        }
        for slot in &self.receptor_off {
            visit(slot);
        }
        for slot in &self.receptor_glow {
            if let Some(slot) = slot.as_ref() {
                visit(slot);
            }
        }
        for slot in &self.mines {
            if let Some(slot) = slot.as_ref() {
                visit(slot);
            }
        }
        for slot in &self.mine_fill_slots {
            if let Some(slot) = slot.as_ref() {
                visit(slot);
            }
        }
        for slot in &self.mine_frames {
            if let Some(slot) = slot.as_ref() {
                visit(slot);
            }
        }
        for explosion in self.tap_explosions.values() {
            for layer in explosion.layers.iter() {
                visit(&layer.slot);
            }
        }
        for by_col in &self.tap_explosions_by_col {
            for explosion in by_col.values() {
                for layer in explosion.layers.iter() {
                    visit(&layer.slot);
                }
            }
        }
        if let Some(explosion) = self.mine_hit_explosion.as_ref() {
            visit(&explosion.slot);
        }
        let mut visit_hold = |h: &HoldVisuals<T>| {
            for slot in [
                h.head_inactive.as_ref(),
                h.head_active.as_ref(),
                h.body_inactive.as_ref(),
                h.body_active.as_ref(),
                h.topcap_inactive.as_ref(),
                h.topcap_active.as_ref(),
                h.bottomcap_inactive.as_ref(),
                h.bottomcap_active.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                visit(slot);
            }
            for layers in [
                h.head_inactive_layers.as_deref(),
                h.head_active_layers.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                for slot in layers {
                    visit(slot);
                }
            }
            if let Some(slot) = h.explosion.as_ref() {
                visit(slot);
            }
        };
        visit_hold(&self.hold);
        visit_hold(&self.roll);
        for col in &self.hold_columns {
            visit_hold(col);
        }
        for col in &self.roll_columns {
            visit_hold(col);
        }
    }

    #[inline(always)]
    pub fn part_uv_phase(
        &self,
        part: NoteAnimPart,
        song_seconds: f32,
        song_beat: f32,
        note_beat: f32,
    ) -> f32 {
        let anim = self.note_display_metrics.part_animation[part as usize];
        part_uv_phase_inner(
            song_seconds,
            song_beat,
            note_beat,
            anim.length,
            anim.vivid,
            self.animation_is_beat_based,
        )
    }

    #[inline(always)]
    pub fn tap_note_uv_phase(&self, song_seconds: f32, song_beat: f32, note_beat: f32) -> f32 {
        self.part_uv_phase(NoteAnimPart::Tap, song_seconds, song_beat, note_beat)
    }

    #[inline(always)]
    pub fn tap_mine_uv_phase(&self, song_seconds: f32, song_beat: f32, note_beat: f32) -> f32 {
        self.part_uv_phase(NoteAnimPart::Mine, song_seconds, song_beat, note_beat)
    }

    #[inline(always)]
    pub fn part_uv_translation(
        &self,
        part: NoteAnimPart,
        note_beat: f32,
        is_addition: bool,
    ) -> [f32; 2] {
        let metrics = self.note_display_metrics.part_texture_translate[part as usize];
        part_uv_translation_inner(note_beat, metrics, is_addition)
    }

    #[inline(always)]
    pub fn hold_visuals_for_col(&self, col: usize, is_roll: bool) -> &HoldVisuals<T> {
        if is_roll {
            self.roll_columns
                .get(col)
                .or_else(|| self.roll_columns.first())
                .unwrap_or(&self.roll)
        } else {
            self.hold_columns
                .get(col)
                .or_else(|| self.hold_columns.first())
                .unwrap_or(&self.hold)
        }
    }

    #[inline(always)]
    pub fn hold_explosion_for_col(&self, col: usize, is_roll: bool) -> Option<&T> {
        self.hold_visuals_for_col(col, is_roll)
            .explosion
            .as_ref()
            .or_else(|| {
                if is_roll {
                    self.roll.explosion.as_ref()
                } else {
                    self.hold.explosion.as_ref()
                }
            })
    }

    #[inline(always)]
    pub fn receptor_step_behavior_for_col(
        &self,
        col: usize,
        window: Option<&str>,
    ) -> ReceptorStepBehavior {
        self.receptor_step_behaviors
            .get(col)
            .copied()
            .or_else(|| self.receptor_step_behaviors.first().copied())
            .unwrap_or_default()
            .for_window(window)
    }
}

#[inline(always)]
pub fn bright_tap_explosion_key(window: &str) -> Option<&'static str> {
    match window {
        "W1" => Some("W1Bright"),
        "W2" => Some("W2Bright"),
        "W3" => Some("W3Bright"),
        "W4" => Some("W4Bright"),
        "W5" => Some("W5Bright"),
        "Held" => Some("HeldBright"),
        _ => None,
    }
}

#[inline(always)]
fn part_uv_phase_inner(
    song_seconds: f32,
    song_beat: f32,
    note_beat: f32,
    length: f32,
    vivid: bool,
    beat_based: bool,
) -> f32 {
    let length = length.max(1e-6);
    let clock = if beat_based { song_beat } else { song_seconds };
    let mut phase = clock.rem_euclid(length) / length;
    if vivid {
        let note_fraction = note_beat.rem_euclid(1.0);
        let vivid_interval = 1.0 / length;
        let vivid_offset = (note_fraction / vivid_interval).floor() * vivid_interval;
        phase = (phase + vivid_offset).rem_euclid(1.0);
    }
    phase
}

#[inline(always)]
fn part_uv_translation_inner(
    note_beat: f32,
    metrics: NotePartTextureTranslate,
    is_addition: bool,
) -> [f32; 2] {
    let count = metrics.note_color_count.max(1);
    let countf = count as f32;
    let color = match metrics.note_color_type {
        NoteColorType::Denominator => {
            let note_type = beat_to_note_type_index(note_beat) as f32;
            note_type.clamp(0.0, (count - 1) as f32)
        }
        NoteColorType::Progress => (note_beat * countf).ceil() % countf,
        NoteColorType::ProgressAlternate => {
            let mut scaled = note_beat * countf;
            if scaled - (scaled as i64 as f32) == 0.0 {
                scaled += countf - 1.0;
            }
            scaled.ceil() % countf
        }
    };
    let add = if is_addition {
        metrics.addition_offset
    } else {
        [0.0, 0.0]
    };
    [
        metrics.note_color_spacing[0].mul_add(color, add[0]),
        metrics.note_color_spacing[1].mul_add(color, add[1]),
    ]
}

#[inline(always)]
fn beat_to_note_type_index(beat: f32) -> i32 {
    let row = (beat * 48.0).round() as i32;
    if row.rem_euclid(48) == 0 {
        0
    } else if row.rem_euclid(24) == 0 {
        1
    } else if row.rem_euclid(16) == 0 {
        2
    } else if row.rem_euclid(12) == 0 {
        3
    } else if row.rem_euclid(8) == 0 {
        4
    } else if row.rem_euclid(6) == 0 {
        5
    } else if row.rem_euclid(4) == 0 {
        6
    } else if row.rem_euclid(3) == 0 {
        7
    } else {
        8
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HoldVisualParts, HoldVisuals, NoteskinRuntime, TapExplosion, TapExplosionLayer,
        bright_tap_explosion_key, default_hold_visuals, default_tap_explosions,
        itg_hit_mine_explosion_from_slot, itg_hold_visuals_from_parts,
        itg_is_common_fallback_hold_explosion_key, itg_is_common_noteskin_key,
        itg_lift_layers_for_col, itg_mine_explosion_from_commands, itg_mine_visuals_from_layers,
        itg_roll_explosion_commands, itg_roll_explosion_should_use_hold,
        itg_roll_visuals_from_parts, itg_tap_explosion_map_from_sources, itg_tap_note_base_layer,
        itg_tap_note_column, itg_tap_note_layer_priority,
    };
    use crate::explosion::ItgTapExplosionSource;
    use crate::{
        ExplosionAnimation, ExplosionSegment, ExplosionState, NoteAnimPart, NoteDisplayMetrics,
        NotePartAnimation, NotePartTextureTranslate, ReceptorStepBehavior, ReceptorStepBehaviors,
        TweenType,
    };
    use std::collections::HashMap;
    use std::sync::Arc;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Slot(u8);

    #[test]
    fn hold_visuals_default_does_not_require_default_slot() {
        let visuals = HoldVisuals::<Slot>::default();
        assert!(visuals.head_inactive.is_none());
        assert!(visuals.head_active.is_none());
        assert!(visuals.explosion.is_none());
    }

    #[test]
    fn tap_explosion_duration_uses_longest_layer() {
        let short = ExplosionAnimation {
            initial: ExplosionState::default(),
            segments: vec![ExplosionSegment {
                duration: 0.25,
                tween: TweenType::Linear,
                start: ExplosionState::default(),
                end_zoom: Some(1.0),
                end_color: Some([1.0, 1.0, 1.0, 0.0]),
                end_rotation_z: None,
                end_visible: None,
            }],
            glow: None,
            blend_add: false,
        };
        let long = ExplosionAnimation {
            initial: ExplosionState::default(),
            segments: vec![ExplosionSegment {
                duration: 0.75,
                tween: TweenType::Linear,
                start: ExplosionState::default(),
                end_zoom: Some(1.0),
                end_color: Some([1.0, 1.0, 1.0, 0.0]),
                end_rotation_z: None,
                end_visible: None,
            }],
            glow: None,
            blend_add: false,
        };
        let explosion = TapExplosion::from_layers(vec![
            TapExplosionLayer {
                slot: Slot(1),
                animation: short,
            },
            TapExplosionLayer {
                slot: Slot(2),
                animation: long,
            },
        ])
        .expect("layers should build tap explosion");

        assert_eq!(explosion.slot, Slot(1));
        assert!((explosion.duration() - 0.75).abs() <= f32::EPSILON);
    }

    #[test]
    fn noteskin_runtime_prefers_column_bright_tap_explosions() {
        let dim = TapExplosion::from_single(Slot(1), ExplosionAnimation::default());
        let bright = TapExplosion::from_single(Slot(2), ExplosionAnimation::default());
        let mut by_col = HashMap::new();
        by_col.insert("W1Bright".to_string(), bright);
        let runtime = NoteskinRuntime {
            tap_explosions: HashMap::from([("W1".to_string(), dim)]),
            tap_explosions_by_col: vec![by_col],
            ..empty_runtime()
        };

        let explosion = runtime
            .tap_explosion_for_col_with_bright(0, "W1", true)
            .expect("bright column explosion should resolve");

        assert_eq!(explosion.slot, Slot(2));
        assert_eq!(bright_tap_explosion_key("Held"), Some("HeldBright"));
        assert_eq!(bright_tap_explosion_key("Miss"), None);
    }

    #[test]
    fn noteskin_runtime_falls_back_to_default_hold_visuals() {
        let runtime = NoteskinRuntime {
            hold: HoldVisuals {
                body_inactive: Some(Slot(1)),
                ..HoldVisuals::default()
            },
            roll_columns: vec![HoldVisuals {
                body_inactive: Some(Slot(2)),
                ..HoldVisuals::default()
            }],
            ..empty_runtime()
        };

        assert_eq!(
            runtime.hold_visuals_for_col(4, false).body_inactive,
            Some(Slot(1))
        );
        assert_eq!(
            runtime.hold_visuals_for_col(4, true).body_inactive,
            Some(Slot(2))
        );
    }

    #[test]
    fn noteskin_runtime_selects_column_or_default_hold_explosion() {
        let runtime = NoteskinRuntime {
            hold: HoldVisuals {
                explosion: Some(Slot(1)),
                ..HoldVisuals::default()
            },
            roll: HoldVisuals {
                explosion: Some(Slot(2)),
                ..HoldVisuals::default()
            },
            hold_columns: vec![HoldVisuals {
                explosion: Some(Slot(3)),
                ..HoldVisuals::default()
            }],
            ..empty_runtime()
        };

        assert_eq!(runtime.hold_explosion_for_col(0, false), Some(&Slot(3)));
        assert_eq!(runtime.hold_explosion_for_col(4, false), Some(&Slot(3)));
        assert_eq!(runtime.hold_explosion_for_col(0, true), Some(&Slot(2)));
    }

    #[test]
    fn default_hold_visuals_prefer_down_then_first_and_hold_for_roll() {
        let hold_columns = vec![
            HoldVisuals {
                body_inactive: Some(Slot(1)),
                ..HoldVisuals::default()
            },
            HoldVisuals {
                body_inactive: Some(Slot(2)),
                ..HoldVisuals::default()
            },
        ];
        let (hold, roll) = default_hold_visuals(&hold_columns, &[], 1);

        assert_eq!(hold.body_inactive, Some(Slot(2)));
        assert_eq!(roll.body_inactive, Some(Slot(2)));
    }

    #[test]
    fn hold_visuals_from_parts_fall_back_active_to_inactive() {
        let inactive_layers: Arc<[Slot]> = Arc::from([Slot(10)]);
        let hold = itg_hold_visuals_from_parts(HoldVisualParts {
            head_inactive: Some(Slot(1)),
            head_inactive_layers: Some(Arc::clone(&inactive_layers)),
            body_inactive: Some(Slot(2)),
            topcap_inactive: Some(Slot(3)),
            bottomcap_inactive: Some(Slot(4)),
            ..HoldVisualParts::default()
        });

        assert_eq!(hold.head_active, Some(Slot(1)));
        assert_eq!(hold.body_active, Some(Slot(2)));
        assert_eq!(hold.topcap_active, Some(Slot(3)));
        assert_eq!(hold.bottomcap_active, Some(Slot(4)));
        assert_eq!(hold.head_active_layers, Some(inactive_layers));
    }

    #[test]
    fn roll_visuals_from_parts_fall_back_to_hold_visuals() {
        let hold = HoldVisuals {
            head_inactive: Some(Slot(1)),
            head_active: Some(Slot(2)),
            head_inactive_layers: Some(Arc::from([Slot(11)])),
            head_active_layers: Some(Arc::from([Slot(12)])),
            body_inactive: Some(Slot(3)),
            body_active: Some(Slot(4)),
            topcap_inactive: Some(Slot(5)),
            topcap_active: Some(Slot(6)),
            bottomcap_inactive: Some(Slot(7)),
            bottomcap_active: Some(Slot(8)),
            explosion: None,
        };

        let roll = itg_roll_visuals_from_parts(
            HoldVisualParts {
                body_inactive: Some(Slot(30)),
                ..HoldVisualParts::default()
            },
            &hold,
        );

        assert_eq!(roll.head_inactive, Some(Slot(1)));
        assert_eq!(roll.head_active, Some(Slot(2)));
        assert_eq!(roll.body_inactive, Some(Slot(30)));
        assert_eq!(roll.body_active, Some(Slot(30)));
        assert_eq!(roll.topcap_active, Some(Slot(6)));
        assert_eq!(roll.bottomcap_active, Some(Slot(8)));
    }

    #[test]
    fn default_tap_explosions_prefer_down_then_first_nonempty() {
        let first = TapExplosion::from_single(Slot(1), ExplosionAnimation::default());
        let down = TapExplosion::from_single(Slot(2), ExplosionAnimation::default());

        let selected = default_tap_explosions(
            &[
                HashMap::from([("W1".to_string(), first.clone())]),
                HashMap::from([("W1".to_string(), down.clone())]),
            ],
            1,
        );
        assert_eq!(
            selected.get("W1").map(|explosion| explosion.slot.clone()),
            Some(Slot(2))
        );

        let selected = default_tap_explosions(
            &[HashMap::from([("W1".to_string(), first)]), HashMap::new()],
            1,
        );
        assert_eq!(
            selected.get("W1").map(|explosion| explosion.slot.clone()),
            Some(Slot(1))
        );
    }

    #[test]
    fn common_noteskin_key_classification_is_case_insensitive() {
        assert!(itg_is_common_fallback_hold_explosion_key(
            "NoteSkins/common/common/Fallback Hold Explosion.png"
        ));
        assert!(itg_is_common_noteskin_key(
            "noteskins/common/common/Fallback Receptor.png"
        ));
        assert!(!itg_is_common_noteskin_key(
            "noteskins/dance/default/Down Hold Explosion.png"
        ));
    }

    #[test]
    fn roll_explosion_prefers_skin_hold_over_common_hold_fallback() {
        assert!(itg_roll_explosion_should_use_hold(
            "NoteSkins/common/common/Fallback Hold Explosion.png",
            "NoteSkins/dance/default/Down Hold Explosion.png",
        ));
        assert!(!itg_roll_explosion_should_use_hold(
            "NoteSkins/dance/default/Down Roll Explosion.png",
            "NoteSkins/dance/default/Down Hold Explosion.png",
        ));
        assert!(!itg_roll_explosion_should_use_hold(
            "NoteSkins/common/common/Fallback Hold Explosion.png",
            "NoteSkins/common/common/Fallback Hold Explosion.png",
        ));
    }

    #[test]
    fn roll_explosion_commands_prefer_actor_then_metrics() {
        let actor_commands =
            HashMap::from([("rolloncommand".to_string(), "diffusealpha,1".to_string())]);
        let commands = itg_roll_explosion_commands(Some(&actor_commands), |_| {
            Some("diffusealpha,0".to_string())
        })
        .expect("actor commands should be selected");
        assert_eq!(commands, actor_commands);

        let commands = itg_roll_explosion_commands(None, |key| match key {
            "RollOnCommand" => Some("diffusealpha,1".to_string()),
            "RollOffCommand" => Some("diffusealpha,0".to_string()),
            _ => None,
        })
        .expect("metric commands should be selected");
        assert_eq!(
            commands.get("rolloncommand").map(String::as_str),
            Some("diffusealpha,1")
        );
        assert_eq!(
            commands.get("rolloffcommand").map(String::as_str),
            Some("diffusealpha,0")
        );

        assert!(itg_roll_explosion_commands(None, |_| None).is_none());
    }

    #[test]
    fn itg_mine_explosion_builds_layers_from_actor_commands() {
        let explosion = itg_mine_explosion_from_commands(
            Slot(7),
            &HashMap::from([
                (
                    "ecommand".to_string(),
                    "diffusealpha,1;linear,0.2;diffusealpha,0".to_string(),
                ),
                ("initcommand".to_string(), "zoom,0.5".to_string()),
            ]),
        )
        .expect("actor mine command should build explosion");

        assert_eq!(explosion.slot, Slot(7));
        assert_eq!(explosion.layers.len(), 1);
        assert_eq!(explosion.animation.initial.zoom, 0.5);
        assert!((explosion.duration() - 0.2).abs() <= f32::EPSILON);
    }

    #[test]
    fn mine_visuals_use_first_layer_fill_and_second_layer_frame() {
        let (fill, frame) = itg_mine_visuals_from_layers(&[Slot(1), Slot(2)], Some(Slot(9)));

        assert_eq!(fill, Some(Slot(1)));
        assert_eq!(frame, Some(Slot(2)));
    }

    #[test]
    fn mine_visuals_use_fallback_when_layers_are_empty() {
        let (fill, frame) = itg_mine_visuals_from_layers(&[], Some(Slot(9)));

        assert_eq!(fill, Some(Slot(9)));
        assert_eq!(frame, None);
    }

    #[test]
    fn tap_note_layer_priority_prefers_moving_model_layers() {
        assert_eq!(itg_tap_note_layer_priority(true, [0.0, 1.0]), 0);
        assert_eq!(itg_tap_note_layer_priority(true, [0.0, 0.0]), 1);
        assert_eq!(itg_tap_note_layer_priority(false, [1.0, 0.0]), 2);
    }

    #[test]
    fn tap_note_base_layer_prefers_moving_model_then_model_then_first() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        struct Layer {
            id: u8,
            has_model: bool,
            uv: [i32; 2],
        }

        let layers = [
            Layer {
                id: 1,
                has_model: false,
                uv: [9, 0],
            },
            Layer {
                id: 2,
                has_model: true,
                uv: [0, 0],
            },
            Layer {
                id: 3,
                has_model: true,
                uv: [0, 1],
            },
        ];
        let info = |layer: &Layer| (layer.has_model, [layer.uv[0] as f32, layer.uv[1] as f32]);

        let moving_model = itg_tap_note_base_layer(&layers, info);
        let static_model = itg_tap_note_base_layer(&layers[..2], info);
        let first_sprite = itg_tap_note_base_layer(&layers[..1], info);

        assert_eq!(moving_model.map(|layer| layer.id), Some(3));
        assert_eq!(static_model.map(|layer| layer.id), Some(2));
        assert_eq!(first_sprite.map(|layer| layer.id), Some(1));
    }

    #[test]
    fn tap_note_column_sorts_layers_and_expands_quantizations() {
        #[derive(Clone, Debug, PartialEq)]
        struct Layer {
            id: u8,
            has_model: bool,
            uv_velocity: [f32; 2],
        }

        let column = itg_tap_note_column(
            vec![
                Layer {
                    id: 2,
                    has_model: true,
                    uv_velocity: [0.0, 0.0],
                },
                Layer {
                    id: 3,
                    has_model: false,
                    uv_velocity: [0.0, 0.0],
                },
                Layer {
                    id: 1,
                    has_model: true,
                    uv_velocity: [0.5, 0.0],
                },
            ],
            2,
            |layer| (layer.has_model, layer.uv_velocity),
        )
        .expect("tap note column");

        assert_eq!(column.base.id, 1);
        assert_eq!(
            column
                .layers
                .iter()
                .map(|layer| layer.id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(column.notes, vec![column.layers[0].clone(); 2]);
        assert_eq!(&*column.note_layers[0], column.layers.as_slice());
        assert_eq!(&*column.note_layers[1], column.layers.as_slice());
    }

    #[test]
    fn lift_layers_fall_back_to_note_layers() {
        let note_layers = [Slot(1), Slot(2)];
        let fallback = itg_lift_layers_for_col(Vec::new(), &note_layers);
        let explicit = itg_lift_layers_for_col(vec![Slot(3)], &note_layers);

        assert_eq!(&*fallback, &note_layers);
        assert_eq!(&*explicit, &[Slot(3)]);
    }

    #[test]
    fn itg_hit_mine_explosion_uses_source_or_metric_command() {
        let source = HashMap::from([
            ("initcommand".to_string(), "zoom,0.5".to_string()),
            (
                "hitminecommand".to_string(),
                "linear,0.2;diffusealpha,0".to_string(),
            ),
        ]);
        let explosion = itg_hit_mine_explosion_from_slot(
            Slot(3),
            Some(&source),
            Some("linear,0.9;diffusealpha,0".to_string()),
        );

        assert_eq!(explosion.slot, Slot(3));
        assert_eq!(explosion.animation.initial.zoom, 0.5);
        assert!((explosion.duration() - 0.2).abs() <= f32::EPSILON);

        let metric = itg_hit_mine_explosion_from_slot(
            Slot(4),
            None,
            Some("linear,0.4;diffusealpha,0".to_string()),
        );
        assert_eq!(metric.slot, Slot(4));
        assert!((metric.duration() - 0.4).abs() <= f32::EPSILON);
    }

    #[test]
    fn itg_tap_explosion_map_builds_dim_and_bright_windows() {
        let map = itg_tap_explosion_map_from_sources(
            [
                ItgTapExplosionSource::new(
                    "Tap Explosion Dim".to_string(),
                    Slot(1),
                    HashMap::from([(
                        "w1command".to_string(),
                        "zoom,0.5;linear,0.2;diffusealpha,0".to_string(),
                    )]),
                ),
                ItgTapExplosionSource::new(
                    "Tap Explosion Bright".to_string(),
                    Slot(2),
                    HashMap::from([(
                        "w1command".to_string(),
                        "zoom,0.75;linear,0.3;diffusealpha,0".to_string(),
                    )]),
                ),
            ],
            |_, _| None,
        );

        assert_eq!(
            map.get("W1").map(|explosion| explosion.slot.clone()),
            Some(Slot(1))
        );
        assert_eq!(
            map.get("W1Bright").map(|explosion| explosion.slot.clone()),
            Some(Slot(2))
        );
        assert_eq!(map["W1"].layers.len(), 2);
        assert!((map["W1"].duration() - 0.3).abs() <= f32::EPSILON);
        assert!((map["W1Bright"].duration() - 0.3).abs() <= f32::EPSILON);
    }

    #[test]
    fn noteskin_runtime_samples_uv_phase_and_translation() {
        let mut metrics = NoteDisplayMetrics::default();
        metrics.part_animation[NoteAnimPart::Tap as usize] = NotePartAnimation {
            length: 2.0,
            vivid: false,
        };
        metrics.part_texture_translate[NoteAnimPart::Tap as usize] = NotePartTextureTranslate {
            addition_offset: [1.0, 2.0],
            note_color_spacing: [0.25, 0.5],
            note_color_count: 8,
            ..NotePartTextureTranslate::default()
        };
        let runtime = NoteskinRuntime {
            animation_is_beat_based: true,
            note_display_metrics: metrics,
            ..empty_runtime()
        };

        assert!((runtime.tap_note_uv_phase(7.0, 3.0, 0.0) - 0.5).abs() <= f32::EPSILON);
        assert_eq!(
            runtime.part_uv_translation(NoteAnimPart::Tap, 0.25, true),
            [1.75, 3.5]
        );
    }

    #[test]
    fn noteskin_runtime_visits_nested_slots() {
        let runtime = NoteskinRuntime {
            notes: vec![Slot(1)],
            note_layers: vec![Arc::from([Slot(2)])],
            receptor_glow: vec![Some(Slot(3))],
            hold: HoldVisuals {
                head_active_layers: Some(Arc::from([Slot(4), Slot(5)])),
                explosion: Some(Slot(6)),
                ..HoldVisuals::default()
            },
            ..empty_runtime()
        };
        let mut visited = Vec::new();

        runtime.for_each_slot(|slot| visited.push(slot.0));

        assert_eq!(visited, [1, 2, 3, 4, 5, 6]);
    }

    fn empty_runtime() -> NoteskinRuntime<Slot> {
        NoteskinRuntime {
            notes: Vec::new(),
            note_layers: Vec::new(),
            lift_note_layers: Vec::new(),
            receptor_off: Vec::new(),
            receptor_glow: Vec::new(),
            receptor_off_reverse: Vec::new(),
            receptor_glow_reverse: Vec::new(),
            receptor_step_behaviors: vec![ReceptorStepBehaviors::new(
                ReceptorStepBehavior {
                    duration: 0.3,
                    zoom_start: 1.0,
                    zoom_end: 2.0,
                    tween: TweenType::Linear,
                    interrupts: true,
                },
                ReceptorStepBehavior::identity(),
                [ReceptorStepBehavior::identity(); 5],
            )],
            mines: Vec::new(),
            mine_fill_slots: Vec::new(),
            mine_frames: Vec::new(),
            column_xs: Vec::new(),
            tap_explosions: HashMap::new(),
            tap_explosions_by_col: Vec::new(),
            mine_hit_explosion: None,
            receptor_glow_behavior: Default::default(),
            receptor_pulse: Default::default(),
            hold_let_go_gray_percent: 0.25,
            hold_columns: Vec::new(),
            roll_columns: Vec::new(),
            hold: HoldVisuals::default(),
            roll: HoldVisuals::default(),
            animation_is_beat_based: false,
            note_display_metrics: NoteDisplayMetrics::default(),
        }
    }
}
