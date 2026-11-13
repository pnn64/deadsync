//! Native actor effect selection, phase and spin, before tween advancement.
use super::{Actor, Host, arg, float_scalar, helper, render, table};
use crate::{Clock, SongLuaOverlayState, SongLuaOverlayUpdateTarget as Target};
use deadlib_lua::{Error, HostCall, TableId, Value};
use deadlib_present::anim::{EffectClock, EffectMode};
use std::sync::atomic::Ordering;

pub(super) const METHODS: &[&str] = &[
    "diffuseblink",
    "diffuseshift",
    "diffuseramp",
    "glowblink",
    "glowshift",
    "glowramp",
    "rainbow",
    "wag",
    "bounce",
    "bob",
    "pulse",
    "spin",
    "vibrate",
    "stopeffect",
    "effectcolor1",
    "effectcolor2",
    "effectperiod",
    "effecttiming",
    "effect_hold_at_full",
    "effectoffset",
    "effectclock",
    "effectmagnitude",
    "geteffectmagnitude",
    "GetSecsIntoEffect",
    "GetEffectDelta",
];

#[derive(Clone, Copy, Default)]
pub(super) struct State {
    // None preserves getters' runtime-clock fallback before the first update.
    motion: Option<Motion>,
    clock_visible: bool,
    spin_seen: bool,
    pub(super) spin_baked: bool,
    replay_epoch: Option<f32>,
}

pub(super) fn snapshot_position_ready(actor: &Actor) -> bool {
    !actor.effects.spin_seen
        && actor.state.render.effect_mode == EffectMode::None
        && !actor.state.render.rainbow
}

pub(super) fn prepare_spin_capture(actor: &mut Actor) {
    if actor.effects.spin_seen {
        super::capture::start_spin(actor);
    }
}

pub(super) fn sync_preparation_motion(actor: &mut Actor, epoch: f32, seconds: f32) {
    if actor.state.render.effect_timer
        && let Some(motion) = actor.effects.motion.as_mut()
        && actor.effects.replay_epoch != Some(epoch)
    {
        actor.effects.replay_epoch = Some(epoch);
        motion.units = Some((seconds - epoch).max(0.));
    }
}

#[derive(Clone, Copy, Default)]
struct Motion {
    // A failed first external-clock read leaves the clock created but without
    // units, as in the production host. Its delta getter still defaults to zero.
    units: Option<f32>,
    delta: f32,
}

fn set_period(render: &mut SongLuaOverlayState, period: f32) {
    render.effect_period = period;
    if render.effect_timing.is_some() {
        render.effect_timing = Some([period / 2.0, 0.0, period / 2.0, 0.0, 0.0]);
    }
}

fn select(actor: &mut Actor, name: &str) -> smallvec::SmallVec<[Target; 8]> {
    let mut fields = smallvec::smallvec![Target::Vibrate, Target::Rainbow, Target::EffectMode];
    if name == "spin" {
        super::capture::start_spin(actor);
    }
    let render = &mut actor.state.render;
    let mode = crate::parse_overlay_effect_mode(name).unwrap_or(EffectMode::None);
    let motion = matches!(name, "bob" | "bounce" | "wag");
    let color = matches!(
        name,
        "diffuseblink" | "diffuseshift" | "diffuseramp" | "glowblink" | "glowshift" | "glowramp"
    );
    let reset = (motion
        && (name == "bounce"
            || mode != render.effect_mode
            || (name == "bob" && render.effect_period != 2.0)))
        || (color && mode != render.effect_mode)
        || (name == "rainbow" && !render.rainbow);
    if motion || color || matches!(name, "pulse" | "rainbow") {
        actor.effects.clock_visible = true;
        let clock = actor.effects.motion.get_or_insert(Motion {
            units: Some(0.0),
            delta: 0.0,
        });
        if reset {
            clock.units = Some(0.0);
        }
    }
    render.effect_mode = mode;
    render.vibrate = name == "vibrate";
    render.rainbow = name == "rainbow";
    match name {
        "glowblink" | "glowshift" | "glowramp" => {
            set_period(render, 1.0);
            fields.push(Target::EffectPeriod);
            if render.effect_timing.is_some() {
                fields.push(Target::EffectTiming);
            }
            fields.extend([Target::EffectColor1, Target::EffectColor2]);
            render.effect_color1 = [1.0, 1.0, 1.0, 0.2];
            render.effect_color2 = [1.0, 1.0, 1.0, 0.8];
        }
        "diffuseblink" | "diffuseshift" | "diffuseramp" => {
            set_period(render, 1.0);
            fields.push(Target::EffectPeriod);
            if render.effect_timing.is_some() {
                fields.push(Target::EffectTiming);
            }
            fields.extend([Target::EffectColor1, Target::EffectColor2]);
            render.effect_color1 = if name == "diffuseblink" {
                [0.5; 4]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            };
            render.effect_color2 = [1.0; 4];
        }
        "bob" | "bounce" | "wag" | "pulse" => {
            if name != "bob" || reset {
                set_period(render, 2.0);
                fields.push(Target::EffectPeriod);
                if render.effect_timing.is_some() {
                    fields.push(Target::EffectTiming);
                }
            }
            fields.push(Target::EffectMagnitude);
            render.effect_magnitude = match name {
                "pulse" => [0.5, 1.0, 1.0],
                "wag" => [0.0, 0.0, 20.0],
                _ => [0.0, 20.0, 0.0],
            };
        }
        "rainbow" => {
            set_period(render, 2.0);
            fields.push(Target::EffectPeriod);
            if render.effect_timing.is_some() {
                fields.push(Target::EffectTiming);
            }
        }
        "spin" => {
            fields.push(Target::EffectMagnitude);
            actor.effects.spin_seen = true;
            render.effect_magnitude = [0.0, 0.0, 180.0];
        }
        "vibrate" => {
            fields.push(Target::EffectMagnitude);
            render.effect_magnitude = [10.0; 3];
        }
        _ => {}
    }
    fields
}

fn number(call: &mut HostCall<'_>, value: Value) -> Result<f64, Error> {
    if let Value::Number(value) = value {
        return Ok(value);
    }
    match call.call(helper(call, 6)?, &[value])?[0] {
        Value::Number(value) => Ok(value),
        _ => Err(call.error(b"number expected")),
    }
}

fn field(call: &mut HostCall<'_>, runtime: Value, key: &str) -> Result<f64, Error> {
    let key = call.try_intern(key.as_bytes())?;
    let value = call.table_get(runtime, key)?;
    number(call, value)
}

fn runtime<T>(
    call: &mut HostCall<'_>,
    read: impl FnOnce(&mut HostCall<'_>, Value) -> Result<T, Error>,
) -> Result<T, Error> {
    let key = call.try_intern(crate::SONG_LUA_RUNTIME_KEY.as_bytes())?;
    let runtime = call.table_get(helper(call, 5)?, key)?;
    table(call, runtime)?;
    // A global __index may synthesize a table that detaches itself and collects
    // while a later field is read. Root the selected table for the whole read.
    call.with_roots(&[runtime], |call| read(call, runtime))
}

pub(super) fn values(call: &mut HostCall<'_>, delta: bool) -> Result<[f32; 2], Error> {
    runtime(call, |call, runtime| {
        let (beat, seconds) = if delta {
            (
                crate::SONG_LUA_RUNTIME_DELTA_BEAT_KEY,
                crate::SONG_LUA_RUNTIME_DELTA_SECONDS_KEY,
            )
        } else {
            (
                crate::SONG_LUA_RUNTIME_BEAT_KEY,
                crate::SONG_LUA_RUNTIME_SECONDS_KEY,
            )
        };
        Ok([
            field(call, runtime, beat)? as f32,
            field(call, runtime, seconds)? as f32,
        ])
    })
}

fn music(call: &mut HostCall<'_>, clock: &Clock) -> Result<f32, Error> {
    runtime(call, |call, runtime| {
        let seconds = field(call, runtime, crate::SONG_LUA_RUNTIME_SECONDS_KEY)?;
        let rate = field(call, runtime, crate::SONG_LUA_RUNTIME_RATE_KEY)? as f32;
        let seconds = if clock.live.load(Ordering::Relaxed) {
            seconds as f32
        } else {
            (seconds * f64::from(rate)) as f32
                + clock
                    .timing
                    .as_ref()
                    .map_or(0.0, |clock| clock.get_time_for_beat_exact(0.0))
        };
        // Preserve the production integer-valued-float path (notably -0).
        Ok(if seconds.fract() == 0.0 && seconds.abs() < 1e14 {
            (seconds as i64) as f32
        } else {
            seconds
        })
    })
}

pub(super) fn method(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<(), Error> {
    let value = arg(call, 1);
    let mut fields = smallvec::SmallVec::<[Target; 8]>::new();
    match name {
        "GetSecsIntoEffect" | "GetEffectDelta" => {
            let (state, clock) = {
                let store = call.store();
                let actor = &store.actors[&actor];
                (actor.effects, actor.state.render.effect_clock)
            };
            let delta = name == "GetEffectDelta";
            let value = match state.motion {
                Some(motion) if delta => motion.delta,
                Some(motion) => motion
                    .units
                    .ok_or_else(|| call.error(b"effect clock has no units"))?,
                None => values(call, delta)?[usize::from(clock == EffectClock::Time)],
            };
            return call.push(Value::Number(value as f64));
        }
        "geteffectmagnitude" => {
            let values = call.store().actors[&actor].state.render.effect_magnitude;
            for value in values {
                call.push(Value::Number(value as f64))?;
            }
            return Ok(());
        }
        "effectcolor1" | "effectcolor2" => {
            let color = render::color(call)?.unwrap_or([1.0; 4]);
            super::action_capture::prepare_write(call, actor)?;
            let store = call.store();
            let render = &mut store.actors.get_mut(&actor).unwrap().state.render;
            if name == "effectcolor1" {
                render.effect_color1 = color;
                fields.push(Target::EffectColor1);
            } else {
                render.effect_color2 = color;
                fields.push(Target::EffectColor2);
            }
        }
        "effectperiod" | "effectoffset" => {
            if let Some(value) = float_scalar(call, value, false) {
                super::action_capture::prepare_write(call, actor)?;
                let store = call.store();
                let render = &mut store.actors.get_mut(&actor).unwrap().state.render;
                if name == "effectperiod" {
                    set_period(render, value);
                    fields.push(Target::EffectPeriod);
                    if render.effect_timing.is_some() {
                        fields.push(Target::EffectTiming);
                    }
                } else {
                    render.effect_offset = value;
                    fields.push(Target::EffectOffset);
                }
            }
        }
        "effectmagnitude" => {
            let magnitude =
                [1, 2, 3].map(|i| float_scalar(call, arg(call, i), false).unwrap_or(0.0));
            super::action_capture::prepare_write(call, actor)?;
            call.store()
                .actors
                .get_mut(&actor)
                .unwrap()
                .state
                .render
                .effect_magnitude = magnitude;
            fields.push(Target::EffectMagnitude);
        }
        "effecttiming" => {
            let mut timing = [0.0; 5];
            for (i, index) in [0, 1, 2, 4].into_iter().enumerate() {
                let Some(value) = float_scalar(call, arg(call, i + 1), false) else {
                    return call.push(Value::Table(actor));
                };
                timing[index] = value.max(0.0);
            }
            timing[3] = float_scalar(call, arg(call, 5), false)
                .unwrap_or(0.0)
                .max(0.0);
            let total = timing.iter().sum::<f32>();
            if total > 0.0 {
                super::action_capture::prepare_write(call, actor)?;
                let store = call.store();
                let render = &mut store.actors.get_mut(&actor).unwrap().state.render;
                render.effect_timing = Some(timing);
                render.effect_period = total;
                fields.extend([Target::EffectTiming, Target::EffectPeriod]);
            }
        }
        "effect_hold_at_full" => {
            let hold = number(call, value)? as f32;
            let render = call.store().actors[&actor].state.render;
            let period = render.effect_period;
            let mut timing =
                render
                    .effect_timing
                    .unwrap_or([period / 2.0, 0.0, period / 2.0, 0.0, 0.0]);
            timing[3] = hold;
            let total = timing.iter().sum::<f32>();
            if hold < 0.0 || total <= 0.0 {
                return Err(call.error(b"effect timings must be nonnegative and not all zero"));
            }
            super::action_capture::prepare_write(call, actor)?;
            let guard = call.store();
            let render = &mut guard.actors.get_mut(&actor).unwrap().state.render;
            render.effect_timing = Some(timing);
            render.effect_period = total;
            fields.extend([Target::EffectTiming, Target::EffectPeriod]);
        }
        "effectclock" => {
            if let Some(raw) = render::text(call, value)
                && let Some(clock) = crate::parse_overlay_effect_clock(raw)
            {
                let no_offset = raw.eq_ignore_ascii_case("beatnooffset");
                let timer = raw.eq_ignore_ascii_case("timer");
                super::action_capture::prepare_write(call, actor)?;
                let store = call.store();
                let render = &mut store.actors.get_mut(&actor).unwrap().state.render;
                render.effect_clock = clock;
                render.effect_no_offset = no_offset;
                render.effect_timer = timer;
                fields.extend([
                    Target::EffectClock,
                    Target::EffectNoOffset,
                    Target::EffectTimer,
                ]);
            }
        }
        _ => {
            super::action_capture::prepare_write(call, actor)?;
            fields = select(call.store().actors.get_mut(&actor).unwrap(), name);
        }
    }
    if !fields.is_empty() {
        let state = call.store().actors[&actor].state.render;
        for target in fields {
            let mut delta = crate::SongLuaOverlayStateDelta::default();
            match target {
                Target::Vibrate => delta.vibrate = Some(state.vibrate),
                Target::Rainbow => delta.rainbow = Some(state.rainbow),
                Target::EffectMode => delta.effect_mode = Some(state.effect_mode),
                Target::EffectColor1 => delta.effect_color1 = Some(state.effect_color1),
                Target::EffectColor2 => delta.effect_color2 = Some(state.effect_color2),
                Target::EffectPeriod => delta.effect_period = Some(state.effect_period),
                Target::EffectMagnitude => delta.effect_magnitude = Some(state.effect_magnitude),
                Target::EffectOffset => delta.effect_offset = Some(state.effect_offset),
                Target::EffectClock => delta.effect_clock = Some(state.effect_clock),
                Target::EffectNoOffset => delta.effect_no_offset = Some(state.effect_no_offset),
                Target::EffectTimer => delta.effect_timer = Some(state.effect_timer),
                Target::EffectTiming => delta.effect_timing = state.effect_timing,
                _ => unreachable!("effect setter published a non-effect target"),
            }
            super::capture::write(call, actor, delta, true);
            if target == Target::EffectClock {
                super::sprites::set(call, actor, "__songlua_state_effect_clock", value)?;
            } else {
                let value = crate::perframe::overlay_state_update_value(&state, target);
                super::state_snapshot::publish(call, actor, target, &value)?;
            }
        }
    }
    call.push(Value::Table(actor))
}

pub(super) fn advance(call: &mut HostCall<'_>, actor: TableId, delta: f32) -> Result<(), Error> {
    let (previous, render, clock) = {
        let store = call.store();
        let clock = store.clock.clone();
        let actor = store.actors.get_mut(&actor).unwrap();
        let previous = actor
            .effects
            .motion
            .get_or_insert_with(Motion::default)
            .units
            .unwrap_or(0.0);
        (previous, actor.state.render, clock)
    };
    let (units, elapsed) = if render.effect_timer {
        let mut units = previous + delta;
        // Native Actor subtracts once, with strict >, rather than modulo.
        if units > render.effect_period {
            units -= render.effect_period;
        }
        (units, delta)
    } else {
        let units = if render.effect_clock == EffectClock::Beat {
            if render.effect_no_offset {
                let seconds = music(call, &clock)?;
                // The production fallback evaluates the runtime beat even if
                // timing data supplies the selected no-offset position.
                let fallback = values(call, false)?[0];
                clock.no_offset.as_ref().map_or(fallback, |timing| {
                    timing.get_song_position_no_offset(seconds).beat
                })
            } else {
                values(call, false)?[0]
            }
        } else {
            music(call, &clock)?
        };
        (units, units - previous)
    };
    let store = call.store();
    let fallback = super::song_pose::mode(&store, actor);
    let actor = store.actors.get_mut(&actor).unwrap();
    actor.effects.motion = Some(Motion {
        units: Some(units),
        delta: elapsed,
    });
    if delta != 0.
        || render.rainbow
        || render.effect_mode != EffectMode::None
        || (!render.effect_timer && render.effect_clock == EffectClock::Time)
    {
        actor.effects.clock_visible = true;
    }
    if actor.effects.spin_seen {
        if fallback {
            super::capture::advance_spin(actor, elapsed, delta);
        } else if actor.state.render.effect_mode == EffectMode::Spin {
            let magnitude = actor.state.render.effect_magnitude;
            for (angle, speed) in [
                &mut actor.current.rot_x_deg,
                &mut actor.current.rot_y_deg,
                &mut actor.current.rot_z_deg,
            ]
            .into_iter()
            .zip(magnitude)
            {
                *angle = (*angle + elapsed * speed).rem_euclid(360.0);
            }
        }
        actor.effects.spin_baked = true;
    }
    Ok(())
}

pub(super) fn render_time(actor: &Actor, frame_clock: [f32; 2], render: &mut SongLuaOverlayState) {
    render.spin_baked = actor.effects.spin_baked;
    if (render.rainbow || !matches!(render.effect_mode, EffectMode::None | EffectMode::Spin))
        && let Some(motion) = actor.effects.motion
    {
        render.effect_time = Some([
            frame_clock[usize::from(render.effect_clock == EffectClock::Beat)],
            motion.units.unwrap_or(0.0),
        ]);
    }
}

pub(super) fn has_motion(actor: &Actor) -> bool {
    actor.effects.clock_visible
}
