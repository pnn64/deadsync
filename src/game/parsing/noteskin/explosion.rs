use super::script::{
    ScriptActorMod, ScriptControl, ScriptEffectMod, normalized_script_command,
    parse_script_actor_mod, parse_script_control, parse_script_effect_mod, parse_script_number,
    parse_script_sleep, parse_script_tween, split_script_token, tween_type_from_script_tween,
};
use super::{ExplosionAnimation, ExplosionSegment, ExplosionState, GlowEffect, TweenType};
use log::warn;

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

pub(super) fn parse_explosion_animation(script: &str) -> ExplosionAnimation {
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
