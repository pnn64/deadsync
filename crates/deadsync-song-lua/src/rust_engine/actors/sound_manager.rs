//! Existing global SOUND stubs, resource discovery and runtime sound capture.
use super::*;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
enum Operation {
    Effect,
    Path(bool),
    Balance,
    Delayed,
}
#[derive(Clone)]
struct Binding {
    store: Shared,
    song_dir: PathBuf,
    operation: Operation,
}
impl Actors {
    pub(in crate::rust_engine) fn install_sound_manager(
        &self,
        vm: &mut Vm,
        song_dir: &Path,
    ) -> Result<(), Error> {
        let globals = vm.get_global(b"_G")?;
        let tonumber = vm.get_global(b"tonumber")?;
        let sound = vm.try_new_table()?;
        let pin = vm.pin(Value::Table(sound))?;
        let result = (|| {
            let paths = vm.try_new_table()?;
            vm.set_global(
                crate::SONG_LUA_SOUND_PATHS_KEY.as_bytes(),
                Value::Table(paths),
            )?;
            vm.set_global(crate::SONG_LUA_SOUND_CALLS_KEY.as_bytes(), Value::Nil)?;
            for (name, operation) in [
                ("DimMusic", Operation::Effect),
                ("StopMusic", Operation::Effect),
                ("PlayMusicPart", Operation::Path(false)),
                ("PlayOnce", Operation::Path(true)),
                ("PlayAnnouncer", Operation::Effect),
                ("GetPlayerBalance", Operation::Balance),
                ("IsTimingDelayed", Operation::Delayed),
            ] {
                let function = vm.new_host_function_with_data(
                    dispatch,
                    &[globals, tonumber, Value::Table(sound)],
                    Binding {
                        store: self.store.clone(),
                        song_dir: song_dir.to_owned(),
                        operation,
                    },
                )?;
                super::super::put(vm, sound, name, function)?;
            }
            vm.set_global(b"SOUND", Value::Table(sound))
        })();
        vm.unpin(pin)?;
        result
    }
}
fn field(call: &mut HostCall<'_>, target: Value, name: &str) -> Result<Value, Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.table_get(target, key)
}
fn text(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"sound string expected"))?;
    std::str::from_utf8(call.string_bytes(id).unwrap())
        .map(str::to_owned)
        .map_err(|e| call.error(e.to_string().as_bytes()))
}
fn dispatch(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Binding>().unwrap().clone();
    match data.operation {
        Operation::Balance => return call.push(Value::Number(0.)),
        Operation::Delayed => return call.push(Value::Boolean(false)),
        _ => {}
    }
    super::super::managers::side_effect(call, call.captures()[0], call.captures()[1])?;
    if let Operation::Path(capture) = data.operation {
        let offset = usize::from(matches!(call.arguments().first(), Some(Value::Table(_))));
        let value = arg(call, offset);
        // The native read_string helper ignores non-string inputs
        // and invalid UTF-8; path-resolution runtime errors are also ignored.
        if matches!(value, Value::String(_)) {
            let raw = text(call, value);
            if let Ok(raw) = raw {
                match resolve(call, &data.song_dir, &raw) {
                    Ok(Some(path)) => record(call, &data.store, path, capture)?,
                    Ok(None) => {}
                    Err(error) if error.kind() == ErrorKind::Runtime => {}
                    Err(error) => return Err(error),
                }
            } else if let Err(error) = raw
                && error.kind() != ErrorKind::Runtime
            {
                return Err(error);
            }
        }
    }
    call.push(call.captures()[2])
}
fn resolve(call: &mut HostCall<'_>, song_dir: &Path, raw: &str) -> Result<Option<PathBuf>, Error> {
    let path = Path::new(raw);
    let resolved = if path.is_absolute() && path.exists() {
        Some(path.to_owned())
    } else {
        let current = field(call, call.captures()[0], "__songlua_script_dir")?;
        let current = if current == Value::Nil {
            None
        } else {
            Some(text(call, current)?)
        };
        current
            .filter(|dir| !dir.trim().is_empty())
            .map(|dir| Path::new(&dir).join(raw))
            .filter(|path| path.exists())
            .or_else(|| {
                let candidate = song_dir.join(raw);
                candidate.exists().then_some(candidate)
            })
            .or_else(|| path.exists().then(|| path.to_owned()))
    };
    Ok(resolved.filter(|path| path.is_file() && crate::is_song_lua_audio_path(path)))
}
fn beat(call: &mut HostCall<'_>) -> Result<f32, Error> {
    let runtime = field(call, call.captures()[0], crate::SONG_LUA_RUNTIME_KEY)?;
    call.with_roots(&[runtime], |call| {
        let mut result = 0.;
        for (i, key) in [
            crate::SONG_LUA_RUNTIME_BEAT_KEY,
            crate::SONG_LUA_RUNTIME_SECONDS_KEY,
        ]
        .into_iter()
        .enumerate()
        {
            let value = field(call, runtime, key)?;
            let value = call.call(call.captures()[1], &[value])?[0];
            let Value::Number(value) = value else {
                return Err(call.error(b"sound clock number expected"));
            };
            if i == 0 {
                result = value as f32;
            }
        }
        Ok(result)
    })
}
fn record(
    call: &mut HostCall<'_>,
    store: &Shared,
    path: PathBuf,
    capture: bool,
) -> Result<(), Error> {
    let globals = call.captures()[0];
    if capture {
        let beat = match beat(call) {
            Ok(beat) => beat,
            Err(e) if e.kind() == ErrorKind::Runtime => 0.,
            Err(e) => return Err(e),
        };
        {
            let mut store = store.lock().unwrap();
            if let Some(queued_startup) = store.sound_capture {
                let queued_startup = queued_startup || store.startup_discovery;
                let source_message = store.active_message.clone();
                store.prepared_sounds.push(crate::SongLuaSoundEvent {
                    beat,
                    path: path.clone(),
                    source_message,
                    queued_startup,
                });
            }
        }
        let calls = field(call, globals, crate::SONG_LUA_SOUND_CALLS_KEY)?;
        if calls != Value::Nil {
            call.with_roots(&[calls], |call| {
                let calls = table(call, calls)?;
                let key = call.try_intern(path.to_string_lossy().as_bytes())?;
                call.raw_table_set(
                    calls,
                    Value::Number((call.raw_table_len(calls)? + 1) as f64),
                    key,
                )
            })?;
        }
    }
    let paths = field(call, globals, crate::SONG_LUA_SOUND_PATHS_KEY)?;
    let paths = if paths == Value::Nil {
        let paths = Value::Table(call.try_new_table()?);
        call.with_roots(&[paths], |call| {
            let key = call.try_intern(crate::SONG_LUA_SOUND_PATHS_KEY.as_bytes())?;
            call.table_set(globals, key, paths)
        })?;
        paths
    } else {
        paths
    };
    call.with_roots(&[paths], |call| {
        let paths = table(call, paths)?;
        let key = path.to_string_lossy();
        let mut i = 1;
        loop {
            let value = call.raw_table_get(paths, Value::Number(i as f64))?;
            if value == Value::Nil {
                break;
            }
            if text(call, value)? == key {
                return Ok(());
            }
            i += 1;
        }
        let key = call.try_intern(key.as_bytes())?;
        call.raw_table_set(
            paths,
            Value::Number((call.raw_table_len(paths)? + 1) as f64),
            key,
        )
    })
}
