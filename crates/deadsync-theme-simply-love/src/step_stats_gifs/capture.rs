//! GIF capture through the shared Rust VM. No Lua values escape preparation.
use super::{
    CapturedGif, EffectClock, GifActorCommand, GifStyle, LUA_HOOK_INTERVAL, MAX_FRAMES,
    MAX_LUA_INSTRUCTIONS, PlayerSide, actor_command, alignment_name, numbered_key,
};
use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm};
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex},
};

struct Capture {
    style: Mutex<GifStyle>,
    result: Mutex<Option<CapturedGif>>,
}

const INSTALL: &[u8] = br#"
local wide, linear = ...
local setmetatable, type, error = setmetatable, type, error
local function actor(table)
    if type(table) ~= 'table' then error('actor table expected') end
    return table
end
Def = {
    ActorFrame = actor,
    Sprite = function(table)
        actor(table)
        table.__deadsync_sprite = true
        return table
    end,
}
IsUsingWideScreen = function(...) return wide end
Sprite = { LinearFrames = linear }
return function(apply)
    return setmetatable({}, {
        __index = function(_, method)
            return function(self, ...)
                apply(self, method, ...)
                return self
            end
        end,
    })
end
"#;

/// Capture one player/aspect variant in a fresh, memory-bounded VM supplied by
/// the loader. The caller owns VM teardown, including every temporary root.
/// Installation and user execution are separate entries; all user callbacks
/// share one instruction budget, including metamethods used during extraction.
pub(super) fn capture(
    vm: &mut Vm,
    content: &str,
    source: &str,
    side: PlayerSide,
    wide: bool,
) -> Result<CapturedGif, String> {
    let mut roots = Vec::new();
    let result = (|| {
        // Lua 5.1's base library includes coroutine. The original loader opens
        // base, table, string and math, but no package, debug, os or io library.
        for name in [
            b"package".as_slice(),
            b"require",
            b"module",
            b"debug",
            b"os",
            b"io",
        ] {
            vm.set_global(name, Value::Nil).map_err(|e| e.to_string())?;
        }
        let linear = vm
            .new_host_function(linear_frames, &[])
            .map_err(|e| e.to_string())?;
        roots.push(vm.pin(linear).map_err(|e| e.to_string())?);
        let install = Value::Function(
            vm.try_load_source(INSTALL, b"=GIF host")
                .map_err(|e| e.to_string())?,
        );
        let factory = vm
            .call(install, &[Value::Boolean(wide), linear])
            .map_err(|e| e.to_string())?[0];
        roots.push(vm.pin(factory).map_err(|e| e.to_string())?);
        let script = Value::Function(
            vm.try_load_source(content.as_bytes(), source.as_bytes())
                .map_err(|e| format!("Lua compile failed: {e}"))?,
        );
        roots.push(vm.pin(script).map_err(|e| e.to_string())?);
        let state = Arc::new(Capture {
            style: Mutex::new(GifStyle::default()),
            result: Mutex::new(None),
        });
        let apply = vm
            .new_host_function_with_data(apply_command, &[], state.clone())
            .map_err(|e| e.to_string())?;
        roots.push(vm.pin(apply).map_err(|e| e.to_string())?);
        let next = vm.global(b"next");
        let tonumber = vm.global(b"tonumber");
        let entry = vm
            .new_host_function_with_data(
                capture_entry,
                &[script, factory, next, apply, tonumber],
                state.clone(),
            )
            .map_err(|e| e.to_string())?;
        roots.push(vm.pin(entry).map_err(|e| e.to_string())?);
        let player = vm
            .try_intern(match side {
                PlayerSide::P1 => b"PlayerNumber_P1",
                PlayerSide::P2 => b"PlayerNumber_P2",
            })
            .map_err(|e| e.to_string())?;
        // The old 10,000-instruction hook rejects the first interval above the
        // million-instruction allowance. Preserve that total for the entry.
        vm.set_instruction_limit(Some(u64::from(MAX_LUA_INSTRUCTIONS + LUA_HOOK_INTERVAL)));
        vm.call(entry, &[player]).map_err(|error| {
            if error.kind() == ErrorKind::Limit {
                "Step Stats GIF script exceeded its instruction limit".to_owned()
            } else {
                format!("Lua execution failed: {error}")
            }
        })?;
        let captured = state
            .result
            .lock()
            .expect("GIF capture state")
            .take()
            .ok_or_else(|| "missing captured GIF".to_owned());
        captured
    })();
    for root in roots.into_iter().rev() {
        // The only roots released here were created in this VM in this entry.
        vm.unpin(root).expect("GIF temporary root");
    }
    result
}

fn field(call: &mut HostCall<'_>, table: Value, name: &[u8]) -> Result<Value, Error> {
    let key = call.try_intern(name)?;
    call.table_get(table, key)
}

fn text(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    std::str::from_utf8(call.string_bytes(id).expect("live GIF string"))
        .map(str::to_owned)
        .map_err(|_| call.error(b"invalid UTF-8 string"))
}

fn table(call: &HostCall<'_>, value: Value) -> Result<deadlib_lua::TableId, Error> {
    if let Value::Table(id) = value {
        Ok(id)
    } else {
        Err(call.error(b"table expected"))
    }
}

fn linear_frames(call: &mut HostCall<'_>) -> Result<(), Error> {
    let count = call.number(0)?;
    if !(count > -1.0 && count < usize::MAX as f64 + 1.0) {
        return Err(call.error(b"frame count out of range"));
    }
    let count = (count as usize).max(1);
    let seconds = (call.number(1)? as f32).max(0.0);
    let result = call.try_new_table()?;
    let key = call.try_intern(b"__deadsync_count")?;
    call.raw_table_set(result, key, Value::Number(count as f64))?;
    let key = call.try_intern(b"__deadsync_seconds")?;
    call.raw_table_set(result, key, Value::Number(f64::from(seconds)))?;
    call.push(Value::Table(result))
}

fn capture_entry(call: &mut HostCall<'_>) -> Result<(), Error> {
    let [script, factory, next, apply, tonumber] = call.captures() else {
        unreachable!()
    };
    let (script, factory, next, apply, tonumber) = (*script, *factory, *next, *apply, *tonumber);
    let player = call.arguments()[0];
    let root = call
        .call(script, &[player])?
        .first()
        .copied()
        .unwrap_or(Value::Nil);
    call.with_roots(&[root], |call| {
        // Store discovered sprites in a traced table. User metamethods can
        // mutate the input tree or collect between visits to different nodes.
        let sprites = call.try_new_table()?;
        call.with_roots(&[Value::Table(sprites)], |call| {
            let mut count = 0;
            collect_sprites(call, root, sprites, &mut count, &mut HashSet::new())?;
            if count != 1 {
                return Err(call
                    .error(format!("expected exactly one Def.Sprite, found {count}").as_bytes()));
            }
            let sprite = call.raw_table_get(sprites, Value::Number(1.0))?;
            let texture = field(call, sprite, b"Texture")?;
            let texture = text(call, texture)?;
            let (frames, delays) = frames(call, sprite, next, tonumber)?;
            let actor = call.call(factory, &[apply])?[0];
            call.with_roots(&[actor], |call| {
                for name in [b"InitCommand".as_slice(), b"OnCommand"] {
                    let command = field(call, sprite, name)?;
                    if matches!(command, Value::Function(_) | Value::Native(_)) {
                        call.call(command, &[actor])?;
                    }
                }
                Ok(())
            })?;
            let state = call.function_data::<Arc<Capture>>().expect("GIF binding");
            let style = *state.style.lock().expect("GIF style");
            *state.result.lock().expect("GIF capture") = Some(CapturedGif {
                texture,
                frames,
                delays,
                style,
            });
            Ok(())
        })
    })
}

fn collect_sprites(
    call: &mut HostCall<'_>,
    value: Value,
    out: deadlib_lua::TableId,
    count: &mut usize,
    active: &mut HashSet<deadlib_lua::TableId>,
) -> Result<(), Error> {
    let Value::Table(id) = value else {
        return Ok(());
    };
    call.with_roots(&[value], |call| {
        if field(call, value, b"__deadsync_sprite")
            .map(Value::is_truthy)
            .unwrap_or(false)
        {
            *count += 1;
            call.raw_table_set(out, Value::Number(*count as f64), value)?;
            return Ok(());
        }
        if !active.insert(id) {
            return Err(call.error(b"cyclic GIF actor tree"));
        }
        let mut index = 1;
        loop {
            let child = call.raw_table_get(id, Value::Number(index as f64))?;
            if child == Value::Nil {
                break;
            }
            collect_sprites(call, child, out, count, active)?;
            index += 1;
        }
        active.remove(&id);
        Ok(())
    })
}

fn number_f32(call: &HostCall<'_>, value: Value) -> Option<f32> {
    let number = match value {
        Value::Number(number) => number as f32,
        Value::String(id) => std::str::from_utf8(call.string_bytes(id)?)
            .ok()?
            .parse()
            .ok()?,
        _ => return None,
    };
    number.is_finite().then_some(number)
}

fn number_u32(call: &HostCall<'_>, value: Value) -> Option<u32> {
    match value {
        Value::Number(number) if number >= 0.0 => {
            // The Lua 5.1 legacy bridge exposes exact i64 values as integers.
            // Its integer-to-u32 conversion rejects overflow; nonintegral
            // numbers use Rust's saturating float cast instead.
            if number.fract() == 0.0 && number < i64::MAX as f64 {
                u32::try_from(number as u64).ok()
            } else {
                Some(number as u32)
            }
        }
        Value::String(id) => std::str::from_utf8(call.string_bytes(id)?)
            .ok()?
            .parse()
            .ok(),
        _ => None,
    }
}

fn frames(
    call: &mut HostCall<'_>,
    sprite: Value,
    next: Value,
    tonumber: Value,
) -> Result<(Vec<u32>, Vec<f32>), Error> {
    let mut frames = BTreeMap::new();
    let mut delays = BTreeMap::new();
    let mut key = Value::Nil;
    loop {
        let pair = call.call(next, &[sprite, key])?;
        key = pair.first().copied().unwrap_or(Value::Nil);
        if key == Value::Nil {
            break;
        }
        let value = pair.get(1).copied().unwrap_or(Value::Nil);
        if let Value::String(id) = key {
            let name = std::str::from_utf8(call.string_bytes(id).expect("live GIF key"))
                .map_err(|_| call.error(b"invalid UTF-8 frame key"))?;
            if let Some(index) = numbered_key(name, "frame")
                && let Some(frame) = number_u32(call, value)
            {
                frames.insert(index, frame);
            } else if let Some(index) = numbered_key(name, "delay")
                && let Some(delay) = number_f32(call, value)
            {
                delays.insert(index, delay.max(0.0));
            }
        }
    }
    if frames.is_empty() {
        let linear = field(call, sprite, b"Frames")?;
        if matches!(linear, Value::Table(_)) {
            return call.with_roots(&[linear], |call| {
                let count = field(call, linear, b"__deadsync_count")?;
                let count = numeric(call, tonumber, count)?;
                if !(count > -1.0 && count < usize::MAX as f64 + 1.0) {
                    return Err(call.error(b"frame count out of range"));
                }
                let count = count as usize;
                let count = count.min(MAX_FRAMES);
                let seconds = field(call, linear, b"__deadsync_seconds")?;
                let seconds = numeric(call, tonumber, seconds)? as f32;
                Ok((
                    (0..count as u32).collect(),
                    vec![seconds / count.max(1) as f32; count],
                ))
            });
        }
    }
    let count = frames
        .last_key_value()
        .map_or(1, |(&i, _)| i.saturating_add(1));
    if count > MAX_FRAMES {
        return Err(
            call.error(format!("animation has {count} frames; limit is {MAX_FRAMES}").as_bytes())
        );
    }
    let mut result_frames = Vec::with_capacity(count);
    let mut result_delays = Vec::with_capacity(count);
    let mut last = 0;
    let default_delay = delays.get(&0).copied().unwrap_or(1.0);
    for i in 0..count {
        last = frames.get(&i).copied().unwrap_or(last);
        result_frames.push(last);
        result_delays.push(delays.get(&i).copied().unwrap_or(default_delay));
    }
    Ok((result_frames, result_delays))
}

fn numeric(call: &mut HostCall<'_>, tonumber: Value, value: Value) -> Result<f64, Error> {
    match call.call(tonumber, &[value])?.first().copied() {
        Some(Value::Number(number)) => Ok(number),
        _ => Err(call.error(b"number expected")),
    }
}

fn apply_command(call: &mut HostCall<'_>) -> Result<(), Error> {
    table(
        call,
        call.arguments().first().copied().unwrap_or(Value::Nil),
    )?;
    let method = call.arguments().get(1).copied().unwrap_or(Value::Nil);
    let method = text(call, method)?;
    let first = call.arguments().get(2).copied().unwrap_or(Value::Nil);
    let second = call.arguments().get(3).copied().unwrap_or(Value::Nil);
    let first_number = number_f32(call, first);
    let second_number = number_f32(call, second);
    let string = if let Value::String(id) = first {
        std::str::from_utf8(call.string_bytes(id).expect("GIF command string")).ok()
    } else {
        None
    };
    let alignment = first_number.or_else(|| string.and_then(alignment_name));
    let state = call.function_data::<Arc<Capture>>().expect("GIF binding");
    let mut style = state.style.lock().expect("GIF style");
    match actor_command(&method) {
        Some(GifActorCommand::EffectClock) => {
            if let Some(clock) = string {
                style.effect_clock = match clock.to_ascii_lowercase().as_str() {
                    "beat" | "beatnooffset" | "bgm" => EffectClock::Beat,
                    _ => EffectClock::Time,
                };
            }
        }
        Some(GifActorCommand::X) => style.x = first_number.unwrap_or(style.x),
        Some(GifActorCommand::Y) => style.y = first_number.unwrap_or(style.y),
        Some(GifActorCommand::Xy) => {
            style.x = first_number.unwrap_or(style.x);
            style.y = second_number.unwrap_or(style.y);
        }
        Some(GifActorCommand::AddX) => style.x += first_number.unwrap_or(0.0),
        Some(GifActorCommand::AddY) => style.y += first_number.unwrap_or(0.0),
        Some(GifActorCommand::Zoom) => style.zoom = first_number.unwrap_or(style.zoom),
        Some(GifActorCommand::HAlign | GifActorCommand::Align) => {
            style.align_x = alignment.unwrap_or(style.align_x)
        }
        Some(GifActorCommand::CropLeft) => style.crop[0] = first_number.unwrap_or(style.crop[0]),
        Some(GifActorCommand::CropRight) => style.crop[1] = first_number.unwrap_or(style.crop[1]),
        Some(GifActorCommand::CropTop) => style.crop[2] = first_number.unwrap_or(style.crop[2]),
        Some(GifActorCommand::CropBottom) => style.crop[3] = first_number.unwrap_or(style.crop[3]),
        None => {}
    }
    Ok(())
}
