use crate::{
    TweenType,
    itg::IniData,
    script::{
        ScriptEffectMod, itg_parse_command_effect, normalized_script_command,
        parse_script_effect_mod, parse_script_number, parse_script_vertalign, split_script_token,
    },
};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct ReceptorGlowBehavior {
    pub press_duration: f32,
    pub press_alpha_start: f32,
    pub press_alpha_end: f32,
    pub press_zoom_start: f32,
    pub press_zoom_end: f32,
    pub press_tween: TweenType,
    pub duration: f32,
    pub alpha_start: f32,
    pub alpha_end: f32,
    pub zoom_start: f32,
    pub zoom_end: f32,
    pub tween: TweenType,
    pub blend_add: bool,
}

impl ReceptorGlowBehavior {
    pub fn sample_press(self, timer_remaining: f32) -> (f32, f32) {
        let duration = self.press_duration.max(0.0);
        if duration <= f32::EPSILON {
            return (
                self.press_alpha_end.clamp(0.0, 1.0),
                self.press_zoom_end.max(0.0),
            );
        }
        let elapsed = (duration - timer_remaining.clamp(0.0, duration)).clamp(0.0, duration);
        let progress = elapsed / duration;
        let eased = self.press_tween.ease(progress);
        let alpha =
            (self.press_alpha_end - self.press_alpha_start).mul_add(eased, self.press_alpha_start);
        let zoom =
            (self.press_zoom_end - self.press_zoom_start).mul_add(eased, self.press_zoom_start);
        (alpha.clamp(0.0, 1.0), zoom.max(0.0))
    }

    pub fn sample_lift(
        self,
        timer_remaining: f32,
        start_alpha: f32,
        start_zoom: f32,
    ) -> (f32, f32) {
        let duration = self.duration.max(0.0);
        if duration <= f32::EPSILON {
            return (self.alpha_end.clamp(0.0, 1.0), self.zoom_end.max(0.0));
        }
        let elapsed = (duration - timer_remaining.clamp(0.0, duration)).clamp(0.0, duration);
        let progress = elapsed / duration;
        let eased = self.tween.ease(progress);
        let alpha = (self.alpha_end - start_alpha).mul_add(eased, start_alpha);
        let zoom = (self.zoom_end - start_zoom).mul_add(eased, start_zoom);
        (alpha.clamp(0.0, 1.0), zoom.max(0.0))
    }
}

impl Default for ReceptorGlowBehavior {
    fn default() -> Self {
        Self {
            press_duration: 0.0,
            press_alpha_start: 1.0,
            press_alpha_end: 1.0,
            press_zoom_start: 1.0,
            press_zoom_end: 1.0,
            press_tween: TweenType::Linear,
            duration: 0.2,
            alpha_start: 1.0,
            alpha_end: 0.0,
            zoom_start: 1.0,
            zoom_end: 1.0,
            tween: TweenType::Decelerate,
            blend_add: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReceptorStepBehavior {
    pub duration: f32,
    pub zoom_start: f32,
    pub zoom_end: f32,
    pub tween: TweenType,
    pub interrupts: bool,
}

impl ReceptorStepBehavior {
    pub const fn identity() -> Self {
        Self {
            duration: 0.0,
            zoom_start: 1.0,
            zoom_end: 1.0,
            tween: TweenType::Linear,
            interrupts: false,
        }
    }

    pub fn sample_zoom(self, timer_remaining: f32) -> f32 {
        let duration = self.duration.max(0.0);
        if duration <= f32::EPSILON {
            return self.zoom_end.max(0.0);
        }
        let elapsed = (duration - timer_remaining.clamp(0.0, duration)).clamp(0.0, duration);
        let progress = elapsed / duration;
        let eased = self.tween.ease(progress);
        (self.zoom_end - self.zoom_start)
            .mul_add(eased, self.zoom_start)
            .max(0.0)
    }
}

impl Default for ReceptorStepBehavior {
    fn default() -> Self {
        Self {
            duration: 0.11,
            zoom_start: 0.75,
            zoom_end: 1.0,
            tween: TweenType::Linear,
            interrupts: true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReceptorStepBehaviors {
    none: ReceptorStepBehavior,
    miss: ReceptorStepBehavior,
    windows: [ReceptorStepBehavior; 5],
}

impl ReceptorStepBehaviors {
    pub const fn new(
        none: ReceptorStepBehavior,
        miss: ReceptorStepBehavior,
        windows: [ReceptorStepBehavior; 5],
    ) -> Self {
        Self {
            none,
            miss,
            windows,
        }
    }

    pub fn for_window(self, window: Option<&str>) -> ReceptorStepBehavior {
        match window {
            Some("W1") => self.windows[0],
            Some("W2") => self.windows[1],
            Some("W3") => self.windows[2],
            Some("W4") => self.windows[3],
            Some("W5") => self.windows[4],
            Some("Miss") => self.miss,
            _ => self.none,
        }
    }
}

impl Default for ReceptorStepBehaviors {
    fn default() -> Self {
        Self {
            none: ReceptorStepBehavior::default(),
            miss: ReceptorStepBehavior::identity(),
            windows: [ReceptorStepBehavior::identity(); 5],
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReceptorReverseState {
    pub base_rotation_z: Option<f32>,
    pub vert_align: Option<f32>,
}

impl ReceptorReverseState {
    #[inline(always)]
    pub fn base_rotation_z(self) -> f32 {
        self.base_rotation_z.unwrap_or(0.0)
    }

    #[inline(always)]
    pub fn vert_align(self) -> f32 {
        self.vert_align.unwrap_or(0.5)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReceptorReverseBehavior {
    pub reverse_off: ReceptorReverseState,
    pub reverse_on: ReceptorReverseState,
}

impl ReceptorReverseBehavior {
    #[inline(always)]
    pub const fn state(self, reverse: bool) -> ReceptorReverseState {
        if reverse {
            self.reverse_on
        } else {
            self.reverse_off
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReceptorPulse {
    pub effect_color1: [f32; 4],
    pub effect_color2: [f32; 4],
    pub effect_period: f32,
    pub ramp_to_half: f32,
    pub hold_at_half: f32,
    pub ramp_to_full: f32,
    pub hold_at_full: f32,
    pub hold_at_zero: f32,
    pub effect_offset: f32,
}

impl ReceptorPulse {
    pub fn total_period(&self) -> f32 {
        let mut total = 0.0;
        total += self.ramp_to_half.max(0.0);
        total += self.hold_at_half.max(0.0);
        total += self.ramp_to_full.max(0.0);
        total += self.hold_at_full.max(0.0);
        total += self.hold_at_zero.max(0.0);
        total
    }

    pub fn color_for_beat(&self, beat: f32) -> [f32; 4] {
        let cycle = self.total_period();
        if cycle <= f32::EPSILON {
            return self.effect_color2;
        }
        let phase = (beat + self.effect_offset).rem_euclid(cycle);

        let ramp_to_half = self.ramp_to_half.max(0.0);
        let hold_at_half = self.hold_at_half.max(0.0);
        let ramp_to_full = self.ramp_to_full.max(0.0);
        let hold_at_full = self.hold_at_full.max(0.0);

        let ramp_and_hold_half = ramp_to_half + hold_at_half;
        let through_ramp_full = ramp_and_hold_half + ramp_to_full;
        let through_hold_full = through_ramp_full + hold_at_full;

        let percent = if ramp_to_half > 0.0 && phase < ramp_to_half {
            (phase / ramp_to_half) * 0.5
        } else if phase < ramp_and_hold_half {
            0.5
        } else if ramp_to_full > 0.0 && phase < through_ramp_full {
            ((phase - ramp_and_hold_half) / ramp_to_full).mul_add(0.5, 0.5)
        } else if phase < through_hold_full {
            1.0
        } else {
            0.0
        };

        let mut color = [0.0; 4];
        for (i, channel) in color.iter_mut().enumerate() {
            *channel =
                self.effect_color1[i].mul_add(percent, self.effect_color2[i] * (1.0 - percent));
        }
        color
    }
}

impl Default for ReceptorPulse {
    fn default() -> Self {
        Self {
            effect_color1: [1.0, 1.0, 1.0, 1.0],
            effect_color2: [1.0, 1.0, 1.0, 1.0],
            effect_period: 1.0,
            ramp_to_half: 0.5,
            hold_at_half: 0.0,
            ramp_to_full: 0.5,
            hold_at_full: 0.0,
            hold_at_zero: 0.0,
            effect_offset: 0.0,
        }
    }
}

pub fn receptor_glow_behavior_from_commands(
    init_cmd: &str,
    press_cmd: &str,
    lift_cmd: &str,
    none_cmd: &str,
) -> ReceptorGlowBehavior {
    let mut out = ReceptorGlowBehavior::default();
    let init = itg_parse_command_effect(init_cmd);
    let press = itg_parse_command_effect(press_cmd);
    let lift = itg_parse_command_effect(lift_cmd);
    let none = itg_parse_command_effect(none_cmd);

    out.press_duration = press.duration.max(0.0);
    out.press_alpha_start = press
        .start_alpha
        .or(press.target_alpha)
        .or(init.target_alpha)
        .unwrap_or(out.press_alpha_start);
    out.press_alpha_end = press
        .target_alpha
        .or(press.start_alpha)
        .or(init.target_alpha)
        .unwrap_or(out.press_alpha_end);
    out.press_zoom_start = press
        .start_zoom
        .or(press.target_zoom)
        .or(init.target_zoom)
        .unwrap_or(out.press_zoom_start);
    out.press_zoom_end = press
        .target_zoom
        .or(press.start_zoom)
        .or(init.target_zoom)
        .unwrap_or(out.press_zoom_end);
    out.press_tween = if press.duration > f32::EPSILON {
        press.tween
    } else {
        out.press_tween
    };

    out.duration = if lift.duration > f32::EPSILON {
        lift.duration
    } else if none.duration > f32::EPSILON {
        none.duration
    } else if press.duration > f32::EPSILON {
        press.duration
    } else {
        out.duration
    };
    out.alpha_start = out.press_alpha_end;
    out.alpha_end = lift
        .target_alpha
        .or(none.target_alpha)
        .or(init.target_alpha)
        .unwrap_or(0.0);
    out.zoom_start = out.press_zoom_end;
    out.zoom_end = lift
        .target_zoom
        .or(none.target_zoom)
        .or(init.target_zoom)
        .unwrap_or(out.zoom_end);
    out.tween = if lift.duration > f32::EPSILON {
        lift.tween
    } else if none.duration > f32::EPSILON {
        none.tween
    } else if press.duration > f32::EPSILON {
        press.tween
    } else {
        out.tween
    };
    out.blend_add = press
        .blend_add
        .or(lift.blend_add)
        .or(init.blend_add)
        .unwrap_or(out.blend_add);
    out.press_alpha_start = out.press_alpha_start.clamp(0.0, 1.0);
    out.press_alpha_end = out.press_alpha_end.clamp(0.0, 1.0);
    out.press_zoom_start = out.press_zoom_start.max(0.0);
    out.press_zoom_end = out.press_zoom_end.max(0.0);
    out.press_duration = out.press_duration.max(0.0);
    out.alpha_start = out.alpha_start.clamp(0.0, 1.0);
    out.alpha_end = out.alpha_end.clamp(0.0, 1.0);
    out.zoom_start = out.zoom_start.max(0.0);
    out.zoom_end = out.zoom_end.max(0.0);
    out.duration = out.duration.max(0.0);
    out
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReceptorOverlayCommands {
    pub init: String,
    pub press: String,
    pub lift: String,
    pub none: String,
}

pub fn receptor_overlay_commands(
    commands: Option<&HashMap<String, String>>,
    mut metric_command: impl FnMut(&str) -> Option<String>,
) -> ReceptorOverlayCommands {
    let command = |commands: Option<&HashMap<String, String>>,
                   metric_command: &mut dyn FnMut(&str) -> Option<String>,
                   actor_key: &str,
                   metric_key: &str| {
        commands
            .and_then(|commands| commands.get(actor_key).cloned())
            .or_else(|| metric_command(metric_key))
            .unwrap_or_default()
    };

    ReceptorOverlayCommands {
        init: command(commands, &mut metric_command, "initcommand", "InitCommand"),
        press: command(
            commands,
            &mut metric_command,
            "presscommand",
            "PressCommand",
        ),
        lift: command(commands, &mut metric_command, "liftcommand", "LiftCommand"),
        none: command(commands, &mut metric_command, "nonecommand", "NoneCommand"),
    }
}

pub fn receptor_glow_behavior(
    commands: Option<&HashMap<String, String>>,
    metric_command: impl FnMut(&str) -> Option<String>,
) -> ReceptorGlowBehavior {
    let commands = receptor_overlay_commands(commands, metric_command);
    receptor_glow_behavior_from_commands(
        &commands.init,
        &commands.press,
        &commands.lift,
        &commands.none,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItgReceptorVisuals<T> {
    pub off: Option<T>,
    pub glow: Option<T>,
}

pub fn itg_receptor_visuals<T: Clone>(
    layers: &[T],
    receptor_fallback: impl FnOnce() -> Option<T>,
    rflash_fallback: impl FnOnce() -> Option<T>,
    glow_fallback: impl FnOnce() -> Option<T>,
) -> ItgReceptorVisuals<T> {
    let off = layers.first().cloned().or_else(receptor_fallback);
    let glow = layers.get(1).cloned().or_else(|| {
        if layers.is_empty() {
            rflash_fallback().or_else(glow_fallback)
        } else {
            None
        }
    });
    ItgReceptorVisuals { off, glow }
}

pub fn itg_receptor_pulse_command<'a>(layers: &[&'a HashMap<String, String>]) -> Option<&'a str> {
    layers
        .first()
        .and_then(|commands| commands.get("initcommand"))
        .map(String::as_str)
}

pub fn itg_receptor_reverse_behaviors(
    layers: &[&HashMap<String, String>],
) -> (ReceptorReverseBehavior, ReceptorReverseBehavior) {
    let off = layers
        .first()
        .map(|commands| receptor_reverse_behavior(commands))
        .unwrap_or_default();
    let glow = layers
        .get(1)
        .map(|commands| receptor_reverse_behavior(commands))
        .unwrap_or_default();
    (off, glow)
}

pub fn receptor_pulse_from_script(command: &str) -> ReceptorPulse {
    let mut pulse = ReceptorPulse::default();
    let command = normalized_script_command(command);
    for raw_token in command.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((cmd, args)) = split_script_token(token) else {
            continue;
        };
        if let Some(effect_mod) = parse_script_effect_mod(cmd.as_str(), &args) {
            match effect_mod {
                ScriptEffectMod::EffectColor1(color) => pulse.effect_color1 = color,
                ScriptEffectMod::EffectColor2(color) => pulse.effect_color2 = color,
                ScriptEffectMod::EffectPeriod(v) => {
                    let period = v.max(f32::EPSILON);
                    pulse.effect_period = period;
                    pulse.ramp_to_half = period * 0.5;
                    pulse.hold_at_half = 0.0;
                    pulse.ramp_to_full = period * 0.5;
                    pulse.hold_at_full = 0.0;
                    pulse.hold_at_zero = 0.0;
                }
                ScriptEffectMod::EffectOffset(v) => {
                    pulse.effect_offset = v;
                }
                ScriptEffectMod::EffectTiming(v) => {
                    pulse.ramp_to_half = v[0].max(0.0);
                    pulse.hold_at_half = v[1].max(0.0);
                    pulse.ramp_to_full = v[2].max(0.0);
                    pulse.hold_at_full = v[3].max(0.0);
                    pulse.hold_at_zero = v[4].max(0.0);
                    pulse.effect_period = pulse.total_period().max(f32::EPSILON);
                }
                _ => {}
            }
        }
    }

    pulse
}

fn receptor_arrow_command(
    metrics: &IniData,
    commands: Option<&HashMap<String, String>>,
    actor_key: &str,
    metric_key: &str,
) -> Option<String> {
    commands
        .and_then(|commands| commands.get(actor_key).cloned())
        .or_else(|| metrics.get("ReceptorArrow", metric_key).map(str::to_string))
}

pub fn receptor_step_behavior_for_command(
    command: Option<String>,
    base_zoom: f32,
) -> ReceptorStepBehavior {
    let Some(command) = command else {
        return ReceptorStepBehavior::identity();
    };
    let none = itg_parse_command_effect(&command);
    let Some(zoom_start) = none.start_zoom.or(none.target_zoom) else {
        return ReceptorStepBehavior {
            interrupts: none.interrupts,
            ..ReceptorStepBehavior::identity()
        };
    };
    let zoom_end = none.target_zoom.or(none.start_zoom).unwrap_or(zoom_start);
    if (zoom_end - zoom_start).abs() <= f32::EPSILON {
        return ReceptorStepBehavior {
            interrupts: none.interrupts,
            ..ReceptorStepBehavior::identity()
        };
    }
    let base_zoom = if base_zoom.abs() > f32::EPSILON {
        base_zoom
    } else {
        1.0
    };

    ReceptorStepBehavior {
        duration: none.duration.max(0.0),
        zoom_start: (zoom_start / base_zoom).max(0.0),
        zoom_end: (zoom_end / base_zoom).max(0.0),
        tween: if none.duration > f32::EPSILON {
            none.tween
        } else {
            TweenType::Linear
        },
        interrupts: none.interrupts,
    }
}

pub fn receptor_step_behaviors(
    metrics: &IniData,
    commands: Option<&HashMap<String, String>>,
    base_zoom: f32,
) -> ReceptorStepBehaviors {
    let behavior = |actor_key, metric_key| {
        receptor_step_behavior_for_command(
            receptor_arrow_command(metrics, commands, actor_key, metric_key),
            base_zoom,
        )
    };
    ReceptorStepBehaviors::new(
        behavior("nonecommand", "NoneCommand"),
        behavior("misscommand", "MissCommand"),
        [
            behavior("w1command", "W1Command"),
            behavior("w2command", "W2Command"),
            behavior("w3command", "W3Command"),
            behavior("w4command", "W4Command"),
            behavior("w5command", "W5Command"),
        ],
    )
}

pub fn receptor_reverse_behavior(commands: &HashMap<String, String>) -> ReceptorReverseBehavior {
    ReceptorReverseBehavior {
        reverse_off: commands
            .get("reverseoffcommand")
            .map(|script| receptor_reverse_state(script))
            .unwrap_or_default(),
        reverse_on: commands
            .get("reverseoncommand")
            .map(|script| receptor_reverse_state(script))
            .unwrap_or_default(),
    }
}

pub fn receptor_reverse_state(script: &str) -> ReceptorReverseState {
    let mut out = ReceptorReverseState::default();
    let script = normalized_script_command(script);
    for raw_token in script.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((cmd, args)) = split_script_token(token) else {
            continue;
        };
        match cmd.as_str() {
            "baserotationz" => {
                out.base_rotation_z = args.first().and_then(|v| parse_script_number(v));
            }
            "vertalign" | "valign" => {
                out.vert_align = args.first().and_then(|v| parse_script_vertalign(v));
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn receptor_pulse_effecttiming_recalculates_period() {
        let pulse = receptor_pulse_from_script(
            "effectclock,'beat';diffuseramp;effectcolor1,0.1,0.1,0.1,1;\
             effectcolor2,1,1,1,1;effectperiod,0.5;\
             effecttiming,0.25,0.50,0,0.25;effectoffset,-0.25",
        );

        assert!((pulse.effect_period - 1.0).abs() <= 1e-6);
        let beat_0 = pulse.color_for_beat(0.0);
        let beat_half = pulse.color_for_beat(0.5);
        let beat_1 = pulse.color_for_beat(1.0);
        assert!((beat_0[0] - beat_1[0]).abs() <= 1e-6);
        assert!((beat_0[0] - beat_half[0]).abs() > 0.2);
    }

    #[test]
    fn receptor_glow_behavior_uses_press_then_lift_policy() {
        let behavior = receptor_glow_behavior_from_commands(
            "diffusealpha,0.4;zoom,0.8;blend,BlendMode_Add",
            "linear,0.1;diffusealpha,1;zoom,1.2",
            "decelerate,0.3;diffusealpha,0;zoom,0.9",
            "linear,0.2;diffusealpha,0.2;zoom,1.1",
        );

        assert!((behavior.press_duration - 0.1).abs() <= f32::EPSILON);
        assert!((behavior.press_alpha_start - 1.0).abs() <= f32::EPSILON);
        assert!((behavior.press_alpha_end - 1.0).abs() <= f32::EPSILON);
        assert!((behavior.press_zoom_start - 1.2).abs() <= f32::EPSILON);
        assert!((behavior.press_zoom_end - 1.2).abs() <= f32::EPSILON);
        assert!((behavior.duration - 0.3).abs() <= f32::EPSILON);
        assert!((behavior.alpha_start - 1.0).abs() <= f32::EPSILON);
        assert!((behavior.alpha_end - 0.0).abs() <= f32::EPSILON);
        assert!((behavior.zoom_start - 1.2).abs() <= f32::EPSILON);
        assert!((behavior.zoom_end - 0.9).abs() <= f32::EPSILON);
        assert!(behavior.blend_add);
    }

    #[test]
    fn receptor_overlay_commands_prefer_actor_then_metrics() {
        let commands = HashMap::from([
            ("initcommand".to_string(), "actor-init".to_string()),
            ("presscommand".to_string(), "actor-press".to_string()),
        ]);
        let overlay =
            receptor_overlay_commands(Some(&commands), |key| Some(format!("metric-{key}")));

        assert_eq!(
            overlay,
            ReceptorOverlayCommands {
                init: "actor-init".to_string(),
                press: "actor-press".to_string(),
                lift: "metric-LiftCommand".to_string(),
                none: "metric-NoneCommand".to_string(),
            }
        );
    }

    #[test]
    fn itg_receptor_visuals_use_actor_layers_before_fallbacks() {
        let visuals = itg_receptor_visuals(&[1, 2], || Some(10), || Some(20), || Some(30));

        assert_eq!(visuals.off, Some(1));
        assert_eq!(visuals.glow, Some(2));

        let single = itg_receptor_visuals(&[1], || Some(10), || Some(20), || Some(30));

        assert_eq!(single.off, Some(1));
        assert_eq!(single.glow, None);
    }

    #[test]
    fn itg_receptor_visuals_use_texture_fallbacks_only_without_actors() {
        let visuals = itg_receptor_visuals::<i32>(&[], || Some(10), || Some(20), || Some(30));

        assert_eq!(visuals.off, Some(10));
        assert_eq!(visuals.glow, Some(20));

        let glow_only = itg_receptor_visuals::<i32>(&[], || Some(10), || None, || Some(30));

        assert_eq!(glow_only.off, Some(10));
        assert_eq!(glow_only.glow, Some(30));
    }

    #[test]
    fn itg_receptor_pulse_command_uses_first_layer_init() {
        let first = HashMap::from([("initcommand".to_string(), "first-init".to_string())]);
        let second = HashMap::from([("initcommand".to_string(), "second-init".to_string())]);

        assert_eq!(
            itg_receptor_pulse_command(&[&first, &second]),
            Some("first-init")
        );
        assert_eq!(itg_receptor_pulse_command(&[&HashMap::new()]), None);
    }

    #[test]
    fn itg_receptor_reverse_behaviors_use_first_two_layers() {
        let first = HashMap::from([(
            "reverseoffcommand".to_string(),
            "baserotationz,90".to_string(),
        )]);
        let second = HashMap::from([(
            "reverseoncommand".to_string(),
            "baserotationz,180".to_string(),
        )]);

        let (off, glow) = itg_receptor_reverse_behaviors(&[&first, &second]);

        assert_eq!(off.state(false).base_rotation_z, Some(90.0));
        assert_eq!(off.state(true).base_rotation_z, None);
        assert_eq!(glow.state(false).base_rotation_z, None);
        assert_eq!(glow.state(true).base_rotation_z, Some(180.0));
    }

    #[test]
    fn receptor_step_behavior_normalizes_zoom_by_base_zoom() {
        let behavior =
            receptor_step_behavior_for_command(Some("zoom,2;linear,0.25;zoom,4".to_string()), 2.0);

        assert!((behavior.duration - 0.25).abs() <= f32::EPSILON);
        assert!((behavior.zoom_start - 1.0).abs() <= f32::EPSILON);
        assert!((behavior.zoom_end - 2.0).abs() <= f32::EPSILON);
        assert!((behavior.sample_zoom(0.125) - 1.5).abs() <= f32::EPSILON);
    }

    #[test]
    fn receptor_reverse_behavior_parses_layer_commands() {
        let commands = HashMap::from([
            (
                "reverseoffcommand".to_string(),
                "baserotationz,180;vertalign,bottom".to_string(),
            ),
            (
                "reverseoncommand".to_string(),
                "baserotationz,0;vertalign,top".to_string(),
            ),
        ]);

        let behavior = receptor_reverse_behavior(&commands);

        assert_eq!(behavior.reverse_off.base_rotation_z, Some(180.0));
        assert_eq!(behavior.reverse_off.vert_align, Some(1.0));
        assert_eq!(behavior.reverse_on.base_rotation_z, Some(0.0));
        assert_eq!(behavior.reverse_on.vert_align, Some(0.0));
    }
}
