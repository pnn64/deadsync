// Frozen from 1a2129e19 (0.5.1652).
use crate::script::*;

#[must_use]
pub fn itg_active_model_commands(
    commands: &HashMap<String, String>,
    active_key: &str,
) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Some(value) = commands.get("initcommand") {
        out.insert("initcommand".to_string(), value.clone());
    }
    if let Some(value) = commands.get(active_key) {
        out.insert("nonecommand".to_string(), value.clone());
    }
    out
}

pub fn model_draw_program(
    commands: &HashMap<String, String>,
) -> (ModelDrawState, Arc<[ModelTweenSegment]>, ModelEffectState) {
    let mut state = ModelDrawState::default();
    let mut base_zoom = [1.0; 3];
    let mut effect = ModelEffectState::default();
    let mut timeline: Vec<ModelTweenSegment> = Vec::new();
    let mut cursor_time = 0.0f32;
    let mut pending_tween: Option<(f32, TweenType)> = None;
    let mut grouped_mods: Vec<ItgActorMod> = Vec::new();

    let flush_group = |state: &mut ModelDrawState,
                       timeline: &mut Vec<ModelTweenSegment>,
                       cursor_time: &mut f32,
                       pending_tween: &mut Option<(f32, TweenType)>,
                       grouped_mods: &mut Vec<ItgActorMod>| {
        if grouped_mods.is_empty() {
            return;
        }
        if let Some((duration, tween)) = pending_tween.take()
            && duration > f32::EPSILON
        {
            let from = *state;
            let mut to = from;
            itg_apply_actor_mods(&mut to, grouped_mods);
            timeline.push(ModelTweenSegment {
                start: *cursor_time,
                duration,
                tween,
                from,
                to,
            });
            *state = to;
            *cursor_time += duration;
            grouped_mods.clear();
            return;
        }
        let from = *state;
        itg_apply_actor_mods(state, grouped_mods);
        if !timeline.is_empty() {
            timeline.push(ModelTweenSegment {
                start: *cursor_time,
                duration: 0.0,
                tween: TweenType::Linear,
                from,
                to: *state,
            });
        }
        grouped_mods.clear();
    };

    for key in ["initcommand", "nonecommand"] {
        let Some(script) = commands.get(key) else {
            continue;
        };
        let script = normalized_script_command(script);
        for raw in script.split(';') {
            let token = raw.trim();
            if token.is_empty() {
                continue;
            }
            let Some(token) = split_script_token(token) else {
                continue;
            };
            let command = token.command();
            let args = token.args();
            // Actor base scale is immediate and independent of tweened zoom.
            if apply_base_zoom(&mut base_zoom, command, args) {
                continue;
            }
            // Sprite UV state and static depth masks are applied by the asset
            // loader. They are not model transforms and must not split a tween
            // group or be reported as unsupported on this second pass.
            if matches!(
                command,
                ScriptCommand::ZTest
                    | ScriptCommand::ZWrite
                    | ScriptCommand::ClearZBuffer
                    | ScriptCommand::CustomTextureRect
                    | ScriptCommand::TexCoordVelocity
                    | ScriptCommand::SetSecondsIntoAnimation
                    | ScriptCommand::Rate
            ) {
                continue;
            }
            if let Some((tween, duration)) = parse_script_tween(command, args) {
                flush_group(
                    &mut state,
                    &mut timeline,
                    &mut cursor_time,
                    &mut pending_tween,
                    &mut grouped_mods,
                );
                pending_tween = Some((duration.max(0.0), tween_type_from_script_tween(tween)));
                continue;
            }
            if let Some(duration) = parse_script_sleep(command, args) {
                flush_group(
                    &mut state,
                    &mut timeline,
                    &mut cursor_time,
                    &mut pending_tween,
                    &mut grouped_mods,
                );
                let duration = duration.max(0.0);
                timeline.push(ModelTweenSegment {
                    start: cursor_time,
                    duration,
                    tween: TweenType::Linear,
                    from: state,
                    to: state,
                });
                cursor_time += duration;
                continue;
            }
            if command == ScriptCommand::EffectClock {
                flush_group(
                    &mut state,
                    &mut timeline,
                    &mut cursor_time,
                    &mut pending_tween,
                    &mut grouped_mods,
                );
                let raw_clock = args.first().copied().unwrap_or("time");
                effect.clock = if let Some(clock) = parse_script_effect_clock(raw_clock) {
                    clock
                } else {
                    warn!("unsupported effectclock '{raw_clock}' in model DSL path");
                    ModelEffectClock::Time
                };
                continue;
            }
            if let Some(effect_mod) = parse_script_effect_mod(command, args) {
                flush_group(
                    &mut state,
                    &mut timeline,
                    &mut cursor_time,
                    &mut pending_tween,
                    &mut grouped_mods,
                );
                match effect_mod {
                    ScriptEffectMod::DiffuseRamp => {
                        effect.mode = ModelEffectMode::DiffuseRamp;
                        effect.period = 1.0;
                        effect.timing = [0.5, 0.0, 0.5, 0.0, 0.0];
                        effect.color1 = [0.0, 0.0, 0.0, 1.0];
                        effect.color2 = [1.0, 1.0, 1.0, 1.0];
                    }
                    ScriptEffectMod::DiffuseShift => {
                        effect.mode = ModelEffectMode::DiffuseShift;
                        effect.period = 1.0;
                        effect.timing = [0.5, 0.0, 0.5, 0.0, 0.0];
                        effect.color1 = [0.0, 0.0, 0.0, 1.0];
                        effect.color2 = [1.0, 1.0, 1.0, 1.0];
                    }
                    ScriptEffectMod::GlowShift => {
                        effect.mode = ModelEffectMode::GlowShift;
                        effect.period = 1.0;
                        effect.timing = [0.5, 0.0, 0.5, 0.0, 0.0];
                        effect.color1 = [1.0, 1.0, 1.0, 0.2];
                        effect.color2 = [1.0, 1.0, 1.0, 0.8];
                    }
                    ScriptEffectMod::Pulse => {
                        effect.mode = ModelEffectMode::Pulse;
                        effect.period = 2.0;
                        effect.timing = [1.0, 0.0, 1.0, 0.0, 0.0];
                        effect.magnitude = [0.5, 1.0, 0.0];
                    }
                    ScriptEffectMod::Thump(period) => {
                        // Themes/_fallback/Scripts/02 Actor.lua: Actor:thump.
                        let period = period.max(f32::EPSILON);
                        effect.mode = ModelEffectMode::Pulse;
                        effect.period = period;
                        effect.timing = [0.0, 0.0, 0.75 * period, 0.0, 0.25 * period];
                        effect.magnitude = [1.0, 1.125, 1.0];
                    }
                    ScriptEffectMod::Spin => {
                        effect.mode = ModelEffectMode::Spin;
                        effect.magnitude = [0.0, 0.0, 180.0];
                    }
                    ScriptEffectMod::StopEffect => {
                        effect.mode = ModelEffectMode::None;
                    }
                    ScriptEffectMod::EffectColor1(c) => {
                        effect.color1 = c;
                    }
                    ScriptEffectMod::EffectColor2(c) => {
                        effect.color2 = c;
                    }
                    ScriptEffectMod::EffectPeriod(v) => {
                        if v > 0.0 {
                            effect.period = v;
                            effect.timing = [v * 0.5, 0.0, v * 0.5, 0.0, 0.0];
                        }
                    }
                    ScriptEffectMod::EffectOffset(v) => {
                        effect.offset = v;
                    }
                    ScriptEffectMod::EffectTiming(v) => {
                        let timing = [
                            v[0].max(0.0),
                            v[1].max(0.0),
                            v[2].max(0.0),
                            v[3].max(0.0),
                            v[4].max(0.0),
                        ];
                        let total = timing[0] + timing[1] + timing[2] + timing[3] + timing[4];
                        if total > 0.0 {
                            effect.timing = timing;
                            effect.period = total;
                        }
                    }
                    ScriptEffectMod::EffectMagnitude(v) => {
                        effect.magnitude = v;
                    }
                }
                continue;
            }
            if let Some(control) = parse_script_control(command) {
                if control != ScriptControl::SetAllStateDelays {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                }
                continue;
            }
            if let Some(mod_cmd) = parse_script_actor_mod(command, args) {
                grouped_mods.push(mod_cmd);
            } else {
                warn!("unsupported noteskin actor command in model DSL path: '{command}'");
            }
        }
    }

    flush_group(
        &mut state,
        &mut timeline,
        &mut cursor_time,
        &mut pending_tween,
        &mut grouped_mods,
    );

    // Bake the final base scale into every tween endpoint. SetBaseZoom changes
    // the whole actor immediately, even when invoked after a queued tween.
    for draw in std::iter::once(&mut state).chain(
        timeline
            .iter_mut()
            .flat_map(|segment| [&mut segment.from, &mut segment.to]),
    ) {
        for (zoom, base) in draw.zoom.iter_mut().zip(base_zoom) {
            *zoom *= base;
        }
    }

    // Preserve scale signs for sprite mirroring at load time. Model sampling
    // sanitizes scale after evaluating the timeline, including its final state.
    state.tint[0] = state.tint[0].clamp(0.0, 1.0);
    state.tint[1] = state.tint[1].clamp(0.0, 1.0);
    state.tint[2] = state.tint[2].clamp(0.0, 1.0);
    state.tint[3] = state.tint[3].clamp(0.0, 1.0);
    state.glow[0] = state.glow[0].clamp(0.0, 1.0);
    state.glow[1] = state.glow[1].clamp(0.0, 1.0);
    state.glow[2] = state.glow[2].clamp(0.0, 1.0);
    state.glow[3] = state.glow[3].clamp(0.0, 1.0);
    for fade in &mut state.fade {
        *fade = fade.clamp(0.0, 1.0);
    }

    (state, Arc::from(timeline), effect)
}
