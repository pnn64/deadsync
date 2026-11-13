//! Sound actors emit immediate host events; their RageSound handles retain
//! independent parameters and cached metadata through ordinary traced captures.
use super::*;
use crate::{SongLuaActorSound, SongLuaSoundEvent};
use std::sync::atomic::Ordering;

type LengthCache = Arc<Mutex<Option<(Option<String>, f32)>>>;
fn parameter(call: &HostCall<'_>) -> Value {
    arg(
        call,
        usize::from(matches!(call.arguments().first(), Some(Value::Table(_)))),
    )
}
#[derive(Clone)]
struct SoundBinding {
    name: &'static str,
    length: LengthCache,
}

pub(super) fn install(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let sound = call.try_new_table()?;
    let methods = match get_static(call, actor, b"__songlua_bound_methods")? {
        Value::Nil => call.try_new_table()?,
        value => table(call, value)?,
    };
    let registry = table(call, call.captures()[0])?;
    // The actor's registry value traces the handle even if authored fields are
    // replaced. Its methods in turn retain the actor after scene retirement.
    let scene = call.store().actors[&actor].scene;
    let retired = call.store().retired_scenes.get(&scene).copied();
    if let Some(owner) = retired {
        call.raw_table_set(owner, Value::Table(sound), Value::Boolean(true))?;
    } else {
        call.raw_table_set(registry, Value::Table(actor), Value::Table(sound))?;
    }
    let key = call.try_intern(b"__songlua_bound_methods")?;
    call.raw_table_set(actor, key, Value::Table(methods))?;
    for name in [b"__songlua_volume".as_slice(), b"__songlua_pitch"] {
        let key = call.try_intern(name)?;
        call.raw_table_set(sound, key, Value::Number(1.0))?;
    }
    let captures = [
        call.captures()[0],
        call.captures()[1],
        Value::Table(actor),
        Value::Table(sound),
    ];
    let length = Arc::new(Mutex::new(None));
    for (target, names) in [
        (methods, &["Load", "get", "stop", "playforplayer"][..]),
        (sound, &["pitch", "volume", "get_length", "SetProperty"][..]),
    ] {
        for &name in names {
            let function = call.new_host_function_with_data(
                bound,
                &captures,
                SoundBinding {
                    name,
                    length: length.clone(),
                },
            )?;
            let key = call.try_intern(name.as_bytes())?;
            call.raw_table_set(target, key, function)?;
        }
    }
    let load = call.try_intern(b"Load")?;
    let load = call.raw_table_get(methods, load)?;
    let key = call.try_intern(b"load")?;
    call.raw_table_set(methods, key, load)?;
    // The actor's compatibility volume method is a chain operation; actual
    // RageSound parameters remain on the independently retained get() handle.
    let volume =
        call.new_host_function(|call| call.push(call.captures()[0]), &[Value::Table(actor)])?;
    let key = call.try_intern(b"volume")?;
    call.raw_table_set(methods, key, volume)?;
    call.store().actors.get_mut(&actor).unwrap().sound = Some(sound);
    Ok(())
}

fn path(
    call: &mut HostCall<'_>,
    actor: TableId,
    file: &str,
) -> Result<Option<std::path::PathBuf>, Error> {
    match assets::resolve(call, actor, file) {
        Err(error) if error.kind() == ErrorKind::Runtime => Ok(None),
        result => result,
    }
}

fn owner(call: &mut HostCall<'_>, actor: TableId) -> u64 {
    call.object_pointer(Value::Table(actor))
        .expect("captured actor") as u64
}

pub(super) fn method(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<(), Error> {
    let (is_sound, live) = {
        let store = call.store();
        (
            store.actors[&actor].state.class == "Sound",
            store.clock.live.load(Ordering::Relaxed),
        )
    };
    if !is_sound {
        if !matches!(name, "play" | "pause") {
            return call.push(Value::Table(actor));
        }
        action_capture::prepare_write(call, actor)?;
        call.store()
            .actors
            .get_mut(&actor)
            .unwrap()
            .state
            .render
            .sprite_animate = name == "play";
        sprites::set(
            call,
            actor,
            "__songlua_state_sprite_animate",
            Value::Boolean(name == "play"),
        )?;
        song_media::record(call, actor, "sprite_animate");
        return call.push(Value::Table(actor));
    }
    match name {
        "play" | "playforplayer" => {
            if let Some(file) = assets::field(call, actor, b"File")?
                && let Some(path) = path(call, actor, &file)?
            {
                if live {
                    let owner = owner(call, actor);
                    call.store()
                        .sounds
                        .push(SongLuaActorSound::Play { owner, path });
                } else {
                    let beat = match effects::values(call, false) {
                        Ok(values) => values[0],
                        Err(error) if error.kind() == ErrorKind::Runtime => 0.0,
                        Err(error) => return Err(error),
                    };
                    {
                        let store = call.store();
                        if let Some(queued_startup) = store.sound_capture {
                            let queued_startup = queued_startup || store.startup_discovery;
                            let source_message = store.active_message.clone();
                            store.prepared_sounds.push(SongLuaSoundEvent {
                                beat,
                                path: path.clone(),
                                source_message,
                                queued_startup,
                            });
                        }
                    }
                    let key = call.try_intern(crate::SONG_LUA_SOUND_CALLS_KEY.as_bytes())?;
                    let calls = call.table_get(helper(call, 5)?, key)?;
                    if calls != Value::Nil {
                        let calls = table(call, calls)?;
                        let value = call.try_intern(path.to_string_lossy().as_bytes())?;
                        let index = Value::Number((call.raw_table_len(calls)? + 1) as f64);
                        call.raw_table_set(calls, index, value)?;
                    }
                }
            }
        }
        "pause" | "stop" if live => {
            let owner = owner(call, actor);
            let event = if name == "pause" {
                SongLuaActorSound::Pause {
                    owner,
                    paused: boolish(call, arg(call, 1)).unwrap_or(false),
                }
            } else {
                SongLuaActorSound::Stop { owner }
            };
            call.store().sounds.push(event);
        }
        _ => {}
    }
    call.push(Value::Table(actor))
}

fn bound(call: &mut HostCall<'_>) -> Result<(), Error> {
    let SoundBinding { name, length } = call.function_data::<SoundBinding>().unwrap().clone();
    let actor = table(call, call.captures()[2])?;
    let sound = table(call, call.captures()[3])?;
    match name {
        "volume" | "pitch" => {
            let value = float_scalar(call, parameter(call), false)
                .ok_or_else(|| call.error(b"sound parameter must be a number"))?;
            let key = call.try_intern(if name == "volume" {
                b"__songlua_volume"
            } else {
                b"__songlua_pitch"
            })?;
            call.raw_table_set(sound, key, Value::Number(value as f64))?;
            call.push(Value::Table(sound))
        }
        "SetProperty" => call.push(Value::Boolean(false)),
        "get_length" => {
            // Read authored File before consulting the cache. Neither metadata
            // lookup nor decoder I/O borrows the store across reentrant Lua calls.
            let file = assets::field(call, actor, b"File")?;
            let cached = length
                .lock()
                .unwrap()
                .as_ref()
                .filter(|(previous, _)| previous == &file)
                .map(|(_, value)| *value);
            let value = if let Some(value) = cached {
                value
            } else {
                let path = match file.as_deref() {
                    Some(file) => path(call, actor, file)?,
                    None => None,
                };
                let value = path
                    .and_then(|path| {
                        deadlib_audio_decode::file_length_seconds(&path, Default::default()).ok()
                    })
                    .map(|seconds| (seconds * 1000.0).trunc() / 1000.0)
                    .unwrap_or(-1.0);
                *length.lock().unwrap() = Some((file, value));
                value
            };
            call.push(Value::Number(value as f64))
        }
        _ => {
            if !call.store().actors.contains_key(&actor) {
                return Err(call.error(b"sound actor has been released"));
            }
            match name {
                "get" => call.push(Value::Table(sound)),
                "Load" => {
                    let value = parameter(call);
                    if matches!(value, Value::String(_) | Value::Nil) {
                        if value != Value::Nil {
                            assets::text(call, value)?;
                        }
                        let key = call.try_intern(b"File")?;
                        call.table_set(Value::Table(actor), key, value)?;
                    }
                    call.push(Value::Table(actor))
                }
                _ => method(call, actor, name),
            }
        }
    }
}

pub(super) fn retire(call: &mut HostCall<'_>, actor: TableId) {
    let store = call.store_ref();
    if store.actors[&actor].sound.is_some() && store.clock.live.load(Ordering::Relaxed) {
        let owner = owner(call, actor);
        call.store().sounds.push(SongLuaActorSound::Stop { owner });
    }
}
