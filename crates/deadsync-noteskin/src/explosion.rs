use crate::TweenType;
use crate::script::{
    ScriptActorMod, ScriptControl, ScriptEffectMod, normalized_script_command,
    parse_script_actor_mod, parse_script_bool, parse_script_control, parse_script_effect_mod,
    parse_script_number, parse_script_sleep, parse_script_tween, split_script_token,
    tween_type_from_script_tween,
};
use log::warn;
use std::collections::HashMap;

pub const ITG_TAP_EXPLOSION_WINDOWS: [&str; 7] = ["W1", "W2", "W3", "W4", "W5", "Miss", "Held"];

#[derive(Debug, Clone, Copy)]
pub struct ExplosionState {
    pub zoom: f32,
    pub color: [f32; 4],
    pub rotation_z: f32,
    pub visible: bool,
}

impl Default for ExplosionState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            color: [1.0, 1.0, 1.0, 1.0],
            rotation_z: 0.0,
            visible: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExplosionSegment {
    pub duration: f32,
    pub tween: TweenType,
    pub start: ExplosionState,
    pub end_zoom: Option<f32>,
    pub end_color: Option<[f32; 4]>,
    pub end_rotation_z: Option<f32>,
    pub end_visible: Option<bool>,
}

#[derive(Debug, Clone, Copy)]
pub struct GlowEffect {
    pub period: f32,
    pub color1: [f32; 4],
    pub color2: [f32; 4],
}

impl GlowEffect {
    fn color_at(&self, time: f32, base_alpha: f32) -> [f32; 4] {
        if self.period <= f32::EPSILON || base_alpha <= f32::EPSILON {
            return [0.0, 0.0, 0.0, 0.0];
        }

        let phase = (time / self.period).rem_euclid(1.0);
        if !phase.is_finite() {
            return [0.0, 0.0, 0.0, 0.0];
        }

        let percent_between = ((phase + 0.25) * std::f32::consts::TAU)
            .sin()
            .mul_add(0.5, 0.5);

        let mut color = [0.0; 4];
        for (i, channel) in color.iter_mut().enumerate() {
            *channel =
                self.color1[i].mul_add(percent_between, self.color2[i] * (1.0 - percent_between));
        }
        color[3] *= base_alpha;
        color
    }
}

#[inline(always)]
fn clamp_rgba_unit(color: [f32; 4]) -> [f32; 4] {
    [
        color[0].clamp(0.0, 1.0),
        color[1].clamp(0.0, 1.0),
        color[2].clamp(0.0, 1.0),
        color[3].clamp(0.0, 1.0),
    ]
}

#[derive(Debug, Clone, Copy)]
pub struct ExplosionVisualState {
    pub zoom: f32,
    pub diffuse: [f32; 4],
    pub glow: [f32; 4],
    pub rotation_z: f32,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct ExplosionAnimation {
    pub initial: ExplosionState,
    pub segments: Vec<ExplosionSegment>,
    pub glow: Option<GlowEffect>,
    pub blend_add: bool,
}

impl Default for ExplosionAnimation {
    fn default() -> Self {
        Self {
            initial: ExplosionState {
                zoom: 1.0,
                color: [1.0, 1.0, 1.0, 1.0],
                rotation_z: 0.0,
                visible: true,
            },
            segments: vec![ExplosionSegment {
                duration: 0.3,
                tween: TweenType::Linear,
                start: ExplosionState {
                    zoom: 1.0,
                    color: [1.0, 1.0, 1.0, 1.0],
                    rotation_z: 0.0,
                    visible: true,
                },
                end_zoom: Some(1.0),
                end_color: Some([1.0, 1.0, 1.0, 0.0]),
                end_rotation_z: None,
                end_visible: None,
            }],
            glow: None,
            blend_add: false,
        }
    }
}

impl ExplosionAnimation {
    pub fn duration(&self) -> f32 {
        self.segments
            .iter()
            .map(|segment| segment.duration.max(0.0))
            .sum::<f32>()
            .max(0.0)
    }

    pub fn state_at(&self, time: f32) -> ExplosionVisualState {
        let mut elapsed = time;
        let mut current = self.initial;

        for segment in &self.segments {
            let duration = segment.duration.max(0.0);
            if duration <= 0.0 {
                if let Some(zoom) = segment.end_zoom {
                    current.zoom = zoom;
                }
                if let Some(color) = segment.end_color {
                    current.color = color;
                }
                if let Some(rotation_z) = segment.end_rotation_z {
                    current.rotation_z = rotation_z;
                }
                if let Some(visible) = segment.end_visible {
                    current.visible = visible;
                }
                continue;
            }

            if elapsed > duration {
                if let Some(zoom) = segment.end_zoom {
                    current.zoom = zoom;
                }
                if let Some(color) = segment.end_color {
                    current.color = color;
                }
                if let Some(rotation_z) = segment.end_rotation_z {
                    current.rotation_z = rotation_z;
                }
                if let Some(visible) = segment.end_visible {
                    current.visible = visible;
                }
                elapsed -= duration;
                continue;
            }

            let progress = (elapsed / duration).clamp(0.0, 1.0);
            let eased = segment.tween.ease(progress);

            let mut zoom = current.zoom;
            if let Some(target_zoom) = segment.end_zoom {
                zoom = (target_zoom - segment.start.zoom).mul_add(eased, segment.start.zoom);
            }

            let mut color = current.color;
            if let Some(target_color) = segment.end_color {
                let mut interpolated = current.color;
                for i in 0..4 {
                    interpolated[i] = (target_color[i] - segment.start.color[i])
                        .mul_add(eased, segment.start.color[i]);
                }
                color = interpolated;
            }
            let mut rotation_z = current.rotation_z;
            if let Some(target_rotation_z) = segment.end_rotation_z {
                rotation_z = (target_rotation_z - segment.start.rotation_z)
                    .mul_add(eased, segment.start.rotation_z);
            }

            let diffuse = color;
            let glow = self
                .glow
                .map_or([0.0, 0.0, 0.0, 0.0], |g| g.color_at(time, diffuse[3]));
            let visible = if progress >= 1.0 {
                segment.end_visible.unwrap_or(current.visible)
            } else {
                current.visible
            };

            return ExplosionVisualState {
                zoom,
                diffuse: clamp_rgba_unit(diffuse),
                glow: clamp_rgba_unit(glow),
                rotation_z,
                visible,
            };
        }

        let diffuse = current.color;
        let glow = self
            .glow
            .map_or([0.0, 0.0, 0.0, 0.0], |g| g.color_at(time, diffuse[3]));

        ExplosionVisualState {
            zoom: current.zoom,
            diffuse: clamp_rgba_unit(diffuse),
            glow: clamp_rgba_unit(glow),
            rotation_z: current.rotation_z,
            visible: current.visible,
        }
    }
}

struct PendingSegment {
    tween: TweenType,
    duration: f32,
    start: ExplosionState,
    target_zoom: Option<f32>,
    target_color: Option<[f32; 4]>,
    target_rotation_z: Option<f32>,
    target_visible: Option<bool>,
}

impl PendingSegment {
    fn end_state(&self) -> ExplosionState {
        let mut end_state = self.start;
        if let Some(z) = self.target_zoom {
            end_state.zoom = z;
        }
        if let Some(color) = self.target_color {
            end_state.color = color;
        }
        if let Some(rotation_z) = self.target_rotation_z {
            end_state.rotation_z = rotation_z;
        }
        if let Some(visible) = self.target_visible {
            end_state.visible = visible;
        }
        end_state
    }

    fn into_segment(self) -> ExplosionSegment {
        ExplosionSegment {
            duration: self.duration.max(0.0),
            tween: self.tween,
            start: self.start,
            end_zoom: self.target_zoom,
            end_color: self.target_color,
            end_rotation_z: self.target_rotation_z,
            end_visible: self.target_visible,
        }
    }
}

pub fn parse_explosion_animation(script: &str) -> ExplosionAnimation {
    let mut animation = ExplosionAnimation {
        initial: ExplosionState::default(),
        segments: Vec::new(),
        glow: None,
        blend_add: false,
    };

    let mut current_state = ExplosionState::default();
    let mut initial_locked = false;
    let mut recognized_command = false;
    let mut pending: Option<PendingSegment> = None;

    let finish_pending = |pending: &mut Option<PendingSegment>,
                          animation: &mut ExplosionAnimation,
                          current_state: &mut ExplosionState,
                          emit_segment: bool| {
        if let Some(segment) = pending.take() {
            let end_state = segment.end_state();
            if emit_segment {
                animation.segments.push(segment.into_segment());
            }
            *current_state = end_state;
        }
    };

    let script = normalized_script_command(script);
    for raw_token in script.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }

        let Some((command, args)) = split_script_token(token) else {
            continue;
        };

        if let Some((tween, duration)) = parse_script_tween(command.as_str(), &args) {
            recognized_command = true;
            finish_pending(&mut pending, &mut animation, &mut current_state, true);
            pending = Some(PendingSegment {
                tween: tween_type_from_script_tween(tween),
                duration: duration.max(0.0),
                start: current_state,
                target_zoom: None,
                target_color: None,
                target_rotation_z: None,
                target_visible: None,
            });
            if !initial_locked {
                animation.initial = current_state;
                initial_locked = true;
            }
            continue;
        }
        if let Some(duration) = parse_script_sleep(command.as_str(), &args) {
            recognized_command = true;
            finish_pending(&mut pending, &mut animation, &mut current_state, true);
            pending = Some(PendingSegment {
                tween: TweenType::Linear,
                duration: duration.max(0.0),
                start: current_state,
                target_zoom: None,
                target_color: None,
                target_rotation_z: None,
                target_visible: None,
            });
            if !initial_locked {
                animation.initial = current_state;
                initial_locked = true;
            }
            continue;
        }
        if let Some(control) = parse_script_control(command.as_str()) {
            recognized_command = true;
            match control {
                ScriptControl::FinishTweening => {
                    finish_pending(&mut pending, &mut animation, &mut current_state, false);
                }
                ScriptControl::StopTweening => {
                    pending = None;
                }
                ScriptControl::SetAllStateDelays => {}
                _ => finish_pending(&mut pending, &mut animation, &mut current_state, true),
            }
            continue;
        }
        if let Some(mod_cmd) = parse_script_actor_mod(command.as_str(), &args) {
            recognized_command = true;
            match mod_cmd {
                ScriptActorMod::DiffuseAlpha(value) => {
                    if let Some(segment) = pending.as_mut() {
                        let mut target_color = segment.target_color.unwrap_or(segment.start.color);
                        target_color[3] = value;
                        segment.target_color = Some(target_color);
                    } else {
                        current_state.color[3] = value;
                        if !initial_locked {
                            animation.initial = current_state;
                        }
                    }
                }
                ScriptActorMod::Zoom(value) => {
                    if let Some(segment) = pending.as_mut() {
                        segment.target_zoom = Some(value);
                    } else {
                        current_state.zoom = value;
                        if !initial_locked {
                            animation.initial = current_state;
                        }
                    }
                }
                ScriptActorMod::RotationZ(value) => {
                    if let Some(segment) = pending.as_mut() {
                        segment.target_rotation_z = Some(value);
                    } else {
                        current_state.rotation_z = value;
                        if !initial_locked {
                            animation.initial = current_state;
                        }
                    }
                }
                ScriptActorMod::Visible(value) => {
                    if let Some(segment) = pending.as_mut() {
                        segment.target_visible = Some(value);
                    } else {
                        current_state.visible = value;
                        if !initial_locked {
                            animation.initial = current_state;
                        }
                    }
                }
                ScriptActorMod::Diffuse(parsed) => {
                    if let Some(segment) = pending.as_mut() {
                        segment.target_color = Some(parsed);
                    } else {
                        current_state.color = parsed;
                        if !initial_locked {
                            animation.initial = current_state;
                        }
                    }
                }
                ScriptActorMod::BlendAdd(v) => {
                    animation.blend_add = v;
                    finish_pending(&mut pending, &mut animation, &mut current_state, true);
                }
                _ => {}
            }
            continue;
        }
        if command == "diffuse" && args.len() >= 3 {
            recognized_command = true;
            let mut parsed = [0.0f32; 4];
            let mut ok = true;
            for i in 0..3 {
                if let Some(v) = parse_script_number(&args[i]) {
                    parsed[i] = v;
                } else {
                    warn!(
                        "Failed to parse diffuse component '{}' in explosion commands",
                        args[i]
                    );
                    ok = false;
                    break;
                }
            }
            if ok {
                parsed[3] = if args.len() >= 4 {
                    parse_script_number(&args[3]).unwrap_or(current_state.color[3])
                } else {
                    current_state.color[3]
                };

                if let Some(segment) = pending.as_mut() {
                    segment.target_color = Some(parsed);
                } else {
                    current_state.color = parsed;
                    if !initial_locked {
                        animation.initial = current_state;
                    }
                }
            }
            continue;
        }
        if let Some(effect_mod) = parse_script_effect_mod(command.as_str(), &args) {
            recognized_command = true;
            match effect_mod {
                ScriptEffectMod::GlowShift => {
                    animation.glow.get_or_insert(GlowEffect {
                        period: 0.0,
                        color1: [1.0, 1.0, 1.0, 0.0],
                        color2: [1.0, 1.0, 1.0, 0.0],
                    });
                }
                ScriptEffectMod::EffectPeriod(period) => {
                    if let Some(glow) = animation.glow.as_mut() {
                        glow.period = period.max(0.0);
                    } else {
                        animation.glow = Some(GlowEffect {
                            period: period.max(0.0),
                            color1: [1.0, 1.0, 1.0, 0.0],
                            color2: [1.0, 1.0, 1.0, 0.0],
                        });
                    }
                }
                ScriptEffectMod::EffectColor1(color) => {
                    if let Some(glow) = animation.glow.as_mut() {
                        glow.color1 = color;
                    } else {
                        animation.glow = Some(GlowEffect {
                            period: 0.0,
                            color1: color,
                            color2: color,
                        });
                    }
                }
                ScriptEffectMod::EffectColor2(color) => {
                    if let Some(glow) = animation.glow.as_mut() {
                        glow.color2 = color;
                    } else {
                        animation.glow = Some(GlowEffect {
                            period: 0.0,
                            color1: color,
                            color2: color,
                        });
                    }
                }
                _ => {}
            }
            continue;
        }
        if !command.is_empty() {
            warn!("Unhandled explosion command '{command}'.");
        }
    }

    finish_pending(&mut pending, &mut animation, &mut current_state, true);

    if !initial_locked {
        animation.initial = current_state;
    }

    if animation.segments.is_empty() && recognized_command {
        animation.initial = current_state;
        animation.segments.push(ExplosionSegment {
            duration: 0.0,
            tween: TweenType::Linear,
            start: current_state,
            end_zoom: Some(current_state.zoom),
            end_color: Some(current_state.color),
            end_rotation_z: Some(current_state.rotation_z),
            end_visible: Some(current_state.visible),
        });
    } else if animation.segments.is_empty() {
        animation.segments.push(ExplosionSegment {
            duration: 0.3,
            tween: TweenType::Linear,
            start: animation.initial,
            end_zoom: Some(animation.initial.zoom),
            end_color: Some([
                animation.initial.color[0],
                animation.initial.color[1],
                animation.initial.color[2],
                0.0,
            ]),
            end_rotation_z: Some(animation.initial.rotation_z),
            end_visible: None,
        });
    }

    animation
}

pub fn itg_command_with_init(init_command: Option<&str>, command: &str) -> Option<String> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    let init_command = init_command
        .map(str::trim)
        .filter(|value| !value.is_empty());
    Some(match init_command {
        Some(init) => [init, command].join(";"),
        None => command.to_owned(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItgTapExplosionMode {
    Dim,
    Bright,
}

impl ItgTapExplosionMode {
    pub fn command_key(self) -> &'static str {
        match self {
            Self::Dim => "dimcommand",
            Self::Bright => "brightcommand",
        }
    }

    pub fn metric_section(self) -> &'static str {
        match self {
            Self::Dim => "GhostArrowDim",
            Self::Bright => "GhostArrowBright",
        }
    }
}

#[derive(Clone)]
pub struct ItgTapExplosionSource<T> {
    pub element: String,
    pub payload: T,
    pub commands: HashMap<String, String>,
    pub mode: ItgTapExplosionMode,
}

impl<T> ItgTapExplosionSource<T> {
    pub fn new(element: String, payload: T, commands: HashMap<String, String>) -> Self {
        let mode = itg_tap_explosion_mode(&element)
            .or_else(|| itg_tap_explosion_mode_from_commands(&commands))
            .unwrap_or(ItgTapExplosionMode::Dim);
        Self {
            element,
            payload,
            commands,
            mode,
        }
    }

    pub fn matches_window(&self, window: &str) -> bool {
        itg_tap_explosion_element_window(&self.element)
            .is_some_and(|value| value.eq_ignore_ascii_case(window))
    }

    pub fn applies_to_window(&self, window: &str, command_key: &str) -> bool {
        self.commands.contains_key(command_key)
            || self.matches_window(window)
            || self.is_generic_tap_explosion()
    }

    pub fn is_generic_tap_explosion(&self) -> bool {
        let element = self.element.trim();
        element.eq_ignore_ascii_case("tap explosion dim")
            || element.eq_ignore_ascii_case("tap explosion bright")
    }
}

pub fn itg_has_tap_explosion_command(commands: &HashMap<String, String>) -> bool {
    [
        "w1command",
        "w2command",
        "w3command",
        "w4command",
        "w5command",
        "heldcommand",
    ]
    .iter()
    .any(|key| commands.contains_key(*key))
}

pub fn itg_has_hit_mine_command(commands: &HashMap<String, String>) -> bool {
    commands.contains_key("hitminecommand")
}

pub fn itg_is_hit_mine_explosion_element(element: &str) -> bool {
    crate::actor::element_contains_hint(element, "hitmine explosion")
}

pub fn itg_mine_explosion_commands(commands: &HashMap<String, String>) -> Vec<String> {
    ["ecommand", "e2command"]
        .iter()
        .filter_map(|key| {
            itg_command_with_init(
                commands.get("initcommand").map(String::as_str),
                commands.get(*key)?,
            )
        })
        .collect()
}

pub fn itg_hit_mine_command_with_init(
    commands: Option<&HashMap<String, String>>,
    metric_command: Option<String>,
) -> Option<String> {
    let command = commands
        .and_then(|commands| commands.get("hitminecommand").cloned())
        .or(metric_command)?;
    itg_command_with_init(
        commands.and_then(|commands| commands.get("initcommand").map(String::as_str)),
        &command,
    )
}

pub fn itg_partition_tap_explosion_sources<T>(
    sources: impl IntoIterator<Item = ItgTapExplosionSource<T>>,
) -> (Vec<ItgTapExplosionSource<T>>, Vec<ItgTapExplosionSource<T>>) {
    let mut dim = Vec::new();
    let mut bright = Vec::new();
    for source in sources {
        match source.mode {
            ItgTapExplosionMode::Dim => dim.push(source),
            ItgTapExplosionMode::Bright => bright.push(source),
        }
    }
    (dim, bright)
}

pub(crate) fn itg_tap_explosion_command_with_init<T>(
    source: &ItgTapExplosionSource<T>,
    mode: ItgTapExplosionMode,
    command: &str,
) -> String {
    let mut sequence = [""; 4];
    let mut len = 0;
    for command in [
        source.commands.get("initcommand"),
        source.commands.get("judgmentcommand"),
        source.commands.get(mode.command_key()),
    ]
    .into_iter()
    .flatten()
    .map(|command| command.trim())
    .filter(|command| !command.is_empty())
    {
        sequence[len] = command;
        len += 1;
    }
    sequence[len] = command.trim();
    sequence[..=len].join(";")
}

pub fn itg_explosion_wrapper<'a, T>(
    layers: &'a [T],
    active_key: &str,
    element_hint: &str,
    mut has_command: impl FnMut(&T, &str) -> bool,
    mut element_matches_hint: impl FnMut(&T, &str) -> bool,
) -> Option<&'a T> {
    layers
        .iter()
        .find(|layer| has_command(layer, active_key))
        .or_else(|| {
            layers
                .iter()
                .find(|layer| element_matches_hint(layer, element_hint))
        })
}

pub fn itg_explosion_source<'a, T>(
    layers: &'a [T],
    active_key: &str,
    mut has_command: impl FnMut(&T, &str) -> bool,
) -> Option<&'a T> {
    layers
        .iter()
        .find(|layer| has_command(layer, active_key))
        .or_else(|| layers.first())
}

pub fn itg_hold_explosion_slot<L, T: Clone>(
    wrapper_layers: &[L],
    source_layers: &[L],
    active_key: &str,
    element_hint: &str,
    blank: bool,
    fallback_slot: Option<T>,
    mut has_command: impl FnMut(&L, &str) -> bool,
    mut element_matches_hint: impl FnMut(&L, &str) -> bool,
    mut layer_slot: impl FnMut(&L) -> T,
    mut apply_commands: impl FnMut(T, &L, &str) -> T,
    mut direct_slot: impl FnMut() -> Option<T>,
    mut wrapped_slots: impl FnMut() -> Vec<T>,
) -> Option<T> {
    let wrapper_has_active = wrapper_layers
        .iter()
        .any(|layer| has_command(layer, active_key));
    let wrapper_has_hint = wrapper_layers
        .iter()
        .any(|layer| element_matches_hint(layer, element_hint));
    let wrapper_has_flash_child = wrapper_layers
        .iter()
        .any(|layer| has_command(layer, "flashcommand"));
    let wrapper = wrapper_layers
        .iter()
        .find(|layer| has_command(layer, active_key))
        .or_else(|| {
            wrapper_layers
                .iter()
                .find(|layer| element_matches_hint(layer, element_hint))
        });

    if let Some(layer) = wrapper.filter(|layer| has_command(layer, active_key)) {
        return Some(apply_commands(layer_slot(layer), layer, active_key));
    }
    if wrapper_has_flash_child && !wrapper_has_active && !wrapper_has_hint {
        return None;
    }
    if blank {
        return None;
    }

    let source = source_layers
        .iter()
        .find(|layer| has_command(layer, active_key))
        .or_else(|| source_layers.first());
    if let Some(layer) = source {
        let commands_layer = wrapper.unwrap_or(layer);
        return Some(apply_commands(
            layer_slot(layer),
            commands_layer,
            active_key,
        ));
    }
    if let Some(layer) = wrapper {
        return Some(apply_commands(layer_slot(layer), layer, active_key));
    }
    if let Some(slot) = direct_slot() {
        return Some(slot);
    }
    if let Some(slot) = wrapped_slots().into_iter().next() {
        return Some(match wrapper {
            Some(layer) => apply_commands(slot, layer, active_key),
            None => slot,
        });
    }
    fallback_slot
}

pub fn itg_hit_mine_explosion_slot<'a, L, T>(
    layers: &'a [L],
    mut has_hit_mine_command: impl FnMut(&L) -> bool,
    mut is_hit_mine_element: impl FnMut(&L) -> bool,
    mut layer_slot: impl FnMut(&L) -> T,
    mut direct_slot: impl FnMut() -> Option<T>,
    actor_slot: impl FnMut() -> Option<T>,
) -> (Option<&'a L>, Option<T>) {
    let source = layers
        .iter()
        .find(|layer| has_hit_mine_command(layer))
        .or_else(|| layers.iter().find(|layer| is_hit_mine_element(layer)));
    let slot = source
        .map(&mut layer_slot)
        .or_else(&mut direct_slot)
        .or_else(actor_slot);
    (source, slot)
}

pub fn itg_tap_explosion_mode(element: &str) -> Option<ItgTapExplosionMode> {
    let starts_with = |prefix: &str| {
        element
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    };
    starts_with("tap explosion bright")
        .then_some(ItgTapExplosionMode::Bright)
        .or_else(|| starts_with("tap explosion dim").then_some(ItgTapExplosionMode::Dim))
}

pub fn itg_tap_explosion_key(window: &str, mode: ItgTapExplosionMode) -> &str {
    if mode == ItgTapExplosionMode::Bright
        && let Some(key) = crate::bright_tap_explosion_key(window)
    {
        key
    } else {
        window
    }
}

pub fn itg_tap_explosion_mode_from_commands(
    commands: &HashMap<String, String>,
) -> Option<ItgTapExplosionMode> {
    let bright_visible = commands
        .get("brightcommand")
        .and_then(|cmd| itg_script_visible_command(cmd));
    let dim_visible = commands
        .get("dimcommand")
        .and_then(|cmd| itg_script_visible_command(cmd));
    match (bright_visible, dim_visible) {
        (Some(true), Some(false)) => Some(ItgTapExplosionMode::Bright),
        (Some(false), Some(true)) => Some(ItgTapExplosionMode::Dim),
        (None, Some(true)) => Some(ItgTapExplosionMode::Dim),
        (Some(true), None) => Some(ItgTapExplosionMode::Bright),
        _ => None,
    }
}

fn itg_script_visible_command(script: &str) -> Option<bool> {
    let script = normalized_script_command(script);
    script.split(';').find_map(|token| {
        let (command, args) = split_script_token(token)?;
        (command == "visible")
            .then(|| args.first().map(|arg| parse_script_bool(arg)))
            .flatten()
    })
}

pub fn itg_tap_explosion_element_window(element: &str) -> Option<&str> {
    let element = element.trim();
    element
        .strip_prefix("Tap Explosion Dim ")
        .or_else(|| element.strip_prefix("Tap Explosion Bright "))
        .map(str::trim)
        .filter(|value| {
            let bytes = value.as_bytes();
            bytes.len() == 2
                && bytes[0].eq_ignore_ascii_case(&b'w')
                && matches!(bytes[1], b'1'..=b'5')
        })
}

pub fn itg_direct_tap_explosion_elements(
    base_element: &str,
    base_blank: bool,
    mut is_blank: impl FnMut(&str) -> bool,
) -> Vec<String> {
    let mut out = Vec::new();
    if !base_blank {
        out.push(base_element.to_string());
    }
    for window in ["W1", "W2", "W3", "W4", "W5"] {
        let element = format!("{base_element} {window}");
        if !is_blank(&element) {
            out.push(element);
        }
    }
    out
}

pub fn itg_direct_tap_explosion_layers<T>(
    base_element: &str,
    base_blank: bool,
    is_blank: impl FnMut(&str) -> bool,
    mut resolve_element: impl FnMut(&str) -> Vec<T>,
) -> Vec<T> {
    let mut out = Vec::new();
    for element in itg_direct_tap_explosion_elements(base_element, base_blank, is_blank) {
        out.extend(resolve_element(&element));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_explosion_animation_builds_tween_segments() {
        let anim =
            parse_explosion_animation("diffusealpha,1;zoom,0.5;linear,0.2;diffusealpha,0;zoom,1.5");

        assert_eq!(anim.initial.color[3], 1.0);
        assert_eq!(anim.initial.zoom, 0.5);
        assert_eq!(anim.segments.len(), 1);
        assert_eq!(anim.segments[0].duration, 0.2);
        assert_eq!(anim.segments[0].end_color, Some([1.0, 1.0, 1.0, 0.0]));
        assert_eq!(anim.segments[0].end_zoom, Some(1.5));
    }

    #[test]
    fn parse_explosion_animation_tracks_glowshift_and_blend() {
        let anim = parse_explosion_animation(
            "blend,'BlendMode_Add';glowshift;effectperiod,0.5;effectcolor1,#ff000080;effectcolor2,#00ff0080",
        );

        assert!(anim.blend_add);
        let glow = anim.glow.expect("glow");
        assert_eq!(glow.period, 0.5);
        assert_eq!(glow.color1, [1.0, 0.0, 0.0, 0.5019608]);
        assert_eq!(glow.color2, [0.0, 1.0, 0.0, 0.5019608]);

        let normal = parse_explosion_animation("blend,'BlendMode_Normal';diffusealpha,1");
        assert!(!normal.blend_add);
    }

    #[test]
    fn parse_explosion_animation_handles_function_duration_and_blend() {
        let anim = parse_explosion_animation(
            "function (self) self:finishtweening():diffusealpha(1.0):blend(Blend.Add):linear(12/60):diffusealpha(0.0) end",
        );

        assert!(anim.blend_add);
        assert!((anim.initial.color[3] - 1.0).abs() <= 1e-6);
        assert_eq!(anim.segments.len(), 1);
        assert!((anim.segments[0].duration - 0.2).abs() <= 1e-6);
        assert_eq!(anim.segments[0].end_color.map(|c| c[3]), Some(0.0));
    }

    #[test]
    fn parse_explosion_animation_finish_tweening_resets_prior_segments() {
        let anim = parse_explosion_animation(
            "diffusealpha,1;linear,0.2;diffusealpha,0;finishtweening;diffusealpha,1;linear,0.1;diffusealpha,0",
        );

        assert_eq!(anim.segments.len(), 1);
        assert!((anim.duration() - 0.1).abs() <= 1e-6);
        assert!((anim.state_at(0.05).diffuse[3] - 0.5).abs() <= 1e-6);

        let canceled =
            parse_explosion_animation("diffusealpha,1;linear,0.2;diffusealpha,0;finishtweening");
        assert_eq!(canceled.duration(), 0.0);
        assert_eq!(canceled.state_at(0.0).diffuse[3], 0.0);
    }

    #[test]
    fn parse_explosion_animation_honors_visible_commands() {
        let anim = parse_explosion_animation("visible,false;sleep,0.1;visible,true");

        assert!(!anim.state_at(0.0).visible);
        assert!(!anim.state_at(0.05).visible);
        assert!(anim.state_at(0.11).visible);
    }

    #[test]
    fn parse_explosion_animation_parses_judgment_line_color() {
        let anim = parse_explosion_animation(
            r#"finishtweening;diffuse,JudgmentLineToColor("JudgmentLine_W5");diffusealpha,1;sleep,.1;decelerate,.2;diffusealpha,0"#,
        );

        assert_eq!(anim.initial.color, [228.0 / 255.0, 77.0 / 255.0, 1.0, 1.0]);
    }

    #[test]
    fn parse_explosion_animation_clamps_overbright_color() {
        let anim = parse_explosion_animation(
            "diffuse,1.5,1.25,1.75,1.2;glowshift;effectperiod,0.05;effectcolor1,1,1,1,1;effectcolor2,1,1,1,1",
        );
        let state = anim.state_at(0.0);

        assert_eq!(state.diffuse, [1.0, 1.0, 1.0, 1.0]);
        assert!(state.glow.iter().all(|c| *c >= 0.0 && *c <= 1.0));
    }

    #[test]
    fn itg_command_with_init_prepends_nonempty_init() {
        assert_eq!(
            itg_command_with_init(Some(" zoom,2 "), " diffusealpha,0 "),
            Some("zoom,2;diffusealpha,0".to_string())
        );
        assert_eq!(
            itg_command_with_init(Some("  "), " diffusealpha,0 "),
            Some("diffusealpha,0".to_string())
        );
        assert_eq!(itg_command_with_init(Some("zoom,2"), "  "), None);
    }

    #[test]
    fn tap_explosion_command_sequence_preserves_order_and_empty_final_command() {
        let source = ItgTapExplosionSource::new(
            "Tap Explosion Bright".to_owned(),
            (),
            HashMap::from([
                ("initcommand".to_owned(), " finish ".to_owned()),
                ("judgmentcommand".to_owned(), " diffuse ".to_owned()),
                ("brightcommand".to_owned(), " glow ".to_owned()),
            ]),
        );

        assert_eq!(
            itg_tap_explosion_command_with_init(&source, ItgTapExplosionMode::Bright, " sleep "),
            "finish;diffuse;glow;sleep"
        );
        assert_eq!(
            itg_tap_explosion_command_with_init(&source, ItgTapExplosionMode::Bright, " "),
            "finish;diffuse;glow;"
        );
    }

    #[test]
    fn tap_explosion_source_uses_visibility_for_mode() {
        let commands = HashMap::from([
            ("brightcommand".to_string(), "visible,true".to_string()),
            ("dimcommand".to_string(), "visible,false".to_string()),
        ]);
        let source = ItgTapExplosionSource::new("Explosion".to_string(), 7, commands);

        assert_eq!(source.mode, ItgTapExplosionMode::Bright);
    }

    #[test]
    fn explosion_element_classifiers_preserve_ascii_case_rules() {
        assert_eq!(
            itg_tap_explosion_mode("tAp ExPlOsIoN BrIgHt W1"),
            Some(ItgTapExplosionMode::Bright)
        );
        assert_eq!(itg_tap_explosion_mode("aaaaaaaaaaaaaaaaaaaé"), None);
        assert!(itg_is_hit_mine_explosion_element(
            "Fallback HITMINE EXPLOSION glow"
        ));
        assert!(!itg_is_hit_mine_explosion_element("Hit Mine Explosion"));

        let generic =
            ItgTapExplosionSource::new(" tAp ExPlOsIoN dIm ".to_string(), (), HashMap::new());
        assert!(generic.is_generic_tap_explosion());
        assert_eq!(
            itg_tap_explosion_element_window(" Tap Explosion Bright w5 "),
            Some("w5")
        );
        assert_eq!(
            itg_tap_explosion_element_window("tap explosion bright W5"),
            None
        );
        assert_eq!(
            itg_tap_explosion_element_window("Tap Explosion Bright W6"),
            None
        );
    }

    #[test]
    fn tap_explosion_sources_partition_by_mode() {
        let sources = [
            ItgTapExplosionSource::new("Tap Explosion Dim W1".to_string(), "dim", HashMap::new()),
            ItgTapExplosionSource::new(
                "Tap Explosion Bright W1".to_string(),
                "bright",
                HashMap::new(),
            ),
        ];
        let (dim, bright) = itg_partition_tap_explosion_sources(sources);

        assert_eq!(
            dim.iter().map(|source| source.payload).collect::<Vec<_>>(),
            vec!["dim"]
        );
        assert_eq!(
            bright
                .iter()
                .map(|source| source.payload)
                .collect::<Vec<_>>(),
            vec!["bright"]
        );
    }

    #[test]
    fn mine_explosion_commands_include_init_for_each_layer() {
        let commands = HashMap::from([
            ("initcommand".to_string(), "zoom,2".to_string()),
            ("ecommand".to_string(), "diffusealpha,1".to_string()),
            ("e2command".to_string(), "diffusealpha,0".to_string()),
        ]);

        assert_eq!(
            itg_mine_explosion_commands(&commands),
            vec!["zoom,2;diffusealpha,1", "zoom,2;diffusealpha,0"]
        );
    }

    #[test]
    fn explosion_animation_ignores_all_state_delay_control() {
        let anim = parse_explosion_animation("linear,0.1;SetAllStateDelays,0.05;diffusealpha,0");

        assert_eq!(anim.segments.len(), 1);
        assert!((anim.state_at(0.1).diffuse[3] - 0.0).abs() <= f32::EPSILON);
    }

    #[test]
    fn hit_mine_command_prefers_source_then_metric() {
        let commands = HashMap::from([
            ("initcommand".to_string(), "zoom,2".to_string()),
            ("hitminecommand".to_string(), "diffusealpha,1".to_string()),
        ]);

        assert_eq!(
            itg_hit_mine_command_with_init(Some(&commands), Some("diffusealpha,0".to_string())),
            Some("zoom,2;diffusealpha,1".to_string())
        );
        assert_eq!(
            itg_hit_mine_command_with_init(None, Some("diffusealpha,0".to_string())),
            Some("diffusealpha,0".to_string())
        );
    }

    #[test]
    fn explosion_wrapper_prefers_active_command_then_element_hint() {
        #[derive(Debug)]
        struct Layer {
            commands: HashMap<String, String>,
            element: &'static str,
        }
        let hinted = Layer {
            commands: HashMap::new(),
            element: "Down Hold Explosion",
        };
        let active = Layer {
            commands: HashMap::from([("holdingoncommand".to_string(), String::new())]),
            element: "Down Sprite",
        };
        let layers = [hinted, active];

        let selected = itg_explosion_wrapper(
            &layers,
            "holdingoncommand",
            "hold explosion",
            |layer, key| layer.commands.contains_key(key),
            |layer, hint| crate::actor::element_contains_hint(layer.element, hint),
        )
        .expect("wrapper should resolve");

        assert_eq!(selected.element, "Down Sprite");
    }

    #[test]
    fn explosion_source_prefers_active_command_then_first_layer() {
        #[derive(Debug)]
        struct Layer {
            id: u8,
            commands: HashMap<String, String>,
        }
        let layers = [
            Layer {
                id: 1,
                commands: HashMap::new(),
            },
            Layer {
                id: 2,
                commands: HashMap::from([("rolloncommand".to_string(), String::new())]),
            },
        ];

        let active = itg_explosion_source(&layers, "rolloncommand", |layer, key| {
            layer.commands.contains_key(key)
        });
        let first = itg_explosion_source(&layers, "holdingoncommand", |layer, key| {
            layer.commands.contains_key(key)
        });

        assert_eq!(active.map(|layer| layer.id), Some(2));
        assert_eq!(first.map(|layer| layer.id), Some(1));
    }

    #[test]
    fn hold_explosion_slot_prefers_wrapper_and_source_before_fallbacks() {
        #[derive(Debug, Clone)]
        struct Layer {
            id: &'static str,
            element: &'static str,
            active: bool,
        }

        let wrapper_layers = [Layer {
            id: "wrapper",
            element: "Hold Explosion Wrapper",
            active: true,
        }];
        let source_layers = [Layer {
            id: "source",
            element: "Hold Explosion",
            active: true,
        }];
        let slot = itg_hold_explosion_slot(
            &wrapper_layers,
            &source_layers,
            "holdingoncommand",
            "hold explosion",
            false,
            Some("fallback".to_string()),
            |layer, _| layer.active,
            |layer, hint| layer.element.to_ascii_lowercase().contains(hint),
            |layer| layer.id.to_string(),
            |slot, layer, key| format!("{slot}+{}:{key}", layer.id),
            || Some("direct".to_string()),
            || vec!["wrapped".to_string()],
        );

        assert_eq!(slot, Some("wrapper+wrapper:holdingoncommand".to_string()));
    }

    #[test]
    fn hold_explosion_slot_uses_source_with_wrapper_commands() {
        #[derive(Debug, Clone)]
        struct Layer {
            id: &'static str,
            element: &'static str,
            active: bool,
        }

        let wrapper_layers = [Layer {
            id: "wrapper",
            element: "Hold Explosion Wrapper",
            active: false,
        }];
        let source_layers = [Layer {
            id: "source",
            element: "Hold Explosion",
            active: true,
        }];
        let slot = itg_hold_explosion_slot(
            &wrapper_layers,
            &source_layers,
            "holdingoncommand",
            "hold explosion",
            false,
            None,
            |layer, _| layer.active,
            |layer, hint| layer.element.to_ascii_lowercase().contains(hint),
            |layer| layer.id.to_string(),
            |slot, layer, key| format!("{slot}+{}:{key}", layer.id),
            || None,
            Vec::new,
        );

        assert_eq!(slot, Some("source+wrapper:holdingoncommand".to_string()));
    }

    #[test]
    fn hold_explosion_slot_skips_child_flash_emitters_without_active_command() {
        #[derive(Debug, Clone)]
        struct Layer {
            id: &'static str,
            element: &'static str,
            active: bool,
            flash: bool,
        }

        let wrapper_layers = [Layer {
            id: "holdflash",
            element: "Flash Dim",
            active: false,
            flash: true,
        }];
        let source_layers = [Layer {
            id: "source",
            element: "Hold Explosion",
            active: true,
            flash: false,
        }];
        let slot = itg_hold_explosion_slot(
            &wrapper_layers,
            &source_layers,
            "holdingoncommand",
            "hold explosion",
            false,
            Some("fallback".to_string()),
            |layer, key| match key {
                "holdingoncommand" => layer.active,
                "flashcommand" => layer.flash,
                _ => false,
            },
            |layer, hint| layer.element.to_ascii_lowercase().contains(hint),
            |layer| layer.id.to_string(),
            |slot, layer, key| format!("{slot}+{}:{key}", layer.id),
            || Some("direct".to_string()),
            || vec!["wrapped".to_string()],
        );

        assert_eq!(slot, None);
    }

    #[test]
    fn hold_explosion_slot_respects_blank_and_late_fallbacks() {
        #[derive(Debug, Clone)]
        struct Layer;

        let blank = itg_hold_explosion_slot(
            &[] as &[Layer],
            &[],
            "holdingoncommand",
            "hold explosion",
            true,
            Some("fallback".to_string()),
            |_, _| false,
            |_, _| false,
            |_| "layer".to_string(),
            |slot, _, _| slot,
            || Some("direct".to_string()),
            || vec!["wrapped".to_string()],
        );
        let late = itg_hold_explosion_slot(
            &[] as &[Layer],
            &[],
            "holdingoncommand",
            "hold explosion",
            false,
            Some("fallback".to_string()),
            |_, _| false,
            |_, _| false,
            |_| "layer".to_string(),
            |slot, _, _| slot,
            || None,
            || vec!["wrapped".to_string()],
        );

        assert_eq!(blank, None);
        assert_eq!(late, Some("wrapped".to_string()));
    }

    #[test]
    fn hit_mine_explosion_slot_prefers_command_source() {
        #[derive(Debug)]
        struct Layer {
            id: &'static str,
            has_command: bool,
            is_hit_mine: bool,
        }
        let layers = [
            Layer {
                id: "element",
                has_command: false,
                is_hit_mine: true,
            },
            Layer {
                id: "command",
                has_command: true,
                is_hit_mine: false,
            },
        ];

        let (source, slot) = itg_hit_mine_explosion_slot(
            &layers,
            |layer| layer.has_command,
            |layer| layer.is_hit_mine,
            |layer| layer.id.to_string(),
            || Some("direct".to_string()),
            || Some("actor".to_string()),
        );

        assert_eq!(source.map(|layer| layer.id), Some("command"));
        assert_eq!(slot, Some("command".to_string()));
    }

    #[test]
    fn hit_mine_explosion_slot_uses_direct_then_actor_fallback() {
        #[derive(Debug)]
        struct Layer;

        let (_, direct) = itg_hit_mine_explosion_slot(
            &[] as &[Layer],
            |_| false,
            |_| false,
            |_| "layer".to_string(),
            || Some("direct".to_string()),
            || Some("actor".to_string()),
        );
        let (_, actor) = itg_hit_mine_explosion_slot(
            &[] as &[Layer],
            |_| false,
            |_| false,
            |_| "layer".to_string(),
            || None,
            || Some("actor".to_string()),
        );

        assert_eq!(direct, Some("direct".to_string()));
        assert_eq!(actor, Some("actor".to_string()));
    }

    #[test]
    fn direct_tap_explosion_elements_skip_blank_variants() {
        let elements = itg_direct_tap_explosion_elements("Tap Explosion Dim", true, |element| {
            element == "Tap Explosion Dim W1" || element == "Tap Explosion Dim W4"
        });

        assert_eq!(
            elements,
            vec![
                "Tap Explosion Dim W2",
                "Tap Explosion Dim W3",
                "Tap Explosion Dim W5"
            ]
        );
    }

    #[test]
    fn direct_tap_explosion_layers_resolve_selected_elements() {
        let layers = itg_direct_tap_explosion_layers(
            "Tap Explosion Bright",
            false,
            |element| element == "Tap Explosion Bright W2",
            |element| vec![element.to_string()],
        );

        assert_eq!(
            layers,
            vec![
                "Tap Explosion Bright",
                "Tap Explosion Bright W1",
                "Tap Explosion Bright W3",
                "Tap Explosion Bright W4",
                "Tap Explosion Bright W5",
            ]
        );
    }
}
