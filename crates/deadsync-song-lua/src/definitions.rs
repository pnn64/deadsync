//! Actor declaration services. Materialization and live methods are separate.
use deadlib_lua::{Error, HostCall, Root, Value, Vm, compile_named};
use std::{
    fs,
    path::{Path, PathBuf},
};

// REGISTER_ACTOR_CLASS / REGISTER_ACTOR_CLASS_WITH_NAME at the pinned ITGmania
// revision. The source inventory and hashes are retained with the tests.
const CLASSES: &[&str] = &[
    "Actor",
    "ActorFrame",
    "ActorFrameTexture",
    "ActorMultiTexture",
    "ActorMultiVertex",
    "ActorProxy",
    "ActorScroller",
    "Sound",
    "Banner",
    "BGAnimation",
    "BitmapText",
    "BPMDisplay",
    "SongBPMDisplay",
    "ComboGraph",
    "ControllerStateDisplay",
    "CourseContentsList",
    "DifficultyIcon",
    "StepsDisplayList",
    "DynamicActorScroller",
    "FadingBanner",
    "GradeDisplay",
    "GraphDisplay",
    "GraphLine",
    "GrooveRadar",
    "HelpDisplay",
    "HoldJudgment",
    "MemoryCardDisplay",
    "MeterDisplay",
    "SongMeterDisplay",
    "Model",
    "ModIconRow",
    "PaneDisplay",
    "PercentageDisplay",
    "Quad",
    "RollingNumbers",
    "ScoreDisplayAliveTime",
    "ScoreDisplayCalories",
    "DeviceList",
    "InputList",
    "Sprite",
    "StepsDisplay",
    "TextBanner",
    "WorkoutGraph",
];

pub(super) struct Loader {
    pub(super) entry: Root,
    target: deadlib_lua::TableId,
}
impl Loader {
    pub(super) fn set_function(&self, vm: &mut Vm, function: Value) -> Result<(), Error> {
        vm.raw_table_set(self.target, Value::Number(1.), function)
    }

    pub(super) fn install_live(&self, vm: &mut Vm) -> Result<(), Error> {
        let declaration = vm.raw_table_get(self.target, Value::Number(1.))?;
        let caller = vm.raw_table_get(self.target, Value::Number(2.))?;
        let globals = vm.get_global(b"_G")?;
        let loader = vm.new_host_function(load_live_actor, &[declaration, caller, globals])?;
        vm.set_global(b"LoadActor", loader)
    }
}

fn load_live_actor(call: &mut HostCall<'_>) -> Result<(), Error> {
    let original = call.captures()[0];
    let mut arguments = call.arguments().to_vec();
    // Resolve the authored caller before entering the Lua declaration adapter.
    // This native boundary preserves a tail-calling helper's file identity.
    if let Some(Value::String(id)) = arguments.first().copied() {
        let path = std::str::from_utf8(call.string_bytes(id).unwrap())
            .map_err(|_| call.error(b"actor path must be UTF-8"))?
            .to_owned();
        if !path.is_empty()
            && theme_actor_class(&path).is_none()
            && deadlib_assets::generated_texture_shared_key(&path).is_none()
            && Path::new(&path).is_relative()
        {
            let caller = call.call(call.captures()[1], &[])?[0];
            let directory = if let Value::String(id) = caller {
                let source = std::str::from_utf8(call.string_bytes(id).unwrap())
                    .map_err(|_| call.error(b"caller source must be UTF-8"))?;
                let source_path = Path::new(source.trim_start_matches('@'));
                (source.starts_with('@') || source_path.is_absolute() || source_path.is_file())
                    .then(|| source_path.parent().map(Path::to_path_buf))
                    .flatten()
            } else {
                None
            };
            let directory = match directory {
                Some(directory) => Some(directory),
                None => {
                    let key = call.try_intern(b"__songlua_script_dir")?;
                    match call.table_get(call.captures()[2], key)? {
                        Value::String(id) => Some(PathBuf::from(
                            std::str::from_utf8(call.string_bytes(id).unwrap())
                                .map_err(|_| call.error(b"script directory must be UTF-8"))?,
                        )),
                        _ => None,
                    }
                }
            };
            if let Some(directory) = directory {
                let absolute = std::path::absolute(directory.join(&path))
                    .map_err(|error| call.error(error.to_string().as_bytes()))?;
                let absolute = crate::files::collapse_actor_path(&absolute);
                arguments[0] = call.try_intern(crate::file_path_string(&absolute).as_bytes())?;
            }
        }
    }
    // Keep the existing declaration resolver, media classification, nil
    // arguments, secondary results and authored ActorUtil overrides.
    call.call_and_return(original, &arguments)
}

pub(super) fn install(vm: &mut Vm) -> Result<Loader, Error> {
    let utility = vm.try_new_table()?;
    let root = vm.pin(Value::Table(utility))?;
    let result = (|| {
        let globals = vm.get_global(b"_G")?;
        for (name, callback) in [
            ("IsRegisteredClass", registered as deadlib_lua::HostFunction),
            ("ResolvePath", resolve),
            ("GetFileType", file_type),
            ("LoadAllCommands", command_side_effect),
            ("LoadAllCommandsFromName", command_side_effect),
            ("LoadAllCommandsAndSetXY", command_side_effect),
        ] {
            let captures = [globals];
            let captures = if name == "ResolvePath" || name.starts_with("LoadAllCommands") {
                &captures[..]
            } else {
                &[]
            };
            let function = vm.new_host_function(callback, captures)?;
            super::put(vm, utility, name, function)?;
        }
        vm.set_global(b"ActorUtil", Value::Table(utility))?;
        let globals = vm.get_global(b"_G")?;
        for (name, warning) in [("Trace", false), ("Warn", true)] {
            let function = vm.new_host_function_with_data(message, &[globals], warning)?;
            vm.set_global(name.as_bytes(), function)?;
        }
        let theme = vm.new_host_function(theme_path_actor_type, &[])?;
        vm.set_global(b"__songlua_theme_path_actor_type", theme)?;
        let program = compile_named(
            include_bytes!("actor_def.lua"),
            // Embedded adapter code has no asset directory. File wrappers add
            // a native boundary, so a tail-called LoadActor can remain visible
            // while Def walks frames. Never let that adapter frame supply the
            // declaration's source directory instead of its authored caller.
            b"=DeadSync actor declaration bindings",
        )?;
        let executed = vm.execute(&program).map(|_| ());
        vm.set_global(b"__songlua_theme_path_actor_type", Value::Nil)?;
        executed?;
        let load_actor = vm.get_global(b"LoadActor")?;
        let target = vm.try_new_table()?;
        let keeper = vm.pin(Value::Table(target))?;
        let result = (|| {
            vm.raw_table_set(target, Value::Number(1.), load_actor)?;
            // Keep the VM primitive before the compiler publishes its reporting
            // debug table or authored files replace public debug functions.
            let code = compile_named(
                b"local getinfo=debug.getinfo; return function() local info=getinfo(3,'S'); return info and info.source end",
                b"=live actor loader caller",
            )?;
            let caller = vm.execute(&code)?[0];
            vm.raw_table_set(target, Value::Number(2.), caller)?;
            let entry = vm.new_host_function(load_entry, &[Value::Table(target)])?;
            Ok(Loader {
                entry: vm.pin(entry)?,
                target,
            })
        })();
        vm.unpin(keeper)?;
        result
    })();
    vm.unpin(root)?;
    result
}

fn load_entry(call: &mut HostCall<'_>) -> Result<(), Error> {
    let path = string_argument(call, 0)?;
    let path = if theme_actor_class(&path).is_some()
        || deadlib_assets::generated_texture_shared_key(&path).is_some()
    {
        path
    } else {
        let absolute =
            std::path::absolute(path).map_err(|error| call.error(error.to_string().as_bytes()))?;
        crate::file_path_string(&absolute)
    };
    let path = call.try_intern(path.as_bytes())?;
    let mut arguments = call.arguments().to_vec();
    arguments[0] = path;
    let Value::Table(target) = call.captures()[0] else {
        unreachable!("actor loader target")
    };
    let function = call.raw_table_get(target, Value::Number(1.))?;
    call.call_and_return(function, &arguments)
}

// Match create_theme_path_actor: theme resources are virtual and preserve their
// authored spelling. Non-media paths produce a plain Actor, including .lua/.txt.
fn theme_actor_class(path: &str) -> Option<&'static str> {
    let path = path.trim_start_matches('/');
    if !path.starts_with(crate::SONG_LUA_THEME_PATH_PREFIX) {
        return None;
    }
    let path = Path::new(path);
    Some(if crate::is_song_lua_audio_path(path) {
        "Sound"
    } else if crate::is_song_lua_image_path(path) || crate::is_song_lua_video_path(path) {
        "Sprite"
    } else {
        "Actor"
    })
}

fn theme_path_actor_type(call: &mut HostCall<'_>) -> Result<(), Error> {
    let path = string_argument(call, 0)?;
    let value = match theme_actor_class(&path) {
        Some(class) => call.try_intern(class.as_bytes())?,
        None => Value::Nil,
    };
    call.push(value)
}

fn string_argument(call: &mut HostCall<'_>, index: usize) -> Result<String, Error> {
    let value = call.arguments().get(index).copied().unwrap_or(Value::Nil);
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"string expected"))?;
    std::str::from_utf8(call.string_bytes(id).expect("live string"))
        .map(str::to_owned)
        .map_err(|error| call.error(error.to_string().as_bytes()))
}

fn registered(call: &mut HostCall<'_>) -> Result<(), Error> {
    let class = string_argument(call, 0)?;
    call.push(Value::Boolean(CLASSES.contains(&class.as_str())))
}

fn message(call: &mut HostCall<'_>) -> Result<(), Error> {
    if *call.function_data::<bool>().expect("warning binding") {
        super::managers::note_side_effect(call, call.captures()[0])
    } else {
        string_argument(call, 0).map(|_| ())
    }
}

fn resolve(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mut path = PathBuf::from(string_argument(call, 0)?);
    let _level = call.number(1)?;
    let optional = !matches!(
        call.arguments().get(2),
        None | Some(Value::Nil | Value::Boolean(false))
    );
    if path.is_relative() {
        let key = call.try_intern(b"__songlua_script_dir")?;
        let directory = call.table_get(call.captures()[0], key)?;
        if directory != Value::Nil {
            let directory = call
                .coerce_string(directory)?
                .ok_or_else(|| call.error(b"script directory must be a string"))?;
            let directory = std::str::from_utf8(call.string_bytes(directory).unwrap())
                .map_err(|error| call.error(error.to_string().as_bytes()))?;
            path = Path::new(directory).join(path);
        }
    }
    let resolved = resolve_path(&path, optional).map_err(|error| call.error(error.as_bytes()))?;
    if let Some(path) = resolved {
        let value = call.try_intern(crate::file_path_string(&path).as_bytes())?;
        call.push(value)?;
    }
    Ok(())
}

fn command_side_effect(call: &mut HostCall<'_>) -> Result<(), Error> {
    super::managers::note_side_effect(call, call.captures()[0])
}

fn resolve_path(path: &Path, optional: bool) -> Result<Option<PathBuf>, String> {
    let mut resolved = crate::files::collapse_actor_path(path);
    let exact_directory = resolved.is_dir();
    if !resolved.exists() {
        let Some(found) = autowildcard(&resolved)? else {
            return if optional {
                Ok(None)
            } else {
                Err(format!("actor '{}' not found", path.display()))
            };
        };
        resolved = found;
    }
    // ActorUtil prefers default.lua, otherwise a directory remains a
    // BGAnimation declaration. Do not reinterpret default.xml as a Lua file.
    if exact_directory {
        let default = resolved.join("default.lua");
        if default.is_file() {
            return Ok(Some(default));
        }
    }
    for _ in 0..100 {
        if !resolved.extension().is_some_and(|ext| ext == "redir") {
            return Ok(Some(resolved));
        }
        let contents = fs::read_to_string(&resolved).map_err(|error| error.to_string())?;
        let target = contents.lines().next().unwrap_or_default();
        if target.is_empty() {
            return Ok(Some(PathBuf::new()));
        }
        let target = resolved.parent().unwrap_or(Path::new(".")).join(target);
        let target = crate::files::collapse_actor_path(&target);
        // DerefRedir always autowildcards, even when its target exists exactly.
        resolved = autowildcard(&target)?
            .ok_or_else(|| format!("redirect target '{}' not found", target.display()))?;
    }
    Err(format!("circular redirect '{}'", path.display()))
}

fn autowildcard(path: &Path) -> Result<Option<PathBuf>, String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Ok(None);
    }
    let Some(prefix) = path.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };
    let mut matches = fs::read_dir(parent)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.get(..prefix.len())
                        .is_some_and(|name| name.eq_ignore_ascii_case(prefix))
                })
        })
        .collect::<Vec<_>>();
    matches.sort_unstable_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
    });
    if matches.len() > 1 {
        return Err(format!(
            "actor '{}' has multiple matches:\n{}",
            path.display(),
            matches
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    Ok(matches
        .pop()
        .map(|path| crate::files::collapse_actor_path(&path)))
}

fn file_type(call: &mut HostCall<'_>) -> Result<(), Error> {
    let spelling = string_argument(call, 0)?;
    let path = Path::new(&spelling);
    // Native ActorUtil::InitFileTypeLists/GetFileType, including extension
    // precedence over directory detection. These classify declarations only;
    // they do not claim that the associated assets can already be rendered.
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let kind = match extension.as_str() {
        "lua" => "FileType_Lua",
        "xml" => "FileType_Xml",
        "ini" => "FileType_Ini",
        "bmp" | "gif" | "jpeg" | "jpg" | "png" => "FileType_Bitmap",
        "mp3" | "oga" | "ogg" | "wav" => "FileType_Sound",
        "avi" | "f4v" | "flv" | "mkv" | "mp4" | "mpeg" | "mpg" | "mov" | "ogv" | "webm" | "wmv" => {
            "FileType_Movie"
        }
        "sprite" => "FileType_Sprite",
        "txt" => "FileType_Model",
        // Retain the existing DeadSync media formats beyond ITGmania's list.
        _ if crate::is_song_lua_image_path(path) => "FileType_Bitmap",
        _ if crate::is_song_lua_audio_path(path) => "FileType_Sound",
        _ if crate::is_song_lua_simfile_path(path) => "FileType_Text",
        _ if spelling.ends_with('/') || path.is_dir() => "FileType_Directory",
        _ => "FileType_Invalid",
    };
    let value = call.try_intern(kind.as_bytes())?;
    call.push(value)
}
