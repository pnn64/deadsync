use crate::explosion::{
    ITG_TAP_EXPLOSION_WINDOWS, ItgTapExplosionMode, ItgTapExplosionSource,
    itg_direct_tap_explosion_layers, itg_has_hit_mine_command, itg_has_tap_explosion_command,
    itg_hit_mine_command_with_init, itg_hit_mine_explosion_slot, itg_hold_explosion_slot,
    itg_is_hit_mine_explosion_element, itg_mine_explosion_commands,
    itg_partition_tap_explosion_sources, itg_tap_explosion_command_for_window,
    itg_tap_explosion_command_with_init, itg_tap_explosion_key,
    itg_tap_explosion_sources_for_window, parse_explosion_animation,
};
use crate::script::{itg_active_model_commands, model_draw_program};
use crate::{
    ExplosionAnimation, ModelDrawState, ModelEffectState, ModelTweenSegment, NoteAnimPart,
    NoteColorType, NoteDisplayMetrics, NotePartTextureTranslate, ReceptorGlowBehavior,
    ReceptorPulse, ReceptorReverseBehavior, ReceptorStepBehavior, ReceptorStepBehaviors,
};
use crate::{actor, compiled, itg, receptor};
use std::collections::{HashMap, HashSet};
use std::path::Path;
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

#[derive(Debug, Clone)]
pub struct ItgResolvedSprite<T> {
    pub element: String,
    pub slot: T,
    pub commands: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ItgReceptorColumn<T> {
    pub off: T,
    pub glow: Option<T>,
    pub off_reverse: ReceptorReverseBehavior,
    pub glow_reverse: ReceptorReverseBehavior,
    pub step_behaviors: ReceptorStepBehaviors,
    pub pulse_command: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItgHoldKind {
    Hold,
    Roll,
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

impl ItgHoldKind {
    pub const fn head_inactive(self) -> &'static str {
        match self {
            Self::Hold => "Hold Head Inactive",
            Self::Roll => "Roll Head Inactive",
        }
    }

    pub const fn head_active(self) -> &'static str {
        match self {
            Self::Hold => "Hold Head Active",
            Self::Roll => "Roll Head Active",
        }
    }

    pub const fn body_inactive(self) -> &'static str {
        match self {
            Self::Hold => "Hold Body Inactive",
            Self::Roll => "Roll Body Inactive",
        }
    }

    pub const fn body_active(self) -> &'static str {
        match self {
            Self::Hold => "Hold Body Active",
            Self::Roll => "Roll Body Active",
        }
    }

    pub const fn topcap_inactive(self) -> &'static str {
        match self {
            Self::Hold => "Hold TopCap Inactive",
            Self::Roll => "Roll TopCap Inactive",
        }
    }

    pub const fn topcap_active(self) -> &'static str {
        match self {
            Self::Hold => "Hold TopCap Active",
            Self::Roll => "Roll TopCap Active",
        }
    }

    pub const fn bottomcap_inactive(self) -> &'static str {
        match self {
            Self::Hold => "Hold BottomCap Inactive",
            Self::Roll => "Roll BottomCap Inactive",
        }
    }

    pub const fn bottomcap_active(self) -> &'static str {
        match self {
            Self::Hold => "Hold BottomCap Active",
            Self::Roll => "Roll BottomCap Active",
        }
    }
}

pub fn itg_hold_visual_parts<T>(
    kind: ItgHoldKind,
    mut maps_head_to_tap: impl FnMut(&str) -> bool,
    mut resolve_head: impl FnMut(&str) -> (Option<T>, Option<Arc<[T]>>),
    mut resolve_single: impl FnMut(&str) -> Option<T>,
) -> HoldVisualParts<T> {
    let (head_inactive, head_inactive_layers) = if maps_head_to_tap(kind.head_inactive()) {
        (None, None)
    } else {
        resolve_head(kind.head_inactive())
    };
    let (head_active, head_active_layers) = if maps_head_to_tap(kind.head_active()) {
        (None, None)
    } else {
        resolve_head(kind.head_active())
    };
    HoldVisualParts {
        head_inactive,
        head_active,
        head_inactive_layers,
        head_active_layers,
        body_inactive: resolve_single(kind.body_inactive()),
        body_active: resolve_single(kind.body_active()),
        topcap_inactive: resolve_single(kind.topcap_inactive()),
        topcap_active: resolve_single(kind.topcap_active()),
        bottomcap_inactive: resolve_single(kind.bottomcap_inactive()),
        bottomcap_active: resolve_single(kind.bottomcap_active()),
    }
}

pub fn itg_hold_head_layers<T: Clone>(layers: Vec<T>) -> (Option<T>, Option<Arc<[T]>>) {
    match layers.len() {
        0 => (None, None),
        1 => (layers.into_iter().next(), None),
        _ => (layers.first().cloned(), Some(Arc::from(layers))),
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

pub fn itg_roll_explosion_from_resolved<T: Clone>(
    roll_blank: bool,
    roll_slot: Option<T>,
    hold_slot: Option<T>,
    mut texture_key: impl FnMut(&T) -> String,
    actor_commands: Option<&HashMap<String, String>>,
    metric: impl FnMut(&str) -> Option<String>,
    mut apply_commands: impl FnMut(&T, &HashMap<String, String>, &str) -> T,
) -> Option<T> {
    if roll_blank {
        return None;
    }
    let Some(roll_slot) = roll_slot else {
        return hold_slot;
    };
    let Some(hold_slot) = hold_slot else {
        return Some(roll_slot);
    };
    if !itg_roll_explosion_should_use_hold(&texture_key(&roll_slot), &texture_key(&hold_slot)) {
        return Some(roll_slot);
    }

    itg_roll_explosion_commands(actor_commands, metric)
        .map(|commands| apply_commands(&hold_slot, &commands, "rolloncommand"))
        .or(Some(hold_slot))
}

pub fn itg_roll_explosion_from_resolved_layers<T: Clone>(
    wrapper_layers: &[ItgResolvedSprite<T>],
    roll_blank: bool,
    roll_slot: Option<T>,
    hold_slot: Option<T>,
    texture_key: impl FnMut(&T) -> String,
    metric: impl FnMut(&str) -> Option<String>,
    apply_commands: impl FnMut(&T, &HashMap<String, String>, &str) -> T,
) -> Option<T> {
    let actor_commands = wrapper_layers
        .iter()
        .find(|sprite| sprite.commands.contains_key("rolloncommand"))
        .or_else(|| {
            wrapper_layers
                .iter()
                .find(|sprite| actor::element_contains_hint(&sprite.element, "roll explosion"))
        })
        .filter(|sprite| sprite.commands.contains_key("rolloncommand"))
        .map(|sprite| &sprite.commands);
    itg_roll_explosion_from_resolved(
        roll_blank,
        roll_slot,
        hold_slot,
        texture_key,
        actor_commands,
        metric,
        apply_commands,
    )
}

pub fn itg_hold_explosion_from_resolved_layers<T: Clone>(
    wrapper_layers: &[ItgResolvedSprite<T>],
    source_layers: &[ItgResolvedSprite<T>],
    active_key: &str,
    element_hint: &str,
    blank: bool,
    fallback_slot: Option<T>,
    direct_slot: impl FnMut() -> Option<T>,
    wrapped_slots: impl FnMut() -> Vec<T>,
    mut apply_commands: impl FnMut(T, &HashMap<String, String>, &str) -> T,
) -> Option<T> {
    itg_hold_explosion_slot(
        wrapper_layers,
        source_layers,
        active_key,
        element_hint,
        blank,
        fallback_slot,
        |sprite, key| sprite.commands.contains_key(key),
        |sprite, hint| actor::element_contains_hint(&sprite.element, hint),
        |sprite| sprite.slot.clone(),
        |slot, sprite, key| apply_commands(slot, &sprite.commands, key),
        direct_slot,
        wrapped_slots,
    )
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

pub fn itg_tap_explosion_map_from_layers<L, T: Clone>(
    explosion_layers: &[L],
    mut layer_has_tap_command: impl FnMut(&L) -> bool,
    mut direct_layers: impl FnMut(ItgTapExplosionMode) -> Vec<L>,
    mut source_from_layer: impl FnMut(&L) -> ItgTapExplosionSource<T>,
    metric_command: impl FnMut(ItgTapExplosionMode, &str) -> Option<String>,
) -> HashMap<String, TapExplosion<T>> {
    let actor_sources = explosion_layers
        .iter()
        .filter(|layer| layer_has_tap_command(layer))
        .map(&mut source_from_layer)
        .collect::<Vec<_>>();

    let (direct_dim_sources, direct_bright_sources) = if actor_sources.is_empty() {
        (
            direct_layers(ItgTapExplosionMode::Dim)
                .iter()
                .map(&mut source_from_layer)
                .collect::<Vec<_>>(),
            direct_layers(ItgTapExplosionMode::Bright)
                .iter()
                .map(&mut source_from_layer)
                .collect::<Vec<_>>(),
        )
    } else {
        (Vec::new(), Vec::new())
    };

    itg_tap_explosion_map_from_sources(
        actor_sources
            .into_iter()
            .chain(direct_dim_sources)
            .chain(direct_bright_sources),
        metric_command,
    )
}

pub fn itg_tap_explosion_map_from_resolved_layers<T: Clone>(
    explosion_layers: &[ItgResolvedSprite<T>],
    mut direct_layers: impl FnMut(&str) -> Vec<ItgResolvedSprite<T>>,
    metric_command: impl FnMut(ItgTapExplosionMode, &str) -> Option<String>,
) -> HashMap<String, TapExplosion<T>> {
    itg_tap_explosion_map_from_layers(
        explosion_layers,
        |sprite| itg_has_tap_explosion_command(&sprite.commands),
        |mode| {
            let base_element = match mode {
                ItgTapExplosionMode::Dim => "Tap Explosion Dim",
                ItgTapExplosionMode::Bright => "Tap Explosion Bright",
            };
            direct_layers(base_element)
        },
        |sprite| {
            ItgTapExplosionSource::new(
                sprite.element.clone(),
                sprite.slot.clone(),
                sprite.commands.clone(),
            )
        },
        metric_command,
    )
}

pub fn itg_direct_tap_explosion_resolved_layers<T>(
    base_element: &str,
    base_blank: bool,
    is_blank: impl FnMut(&str) -> bool,
    resolve_element: impl FnMut(&str) -> Vec<ItgResolvedSprite<T>>,
) -> Vec<ItgResolvedSprite<T>> {
    itg_direct_tap_explosion_layers(base_element, base_blank, is_blank, resolve_element)
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

pub fn itg_hit_mine_explosion_from_layers<T: Clone>(
    layers: &[ItgResolvedSprite<T>],
    direct_fallback: impl FnMut() -> Option<T>,
    actor_fallback: impl FnMut() -> Option<T>,
    metric_command: Option<String>,
) -> Option<TapExplosion<T>> {
    let (source, slot) = itg_hit_mine_explosion_slot(
        layers,
        |sprite| itg_has_hit_mine_command(&sprite.commands),
        |sprite| itg_is_hit_mine_explosion_element(&sprite.element),
        |sprite| sprite.slot.clone(),
        direct_fallback,
        actor_fallback,
    );
    source
        .and_then(|source| itg_mine_explosion_from_commands(source.slot.clone(), &source.commands))
        .or_else(|| {
            slot.map(|slot| {
                itg_hit_mine_explosion_from_slot(
                    slot,
                    source.map(|sprite| &sprite.commands),
                    metric_command,
                )
            })
        })
}

pub fn itg_tap_note_layers<T>(mut layers: Vec<T>, fallback: impl FnOnce() -> Option<T>) -> Vec<T> {
    if layers.is_empty()
        && let Some(fallback) = fallback()
    {
        layers.push(fallback);
    }
    layers
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

pub fn itg_resolved_slots_with_model_draw<T>(
    sprites: impl IntoIterator<Item = ItgResolvedSprite<T>>,
    mut apply: impl FnMut(&mut T, ModelDrawState, Arc<[ModelTweenSegment]>, ModelEffectState),
) -> Vec<T> {
    sprites
        .into_iter()
        .map(|mut sprite| {
            let (draw, timeline, effect) = model_draw_program(&sprite.commands);
            apply(&mut sprite.slot, draw, timeline, effect);
            sprite.slot
        })
        .collect()
}

pub fn itg_apply_loader_command<T>(
    sprites: &mut [ItgResolvedSprite<T>],
    command: Option<&str>,
    mut apply: impl FnMut(&mut T, &str),
) {
    let Some(command) = command.filter(|cmd| !cmd.trim().is_empty()) else {
        return;
    };
    for sprite in sprites {
        apply(&mut sprite.slot, command);
    }
}

pub fn itg_slot_with_active_model_draw<T: Clone>(
    slot: &T,
    commands: &HashMap<String, String>,
    active_key: &str,
    mut apply: impl FnMut(&mut T, ModelDrawState, Arc<[ModelTweenSegment]>, ModelEffectState),
) -> T {
    let mut out = slot.clone();
    let scripted = itg_active_model_commands(commands, active_key);
    let (draw, timeline, effect) = model_draw_program(&scripted);
    apply(&mut out, draw, timeline, effect);
    out
}

pub fn itg_first_resolved_slot_or_fallback<T>(
    sprites: impl IntoIterator<Item = ItgResolvedSprite<T>>,
    blank: bool,
    fallback: impl FnOnce() -> Option<T>,
) -> Option<T> {
    sprites
        .into_iter()
        .next()
        .map(|sprite| sprite.slot)
        .or_else(|| (!blank).then(fallback).flatten())
}

pub fn itg_first_actor_sprite_slot<T>(
    data: &itg::NoteskinData,
    compiled_actors: &compiled::CompiledActors,
    path: &Path,
    mut load_texture: impl FnMut(&Path) -> Option<T>,
    mut load_frame: impl FnMut(&Path, usize) -> Option<T>,
    mut load_animated: impl FnMut(
        &Path,
        usize,
        usize,
        Option<&[usize]>,
        Option<&[f32]>,
        bool,
    ) -> Option<T>,
) -> Option<T> {
    if !actor::is_lua_path(path) {
        return load_texture(path);
    }

    let decl = compiled_actors.decl_for_path(&data.search_dirs, path)?;
    let default_anim_is_beat = itg::animation_is_beat_based(data);
    for sprite in decl.sprites {
        let texture_path = itg::resolve_texture_expr(data, &sprite.texture_expr, None)?;
        let anim_is_beat =
            crate::script::sprite_animation_is_beat_based(&sprite.commands, default_anim_is_beat);
        let slot = if sprite.frame_count > 1 {
            load_animated(
                &texture_path,
                sprite.frame0,
                sprite.frame_count,
                sprite.frame_indices.as_deref(),
                sprite.frame_delays.as_deref(),
                anim_is_beat,
            )
            .or_else(|| load_frame(&texture_path, sprite.frame0))
        } else {
            load_frame(&texture_path, sprite.frame0)
        }
        .or_else(|| load_texture(&texture_path));
        if let Some(slot) = slot {
            return Some(slot);
        }
    }
    None
}

pub fn itg_resolve_actor_sprites_compiled<T>(
    data: &itg::NoteskinData,
    compiled: &compiled::CompiledLoader,
    button: &str,
    element: &str,
    mut resolve_file: impl FnMut(
        &Path,
        Option<i32>,
        usize,
        &mut HashSet<String>,
        Option<&Path>,
    ) -> Vec<ItgResolvedSprite<T>>,
    mut apply_loader_command: impl FnMut(&mut [ItgResolvedSprite<T>], Option<&str>),
) -> Vec<ItgResolvedSprite<T>> {
    let mut visiting = HashSet::new();
    itg_resolve_actor_sprites_inner_compiled(
        data,
        compiled,
        button,
        element,
        0,
        &mut visiting,
        &mut resolve_file,
        &mut apply_loader_command,
    )
}

pub fn itg_resolve_actor_sprites_inner_compiled<T>(
    data: &itg::NoteskinData,
    compiled: &compiled::CompiledLoader,
    button: &str,
    element: &str,
    depth: usize,
    visiting: &mut HashSet<String>,
    mut resolve_file: impl FnMut(
        &Path,
        Option<i32>,
        usize,
        &mut HashSet<String>,
        Option<&Path>,
    ) -> Vec<ItgResolvedSprite<T>>,
    mut apply_loader_command: impl FnMut(&mut [ItgResolvedSprite<T>], Option<&str>),
) -> Vec<ItgResolvedSprite<T>> {
    if depth > compiled::ACTOR_RECURSION_MAX_DEPTH {
        log::warn!("noteskin lua actor recursion depth exceeded at '{button} {element}'");
        return Vec::new();
    }

    let visit_key = compiled::actor_visit_key(button, element);
    if !visiting.insert(visit_key.clone()) {
        log::warn!("noteskin lua actor recursion loop detected at '{button} {element}'");
        return Vec::new();
    }

    let request = compiled.load_request(button, element);
    if request.blank {
        visiting.remove(&visit_key);
        return Vec::new();
    }
    let path = data.resolve_path(&request.load_button, &request.load_element);
    let Some(path) = path else {
        visiting.remove(&visit_key);
        return Vec::new();
    };

    let mut out = resolve_file(&path, request.rotation_z, depth, visiting, None);
    apply_loader_command(&mut out, request.init_command.as_deref());

    visiting.remove(&visit_key);
    out
}

pub fn itg_receptor_column<T: Clone>(
    layers: &[ItgResolvedSprite<T>],
    metrics: &itg::IniData,
    receptor_fallback: impl FnOnce() -> Option<T>,
    rflash_fallback: impl FnOnce() -> Option<T>,
    glow_fallback: impl FnOnce() -> Option<T>,
    mut apply_init: impl FnMut(&mut T, &str),
    mut base_zoom: impl FnMut(&T) -> f32,
) -> Option<ItgReceptorColumn<T>> {
    let layer_commands = layers
        .iter()
        .map(|sprite| &sprite.commands)
        .collect::<Vec<_>>();
    let receptor_slots = layers
        .iter()
        .map(|sprite| sprite.slot.clone())
        .collect::<Vec<_>>();
    let visuals = receptor::itg_receptor_visuals(
        &receptor_slots,
        receptor_fallback,
        rflash_fallback,
        glow_fallback,
    );
    let mut off = visuals.off?;
    let receptor_commands = layer_commands.first().copied();
    if let Some(init_command) = receptor_commands.and_then(|commands| commands.get("initcommand")) {
        apply_init(&mut off, init_command);
    }
    let step_behaviors =
        receptor::receptor_step_behaviors(metrics, receptor_commands, base_zoom(&off));
    let (off_reverse, glow_reverse) = receptor::itg_receptor_reverse_behaviors(&layer_commands);
    Some(ItgReceptorColumn {
        off,
        glow: visuals.glow,
        off_reverse,
        glow_reverse,
        step_behaviors,
        pulse_command: receptor::itg_receptor_pulse_command(&layer_commands).map(str::to_string),
    })
}

pub fn itg_receptor_glow_behavior_from_layers<T>(
    layers: &[ItgResolvedSprite<T>],
    metric_command: impl FnMut(&str) -> Option<String>,
) -> ReceptorGlowBehavior {
    receptor::receptor_glow_behavior(layers.get(1).map(|sprite| &sprite.commands), metric_command)
}

pub fn itg_receptor_pulse_from_command(command: Option<&str>) -> ReceptorPulse {
    command
        .map(receptor::receptor_pulse_from_script)
        .unwrap_or_default()
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
        HoldVisualParts, HoldVisuals, ItgHoldKind, ItgResolvedSprite, NoteskinRuntime,
        TapExplosion, TapExplosionLayer, bright_tap_explosion_key, default_hold_visuals,
        default_tap_explosions, itg_apply_loader_command, itg_direct_tap_explosion_resolved_layers,
        itg_first_actor_sprite_slot, itg_first_resolved_slot_or_fallback,
        itg_hit_mine_explosion_from_layers, itg_hit_mine_explosion_from_slot,
        itg_hold_explosion_from_resolved_layers, itg_hold_head_layers, itg_hold_visual_parts,
        itg_hold_visuals_from_parts, itg_is_common_fallback_hold_explosion_key,
        itg_is_common_noteskin_key, itg_lift_layers_for_col, itg_mine_explosion_from_commands,
        itg_mine_visuals_from_layers, itg_receptor_column, itg_receptor_glow_behavior_from_layers,
        itg_receptor_pulse_from_command, itg_resolve_actor_sprites_compiled,
        itg_resolved_slots_with_model_draw, itg_roll_explosion_commands,
        itg_roll_explosion_from_resolved, itg_roll_explosion_from_resolved_layers,
        itg_roll_explosion_should_use_hold, itg_roll_visuals_from_parts,
        itg_slot_with_active_model_draw, itg_tap_explosion_map_from_layers,
        itg_tap_explosion_map_from_resolved_layers, itg_tap_explosion_map_from_sources,
        itg_tap_note_base_layer, itg_tap_note_column, itg_tap_note_layer_priority,
        itg_tap_note_layers,
    };
    use crate::explosion::{
        ItgTapExplosionMode, ItgTapExplosionSource, itg_has_tap_explosion_command,
    };
    use crate::{
        ExplosionAnimation, ExplosionSegment, ExplosionState, NoteAnimPart, NoteDisplayMetrics,
        NotePartAnimation, NotePartTextureTranslate, ReceptorStepBehavior, ReceptorStepBehaviors,
        TweenType, compiled, itg,
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
    fn hold_visual_parts_skips_head_to_tap_mapped_heads() {
        let parts = itg_hold_visual_parts(
            ItgHoldKind::Hold,
            |element| element == "Hold Head Inactive",
            |element| {
                assert!(element.starts_with("Hold Head"));
                (Some(Slot(1)), Some(Arc::from([Slot(10)])))
            },
            |element| Some(Slot(element.len() as u8)),
        );

        assert_eq!(parts.head_inactive, None);
        assert_eq!(parts.head_inactive_layers, None);
        assert_eq!(parts.head_active, Some(Slot(1)));
        assert_eq!(
            parts.body_inactive,
            Some(Slot("Hold Body Inactive".len() as u8))
        );
        assert_eq!(
            parts.topcap_active,
            Some(Slot("Hold TopCap Active".len() as u8))
        );
    }

    #[test]
    fn hold_visual_parts_uses_roll_element_names() {
        let mut singles = Vec::new();
        let parts = itg_hold_visual_parts(
            ItgHoldKind::Roll,
            |_| false,
            |element| {
                assert!(element.starts_with("Roll Head"));
                (Some(Slot(element.len() as u8)), None)
            },
            |element| {
                singles.push(element.to_string());
                Some(Slot(element.len() as u8))
            },
        );

        assert_eq!(
            parts.head_inactive,
            Some(Slot("Roll Head Inactive".len() as u8))
        );
        assert_eq!(
            singles,
            vec![
                "Roll Body Inactive",
                "Roll Body Active",
                "Roll TopCap Inactive",
                "Roll TopCap Active",
                "Roll BottomCap Inactive",
                "Roll BottomCap Active",
            ]
        );
    }

    #[test]
    fn hold_head_layers_keep_stack_only_for_multi_layer_heads() {
        assert_eq!(itg_hold_head_layers::<Slot>(Vec::new()), (None, None));
        assert_eq!(itg_hold_head_layers(vec![Slot(1)]), (Some(Slot(1)), None));

        let (head, layers) = itg_hold_head_layers(vec![Slot(1), Slot(2)]);

        assert_eq!(head, Some(Slot(1)));
        assert_eq!(layers.as_deref(), Some(&[Slot(1), Slot(2)][..]));
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
    fn roll_explosion_from_resolved_respects_blank_and_fallback_policy() {
        let actor_commands =
            HashMap::from([("rolloncommand".to_string(), "diffusealpha,1".to_string())]);

        assert_eq!(
            itg_roll_explosion_from_resolved::<Slot>(
                true,
                Some(Slot(1)),
                Some(Slot(2)),
                |_| String::new(),
                Some(&actor_commands),
                |_| None,
                |slot, _, _| slot.clone(),
            ),
            None
        );

        let selected = itg_roll_explosion_from_resolved(
            false,
            Some(Slot(1)),
            Some(Slot(2)),
            |slot| match slot.0 {
                1 => "noteskins/common/common/Fallback Hold Explosion.png".to_string(),
                2 => "noteskins/dance/default/Down Hold Explosion.png".to_string(),
                _ => String::new(),
            },
            Some(&actor_commands),
            |_| None,
            |slot, commands, key| {
                assert_eq!(slot, &Slot(2));
                assert!(commands.contains_key(key));
                Slot(9)
            },
        );

        assert_eq!(selected, Some(Slot(9)));

        let selected = itg_roll_explosion_from_resolved(
            false,
            Some(Slot(1)),
            Some(Slot(2)),
            |slot| match slot.0 {
                1 => "noteskins/dance/default/Down Roll Explosion.png".to_string(),
                2 => "noteskins/dance/default/Down Hold Explosion.png".to_string(),
                _ => String::new(),
            },
            Some(&actor_commands),
            |_| None,
            |slot, _, _| slot.clone(),
        );

        assert_eq!(selected, Some(Slot(1)));
    }

    #[test]
    fn roll_explosion_from_resolved_layers_selects_wrapper_commands() {
        let wrapper = [ItgResolvedSprite {
            element: "Roll Explosion".to_string(),
            slot: Slot(7),
            commands: HashMap::from([(
                "rolloncommand".to_string(),
                "linear,0.2;diffusealpha,0".to_string(),
            )]),
        }];

        let selected = itg_roll_explosion_from_resolved_layers(
            &wrapper,
            false,
            Some(Slot(1)),
            Some(Slot(2)),
            |slot| match slot.0 {
                1 => "noteskins/common/common/Fallback Hold Explosion.png".to_string(),
                2 => "noteskins/dance/default/Down Hold Explosion.png".to_string(),
                _ => String::new(),
            },
            |_| None,
            |slot, commands, key| {
                assert_eq!(slot, &Slot(2));
                assert!(commands.contains_key(key));
                Slot(12)
            },
        );

        assert_eq!(selected, Some(Slot(12)));
    }

    #[test]
    fn hold_explosion_from_resolved_layers_uses_wrapper_command_policy() {
        let wrapper = [ItgResolvedSprite {
            element: "Explosion".to_string(),
            slot: Slot(1),
            commands: HashMap::from([(
                "holdingoncommand".to_string(),
                "linear,0.2;diffusealpha,0".to_string(),
            )]),
        }];
        let source = [ItgResolvedSprite {
            element: "Hold Explosion".to_string(),
            slot: Slot(2),
            commands: HashMap::new(),
        }];

        let selected = itg_hold_explosion_from_resolved_layers(
            &wrapper,
            &source,
            "holdingoncommand",
            "hold explosion",
            false,
            None,
            || panic!("direct fallback should be lazy"),
            || panic!("wrapped fallback should be lazy"),
            |slot, commands, key| {
                assert!(commands.contains_key(key));
                Slot(slot.0 + 10)
            },
        );

        assert_eq!(selected, Some(Slot(11)));
    }

    #[test]
    fn hold_explosion_from_resolved_layers_uses_root_fallbacks() {
        let selected = itg_hold_explosion_from_resolved_layers(
            &[],
            &[],
            "holdingoncommand",
            "hold explosion",
            false,
            Some(Slot(9)),
            || Some(Slot(3)),
            || panic!("wrapped fallback should be lazy after direct slot"),
            |slot, _, _| slot,
        );
        assert_eq!(selected, Some(Slot(3)));

        let blank = itg_hold_explosion_from_resolved_layers(
            &[],
            &[],
            "holdingoncommand",
            "hold explosion",
            true,
            Some(Slot(9)),
            || Some(Slot(3)),
            || vec![Slot(4)],
            |slot, _, _| slot,
        );
        assert_eq!(blank, None);
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
    fn tap_note_layers_only_use_fallback_when_empty() {
        let layers = itg_tap_note_layers(vec![Slot(1)], || panic!("fallback should be lazy"));
        assert_eq!(layers, vec![Slot(1)]);

        let layers = itg_tap_note_layers(Vec::new(), || Some(Slot(2)));
        assert_eq!(layers, vec![Slot(2)]);

        let layers = itg_tap_note_layers::<Slot>(Vec::new(), || None);
        assert!(layers.is_empty());
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
    fn resolved_slots_apply_model_draw_programs() {
        let sprites = vec![ItgResolvedSprite {
            element: "Tap Note".to_string(),
            slot: 7,
            commands: HashMap::from([("initcommand".to_string(), "zoom,2".to_string())]),
        }];

        let slots = itg_resolved_slots_with_model_draw(sprites, |slot, draw, _, _| {
            *slot += draw.zoom[0] as i32;
        });

        assert_eq!(slots, vec![9]);
    }

    #[test]
    fn loader_command_applies_nonempty_command_to_slots() {
        let mut sprites = vec![
            ItgResolvedSprite {
                element: "Tap Note".to_string(),
                slot: 1,
                commands: HashMap::new(),
            },
            ItgResolvedSprite {
                element: "Tap Mine".to_string(),
                slot: 2,
                commands: HashMap::new(),
            },
        ];

        itg_apply_loader_command(&mut sprites, Some("zoom,3"), |slot, command| {
            *slot += command.len() as i32;
        });
        assert_eq!(
            sprites.iter().map(|sprite| sprite.slot).collect::<Vec<_>>(),
            vec![7, 8]
        );

        itg_apply_loader_command(&mut sprites, Some("   "), |slot, _| *slot += 10);
        assert_eq!(
            sprites.iter().map(|sprite| sprite.slot).collect::<Vec<_>>(),
            vec![7, 8]
        );
    }

    #[test]
    fn active_model_draw_helper_applies_selected_command() {
        let commands = HashMap::from([
            ("initcommand".to_string(), "zoom,2".to_string()),
            ("holdingoncommand".to_string(), "zoom,4".to_string()),
        ]);

        let slot = itg_slot_with_active_model_draw(
            &1,
            &commands,
            "holdingoncommand",
            |slot, draw, _, _| {
                *slot += draw.zoom[0] as i32;
            },
        );

        assert_eq!(slot, 5);
    }

    #[test]
    fn first_resolved_slot_precedes_fallback_and_blank_suppresses_fallback() {
        let sprites = vec![ItgResolvedSprite {
            element: "Hold Body Active".to_string(),
            slot: 4,
            commands: HashMap::new(),
        }];

        assert_eq!(
            itg_first_resolved_slot_or_fallback(sprites, false, || Some(9)),
            Some(4)
        );
        assert_eq!(
            itg_first_resolved_slot_or_fallback(
                Vec::<ItgResolvedSprite<i32>>::new(),
                false,
                || { Some(9) }
            ),
            Some(9)
        );
        assert_eq!(
            itg_first_resolved_slot_or_fallback(Vec::<ItgResolvedSprite<i32>>::new(), true, || {
                Some(9)
            }),
            None
        );
    }

    #[test]
    fn actor_sprite_resolution_applies_loader_request_policy() {
        let root = std::env::temp_dir().join(format!(
            "deadsync-actor-loader-policy-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let search_dir = root.join("dance").join("default");
        std::fs::create_dir_all(&search_dir).unwrap();
        let actor_path = search_dir.join("Up Tap Note.lua");
        std::fs::write(&actor_path, []).unwrap();

        let data = itg::NoteskinData {
            name: "default".to_string(),
            metrics: itg::IniData::default(),
            search_dirs: vec![search_dir],
        };
        let loader = compiled::CompiledLoader {
            version: compiled::CACHE_SCHEMA_VERSION,
            game: "dance".to_string(),
            skin: "default".to_string(),
            entries: vec![compiled::CompiledLoaderEntry {
                button: "Left".to_string(),
                element: "Tap Note".to_string(),
                load_button: "Up".to_string(),
                load_element: "Tap Note".to_string(),
                blank: false,
                rotation_z: Some(90),
                init_command: Some("zoom,2".to_string()),
            }],
        };

        let sprites = itg_resolve_actor_sprites_compiled::<i32>(
            &data,
            &loader,
            "Left",
            "Tap Note",
            |path, rotation_z, depth, visiting, arg0_path| {
                assert_eq!(path, actor_path.as_path());
                assert_eq!(rotation_z, Some(90));
                assert_eq!(depth, 0);
                assert!(arg0_path.is_none());
                assert!(visiting.contains(&compiled::actor_visit_key("Left", "Tap Note")));
                vec![ItgResolvedSprite {
                    element: "Tap Note".to_string(),
                    slot: 7,
                    commands: HashMap::new(),
                }]
            },
            |sprites, command| {
                assert_eq!(command, Some("zoom,2"));
                for sprite in sprites {
                    sprite
                        .commands
                        .insert("initcommand".to_string(), command.unwrap().to_string());
                }
            },
        );

        assert_eq!(sprites.len(), 1);
        assert_eq!(sprites[0].slot, 7);
        assert_eq!(
            sprites[0].commands.get("initcommand").map(String::as_str),
            Some("zoom,2")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn actor_sprite_resolution_suppresses_blank_requests() {
        let data = itg::NoteskinData {
            name: "default".to_string(),
            metrics: itg::IniData::default(),
            search_dirs: Vec::new(),
        };
        let loader = compiled::CompiledLoader {
            version: compiled::CACHE_SCHEMA_VERSION,
            game: "dance".to_string(),
            skin: "default".to_string(),
            entries: vec![compiled::CompiledLoaderEntry {
                button: "Left".to_string(),
                element: "Tap Note".to_string(),
                load_button: String::new(),
                load_element: String::new(),
                blank: true,
                rotation_z: None,
                init_command: Some("zoom,2".to_string()),
            }],
        };

        let sprites = itg_resolve_actor_sprites_compiled::<i32>(
            &data,
            &loader,
            "Left",
            "Tap Note",
            |_, _, _, _, _| panic!("blank loader requests should not resolve files"),
            |_, _| panic!("blank loader requests should not apply commands"),
        );

        assert!(sprites.is_empty());
    }

    #[test]
    fn first_actor_sprite_slot_uses_texture_loader_for_non_lua_paths() {
        let data = crate::itg::NoteskinData {
            name: "default".to_string(),
            metrics: crate::itg::IniData::default(),
            search_dirs: Vec::new(),
        };
        let actors = crate::compiled::CompiledActors::default();
        let slot = itg_first_actor_sprite_slot(
            &data,
            &actors,
            std::path::Path::new("Tap Note.png"),
            |path| Some(format!("texture:{}", path.display())),
            |_, _| panic!("frame loader should not run"),
            |_, _, _, _, _, _| panic!("animated loader should not run"),
        );

        assert_eq!(slot.as_deref(), Some("texture:Tap Note.png"));
    }

    #[test]
    fn first_actor_sprite_slot_uses_compiled_animation_metadata() {
        #[derive(Debug, PartialEq)]
        enum Loaded {
            Texture,
            Frame,
            Animated {
                path: std::path::PathBuf,
                frame0: usize,
                frame_count: usize,
                frame_indices: Option<Vec<usize>>,
                frame_delays: Option<Vec<f32>>,
                beat_based: bool,
            },
        }

        let root = std::env::temp_dir().join(format!(
            "deadsync-first-actor-sprite-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let search_dir = root.join("dance").join("default");
        std::fs::create_dir_all(&search_dir).unwrap();
        let texture_path = search_dir.join("Tap Note.png");
        std::fs::write(&texture_path, []).unwrap();
        let actor_path = search_dir.join("Down Tap Note.lua");
        let data = crate::itg::NoteskinData {
            name: "default".to_string(),
            metrics: crate::itg::IniData::default(),
            search_dirs: vec![search_dir],
        };
        let actors = crate::compiled::CompiledActors {
            version: crate::compiled::CACHE_SCHEMA_VERSION,
            files: vec![crate::compiled::CompiledActorFile {
                key: "dance/default/down tap note.lua".to_string(),
                decl: crate::actor::ItgLuaActorDecl {
                    sprites: vec![crate::actor::ItgLuaSpriteDecl {
                        texture_expr: "\"Tap Note.png\"".to_string(),
                        frame0: 2,
                        frame_count: 4,
                        frame_indices: Some(vec![2, 3, 4, 5]),
                        frame_delays: Some(vec![0.1, 0.2, 0.3, 0.4]),
                        commands: HashMap::new(),
                    }],
                    ..crate::actor::ItgLuaActorDecl::default()
                },
            }],
        };

        let slot = itg_first_actor_sprite_slot(
            &data,
            &actors,
            &actor_path,
            |_| Some(Loaded::Texture),
            |_, _| Some(Loaded::Frame),
            |path, frame0, frame_count, frame_indices, frame_delays, beat_based| {
                Some(Loaded::Animated {
                    path: path.to_path_buf(),
                    frame0,
                    frame_count,
                    frame_indices: frame_indices.map(<[usize]>::to_vec),
                    frame_delays: frame_delays.map(<[f32]>::to_vec),
                    beat_based,
                })
            },
        );

        assert_eq!(
            slot,
            Some(Loaded::Animated {
                path: texture_path,
                frame0: 2,
                frame_count: 4,
                frame_indices: Some(vec![2, 3, 4, 5]),
                frame_delays: Some(vec![0.1, 0.2, 0.3, 0.4]),
                beat_based: false,
            })
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn receptor_column_applies_init_and_builds_column_policy() {
        let metrics = crate::itg::IniData::default();
        let layers = vec![
            ItgResolvedSprite {
                element: "Receptor".to_string(),
                slot: Slot(1),
                commands: HashMap::from([
                    ("initcommand".to_string(), "zoom,2".to_string()),
                    (
                        "reverseoffcommand".to_string(),
                        "linear,0.1;vertalign,top".to_string(),
                    ),
                ]),
            },
            ItgResolvedSprite {
                element: "Receptor".to_string(),
                slot: Slot(2),
                commands: HashMap::from([(
                    "reverseoncommand".to_string(),
                    "linear,0.2;vertalign,bottom".to_string(),
                )]),
            },
        ];

        let column = itg_receptor_column(
            &layers,
            &metrics,
            || Some(Slot(9)),
            || Some(Slot(10)),
            || Some(Slot(11)),
            |slot, command| {
                if command == "zoom,2" {
                    slot.0 += 10;
                }
            },
            |slot| slot.0 as f32,
        )
        .expect("receptor column");

        assert_eq!(column.off, Slot(11));
        assert_eq!(column.glow, Some(Slot(2)));
        assert_eq!(column.pulse_command.as_deref(), Some("zoom,2"));
        assert_eq!(column.off_reverse.reverse_off.vert_align, Some(0.0));
        assert_eq!(column.glow_reverse.reverse_on.vert_align, Some(1.0));
    }

    #[test]
    fn receptor_glow_behavior_uses_second_layer_commands_then_metrics() {
        let layers = vec![
            ItgResolvedSprite {
                element: "Receptor".to_string(),
                slot: Slot(1),
                commands: HashMap::from([(
                    "presscommand".to_string(),
                    "linear,0.1;zoom,3".to_string(),
                )]),
            },
            ItgResolvedSprite {
                element: "Receptor".to_string(),
                slot: Slot(2),
                commands: HashMap::from([(
                    "presscommand".to_string(),
                    "linear,0.2;zoom,4".to_string(),
                )]),
            },
        ];

        let behavior = itg_receptor_glow_behavior_from_layers(&layers, |key| match key {
            "LiftCommand" => Some("linear,0.3;diffusealpha,0".to_string()),
            _ => None,
        });

        assert!((behavior.press_duration - 0.2).abs() <= f32::EPSILON);
        assert!((behavior.press_zoom_end - 4.0).abs() <= f32::EPSILON);
        assert!((behavior.duration - 0.3).abs() <= f32::EPSILON);
        assert_eq!(behavior.alpha_end, 0.0);
    }

    #[test]
    fn receptor_pulse_from_command_uses_script_or_default() {
        let default_pulse = itg_receptor_pulse_from_command(None);
        assert_eq!(default_pulse.effect_color1, [1.0; 4]);
        assert!((default_pulse.effect_period - 1.0).abs() <= f32::EPSILON);

        let pulse = itg_receptor_pulse_from_command(Some(
            "effectcolor1,0.25,0.5,0.75,1;effectcolor2,1,0,0.5,1;effectperiod,2",
        ));

        assert_eq!(pulse.effect_color1, [0.25, 0.5, 0.75, 1.0]);
        assert_eq!(pulse.effect_color2, [1.0, 0.0, 0.5, 1.0]);
        assert!((pulse.effect_period - 2.0).abs() <= f32::EPSILON);
        assert!((pulse.ramp_to_half - 1.0).abs() <= f32::EPSILON);
        assert!((pulse.ramp_to_full - 1.0).abs() <= f32::EPSILON);
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
    fn hit_mine_explosion_from_layers_prefers_actor_policy() {
        let layers = vec![ItgResolvedSprite {
            element: "Explosion".to_string(),
            slot: Slot(1),
            commands: HashMap::from([
                ("initcommand".to_string(), "zoom,0.25".to_string()),
                (
                    "hitminecommand".to_string(),
                    "linear,0.4;diffusealpha,0".to_string(),
                ),
            ]),
        }];

        let explosion = itg_hit_mine_explosion_from_layers(
            &layers,
            || panic!("direct fallback should be lazy"),
            || panic!("actor fallback should be lazy"),
            Some("linear,0.9;diffusealpha,0".to_string()),
        )
        .expect("hit mine explosion");

        assert_eq!(explosion.slot, Slot(1));
        assert!((explosion.animation.initial.zoom - 0.25).abs() <= f32::EPSILON);
        assert!((explosion.duration() - 0.4).abs() <= f32::EPSILON);
    }

    #[test]
    fn hit_mine_explosion_from_layers_uses_fallback_and_metric() {
        let explosion = itg_hit_mine_explosion_from_layers(
            &[],
            || None,
            || Some(Slot(2)),
            Some("linear,0.6;diffusealpha,0".to_string()),
        )
        .expect("fallback hit mine explosion");

        assert_eq!(explosion.slot, Slot(2));
        assert!((explosion.duration() - 0.6).abs() <= f32::EPSILON);
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
    fn tap_explosion_map_from_layers_uses_direct_only_without_actor_sources() {
        #[derive(Clone)]
        struct Layer {
            element: &'static str,
            slot: Slot,
            commands: HashMap<String, String>,
        }

        let direct = |mode| match mode {
            ItgTapExplosionMode::Dim => vec![Layer {
                element: "Tap Explosion Dim W1",
                slot: Slot(1),
                commands: HashMap::from([("w1command".to_string(), "diffusealpha,1".to_string())]),
            }],
            ItgTapExplosionMode::Bright => vec![Layer {
                element: "Tap Explosion Bright W1",
                slot: Slot(2),
                commands: HashMap::from([("w1command".to_string(), "diffusealpha,1".to_string())]),
            }],
        };
        let to_source = |layer: &Layer| {
            ItgTapExplosionSource::new(
                layer.element.to_string(),
                layer.slot.clone(),
                layer.commands.clone(),
            )
        };

        let direct_map = itg_tap_explosion_map_from_layers(
            &[] as &[Layer],
            |layer| itg_has_tap_explosion_command(&layer.commands),
            direct,
            to_source,
            |_, _| None,
        );

        assert_eq!(
            direct_map.get("W1").map(|explosion| explosion.slot.clone()),
            Some(Slot(1))
        );
        assert_eq!(
            direct_map
                .get("W1Bright")
                .map(|explosion| explosion.slot.clone()),
            Some(Slot(2))
        );

        let actor = [Layer {
            element: "Explosion",
            slot: Slot(9),
            commands: HashMap::from([("w1command".to_string(), "diffusealpha,1".to_string())]),
        }];
        let actor_map = itg_tap_explosion_map_from_layers(
            &actor,
            |layer| itg_has_tap_explosion_command(&layer.commands),
            |_| panic!("direct layers should not be resolved when actor sources exist"),
            to_source,
            |_, _| None,
        );

        assert_eq!(
            actor_map.get("W1").map(|explosion| explosion.slot.clone()),
            Some(Slot(9))
        );
    }

    #[test]
    fn tap_explosion_map_from_resolved_layers_builds_sources() {
        let direct = |base_element: &str| match base_element {
            "Tap Explosion Dim" => vec![ItgResolvedSprite {
                element: "Tap Explosion Dim W1".to_string(),
                slot: Slot(1),
                commands: HashMap::new(),
            }],
            "Tap Explosion Bright" => vec![ItgResolvedSprite {
                element: "Tap Explosion Bright W1".to_string(),
                slot: Slot(2),
                commands: HashMap::new(),
            }],
            _ => Vec::new(),
        };

        let direct_map =
            itg_tap_explosion_map_from_resolved_layers(&[], direct, |_, metric_key| {
                (metric_key == "W1Command").then(|| "linear,0.2;diffusealpha,0".to_string())
            });

        assert_eq!(
            direct_map.get("W1").map(|explosion| explosion.slot.clone()),
            Some(Slot(1))
        );
        assert_eq!(
            direct_map
                .get("W1Bright")
                .map(|explosion| explosion.slot.clone()),
            Some(Slot(2))
        );

        let actor = [ItgResolvedSprite {
            element: "Explosion".to_string(),
            slot: Slot(9),
            commands: HashMap::from([(
                "w1command".to_string(),
                "linear,0.3;diffusealpha,0".to_string(),
            )]),
        }];
        let actor_map = itg_tap_explosion_map_from_resolved_layers(
            &actor,
            |_| panic!("direct layers should not be resolved when actor sources exist"),
            |_, _| None,
        );

        assert_eq!(
            actor_map.get("W1").map(|explosion| explosion.slot.clone()),
            Some(Slot(9))
        );
        assert!((actor_map["W1"].duration() - 0.3).abs() <= f32::EPSILON);
    }

    #[test]
    fn direct_tap_explosion_resolved_layers_skip_blank_variants() {
        let layers = itg_direct_tap_explosion_resolved_layers(
            "Tap Explosion Dim",
            true,
            |element| element.ends_with("W2") || element.ends_with("W4"),
            |element| {
                vec![ItgResolvedSprite {
                    element: element.to_string(),
                    slot: Slot(element.len() as u8),
                    commands: HashMap::new(),
                }]
            },
        );

        let elements = layers
            .iter()
            .map(|layer| layer.element.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            elements,
            [
                "Tap Explosion Dim W1",
                "Tap Explosion Dim W3",
                "Tap Explosion Dim W5"
            ]
        );
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
