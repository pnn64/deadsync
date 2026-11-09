//! Production preparation LoadActor: caller scope, file results and media instances.
use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn load_actor(call: &mut HostCall<'_>, store: &Shared) -> Result<(), Error> {
    let Value::String(path) = arg(call, 0) else {
        let actor = offline::dummy(call, store, "LoadActor")?;
        return call.push(Value::Table(actor));
    };
    let path = std::str::from_utf8(call.string_bytes(path).expect("live loader path"))
        .map_err(|_| call.error(b"actor path must be UTF-8"))?
        .to_owned();
    let arguments = call.arguments()[1..].to_vec();
    let source = call
        .call(helper(call, 14)?, &[])?
        .first()
        .copied()
        .unwrap_or(Value::Nil);
    let directory = if let Value::String(source) = source {
        let source = std::str::from_utf8(call.string_bytes(source).expect("caller source"))
            .map_err(|_| call.error(b"caller source must be UTF-8"))?
            .trim_start_matches('@');
        if source.starts_with('=') || source == "[C]" {
            None
        } else {
            Path::new(source)
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map(Path::to_path_buf)
        }
    } else {
        None
    };
    let value = with_directory(call, directory.as_deref(), |call| {
        load_path(call, store, &path, &arguments)
    })?;
    call.push(value)
}

pub(super) fn add_child_from_path(
    call: &mut HostCall<'_>,
    store: &Shared,
    parent: TableId,
) -> Result<(), Error> {
    let Some(path) = call.coerce_string(arg(call, 1))? else {
        return call.push(Value::Table(parent));
    };
    let path = std::str::from_utf8(call.string_bytes(path).expect("actor child path"))
        .map_err(|_| call.error(b"actor path must be UTF-8"))?
        .to_owned();
    let directory = assets::field(call, parent, b"__songlua_script_dir")?.map(PathBuf::from);
    let loaded = with_directory(call, directory.as_deref(), |call| {
        load_path(call, store, &path, &[])
    });
    let child = match loaded {
        Ok(Value::Table(child)) => child,
        Err(error) if matches!(error.kind(), ErrorKind::Limit | ErrorKind::Memory) => {
            return Err(error);
        }
        // The existing host ignores missing, malformed and non-actor loads.
        _ => return call.push(Value::Table(parent)),
    };
    call.with_roots(&[Value::Table(child)], |call| {
        if !store.lock().unwrap().actors.contains_key(&child) {
            return call.push(Value::Table(parent));
        }
        let already_present = store.lock().unwrap().actors[&parent]
            .children
            .contains(&child);
        if !already_present {
            parent_scene(call, store, child, Value::Table(parent), true)?;
            initialize(call, store, child)?;
            let walked = sprites::field(call, parent, "__songlua_startup_children_walked")?;
            if store.lock().unwrap().actors[&parent].started
                || !matches!(walked, Value::Nil | Value::Boolean(false))
            {
                lifecycle::startup(call, store, child)?;
            }
            lifecycle::invalidate_plans(call, store)?;
        }
        call.push(Value::Table(parent))
    })
}

fn with_directory(
    call: &mut HostCall<'_>,
    directory: Option<&Path>,
    action: impl FnOnce(&mut HostCall<'_>) -> Result<Value, Error>,
) -> Result<Value, Error> {
    let Some(directory) = directory else {
        return action(call);
    };
    let globals = helper(call, 5)?;
    let key = call.try_intern(b"__songlua_script_dir")?;
    let previous = call.table_get(globals, key)?;
    call.with_roots(&[key, previous], |call| {
        let directory = call.try_intern(directory.to_string_lossy().as_bytes())?;
        call.table_set(globals, key, directory)?;
        let result = action(call);
        let roots: Vec<Value> = match &result {
            Ok(value) => vec![*value],
            Err(error) => error.value().into_iter().collect(),
        };
        call.with_roots(&roots, |call| {
            if result.as_ref().is_err_and(|e| {
                matches!(
                    e.kind(),
                    deadlib_lua::ErrorKind::Limit | deadlib_lua::ErrorKind::Memory
                )
            }) {
                call.raw_table_set(table(call, globals)?, key, previous)?;
            } else {
                call.table_set(globals, key, previous)?;
            }
            result
        })
    })
}

fn load_path(
    call: &mut HostCall<'_>,
    store: &Shared,
    path: &str,
    arguments: &[Value],
) -> Result<Value, Error> {
    if deadlib_assets::generated_texture_shared_key(path).is_some() {
        return media(call, store, "Sprite", path, false);
    }
    let theme = path.trim_start_matches('/');
    if theme.starts_with(crate::SONG_LUA_THEME_PATH_PREFIX) {
        let path_ref = Path::new(theme);
        let class = if crate::is_song_lua_audio_path(path_ref) {
            "Sound"
        } else if crate::is_song_lua_image_path(path_ref) || crate::is_song_lua_video_path(path_ref)
        {
            "Sprite"
        } else {
            "Actor"
        };
        return media(call, store, class, path, true);
    }
    let song_dir = store.lock().unwrap().song_dir.clone();
    let globals = table(call, helper(call, 5)?)?;
    let current = assets::field(call, globals, b"__songlua_script_dir")?
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from);
    let resolved = resolve(&song_dir, current.as_deref(), path)
        .map_err(|error| call.error(error.as_bytes()))?;
    if crate::is_song_lua_media_path(&resolved) {
        let class = if crate::is_song_lua_audio_path(&resolved) {
            "Sound"
        } else {
            "Sprite"
        };
        return media(
            call,
            store,
            class,
            &crate::file_path_string(&resolved),
            false,
        );
    }
    let path = call.try_intern(resolved.to_string_lossy().as_bytes())?;
    let source_loader = native_children::helper_function(call, b"source_loader")?;
    let function = call.call(source_loader, &[path])?[0];
    call.with_roots(&[function], |call| {
        Ok(call
            .call(function, arguments)?
            .first()
            .copied()
            .unwrap_or(Value::Nil))
    })
}

fn media(
    call: &mut HostCall<'_>,
    store: &Shared,
    class: &'static str,
    path: &str,
    theme: bool,
) -> Result<Value, Error> {
    let path = if theme {
        path.to_owned()
    } else {
        let absolute =
            std::path::absolute(path).map_err(|error| call.error(error.to_string().as_bytes()))?;
        crate::file_path_string(&absolute)
    };
    let actor = offline::dummy(call, store, class)?;
    if matches!(class, "Sprite" | "Sound") {
        let value = call.try_intern(path.as_bytes())?;
        sprites::set(
            call,
            actor,
            if class == "Sound" { "File" } else { "Texture" },
            value,
        )?;
    }
    if class == "Sprite" {
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .state
            .render
            .sprite_texture = true;
        if !theme {
            sprites::decode(
                call,
                store,
                actor,
                crate::is_song_lua_video_path(Path::new(&path)),
            )?;
            sprites::movie_size(call, actor)?;
        }
        sprites::refresh_size(call, store, actor)?;
    }
    Ok(Value::Table(actor))
}

// These are the existing production resolver's rules, including default.xml
// passing to the Lua loader. ActorUtil's declaration-only classification remains
// independent; it must not silently add model/BGAnimation behavior here.
fn resolve(song_dir: &Path, current: Option<&Path>, path: &str) -> Result<PathBuf, String> {
    let mut bases = Vec::new();
    if let Some(current) = current {
        bases.push(current.to_path_buf());
    }
    if !bases.iter().any(|base| base == song_dir) {
        bases.push(song_dir.to_path_buf());
    }
    let collapse = crate::files::collapse_actor_path;
    let raw = collapse(Path::new(path));
    let mut resolved = if raw.is_absolute() && raw.exists() {
        raw.clone()
    } else if let Some(candidate) = bases
        .iter()
        .map(|base| collapse(&base.join(path)))
        .find(|candidate| candidate.exists())
    {
        candidate
    } else if raw.exists() {
        raw
    } else {
        wildcard(&bases, song_dir, path)?
    };
    if resolved.is_dir() {
        return directory(&resolved, song_dir, path);
    }
    for _ in 0..100 {
        if !resolved
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("redir"))
        {
            return Ok(resolved);
        }
        let contents = fs::read_to_string(&resolved).map_err(|error| error.to_string())?;
        let target = contents.lines().next().unwrap_or_default().trim();
        if target.is_empty() {
            return Err(format!("empty redirect '{}'", resolved.display()));
        }
        let target = resolved.parent().unwrap_or(song_dir).join(target);
        let target = std::path::absolute(target).map_err(|error| error.to_string())?;
        resolved = wildcard(&bases, song_dir, &target.to_string_lossy())?;
    }
    Err(format!("circular redirect '{path}'"))
}
fn directory(dir: &Path, song_dir: &Path, path: &str) -> Result<PathBuf, String> {
    for candidate in [dir.join("default.lua"), dir.join("default.xml")] {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(missing(song_dir, path))
}
fn missing(song_dir: &Path, path: &str) -> String {
    format!(
        "script '{path}' not found relative to '{}'",
        song_dir.display()
    )
}
fn wildcard(bases: &[PathBuf], song_dir: &Path, path: &str) -> Result<PathBuf, String> {
    for base_dir in bases {
        let base = crate::files::collapse_actor_path(&base_dir.join(path));
        let Some(parent) = base.parent().filter(|parent| parent.is_dir()) else {
            continue;
        };
        let Some(prefix) = base.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let mut matches = fs::read_dir(parent)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|candidate| {
                candidate
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| {
                        name.get(..prefix.len())
                            .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
                    })
            })
            .collect::<Vec<_>>();
        matches.sort_unstable_by_key(|candidate| {
            candidate
                .file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
        });
        if matches.len() > 1 {
            return Err(format!(
                "script '{path}' has multiple matches relative to '{}':\n{}",
                base_dir.display(),
                matches
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
        if let Some(found) = matches.pop() {
            return if found.is_dir() {
                directory(&found, song_dir, path)
            } else {
                Ok(found)
            };
        }
    }
    Err(missing(song_dir, path))
}
