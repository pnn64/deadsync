use mlua::{Lua, MultiValue, Table, Value};
use std::fs;
use std::path::{Path, PathBuf};

use super::runtime::note_song_lua_side_effect;
use super::util::{file_path_string, method_arg, read_boolish, read_string};

pub(super) fn create_fileman_table(lua: &Lua, song_dir: &Path) -> mlua::Result<Table> {
    let fileman = lua.create_table()?;
    let song_dir = song_dir.to_path_buf();
    let listing_song_dir = song_dir.clone();
    fileman.set(
        "GetDirListing",
        lua.create_function(move |lua, args: MultiValue| {
            fileman_dir_listing_table(lua, &listing_song_dir, &args).map(Value::Table)
        })?,
    )?;
    let file_song_dir = song_dir.clone();
    fileman.set(
        "DoesFileExist",
        lua.create_function(move |_, args: MultiValue| {
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(false);
            };
            Ok(resolve_compat_path(&file_song_dir, raw_path.as_str()).exists())
        })?,
    )?;
    let size_song_dir = song_dir.clone();
    fileman.set(
        "GetFileSizeBytes",
        lua.create_function(move |_, args: MultiValue| {
            let Some(raw_path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(0_i64);
            };
            let size = resolve_compat_path(&size_song_dir, raw_path.as_str())
                .metadata()
                .ok()
                .filter(|metadata| metadata.is_file())
                .map(|metadata| metadata.len().min(i64::MAX as u64) as i64)
                .unwrap_or(0);
            Ok(size)
        })?,
    )?;
    fileman.set(
        "GetHashForFile",
        lua.create_function(|_, _args: MultiValue| Ok(0_i64))?,
    )?;
    fileman.set(
        "Copy",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(false)
        })?,
    )?;
    for (name, value) in [("CreateDir", true), ("Remove", true), ("Unzip", false)] {
        fileman.set(
            name,
            lua.create_function(move |lua, _args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                Ok(value)
            })?,
        )?;
    }
    fileman.set(
        "FlushDirCache",
        lua.create_function(|lua, _args: MultiValue| {
            note_song_lua_side_effect(lua)?;
            Ok(())
        })?,
    )?;
    Ok(fileman)
}

pub(super) fn resolve_compat_path(song_dir: &Path, raw_path: &str) -> PathBuf {
    let path = Path::new(raw_path.trim());
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        song_dir.join(path)
    }
}

fn fileman_dir_listing_table(lua: &Lua, song_dir: &Path, args: &MultiValue) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let Some(raw_path) = method_arg(args, 0).cloned().and_then(read_string) else {
        return Ok(table);
    };
    let only_dirs = method_arg(args, 1)
        .cloned()
        .and_then(read_boolish)
        .unwrap_or(false);
    let return_path_too = method_arg(args, 2)
        .cloned()
        .and_then(read_boolish)
        .unwrap_or(false);

    let path = resolve_compat_path(song_dir, raw_path.as_str());
    let mut entries = Vec::new();
    if path.is_dir() {
        entries = fileman_read_dir(&path, None, only_dirs, return_path_too)?;
    } else if let Some(pattern) = path.file_name().and_then(|name| name.to_str())
        && pattern.bytes().any(|byte| matches!(byte, b'*' | b'?'))
        && let Some(parent) = path.parent()
    {
        entries = fileman_read_dir(parent, Some(pattern), only_dirs, return_path_too)?;
    } else if path.exists() && (!only_dirs || path.is_dir()) {
        entries.push(fileman_entry_name(&path, return_path_too));
    }

    entries.sort_unstable();
    for (idx, entry) in entries.into_iter().enumerate() {
        table.raw_set(idx + 1, entry)?;
    }
    Ok(table)
}

fn fileman_read_dir(
    path: &Path,
    pattern: Option<&str>,
    only_dirs: bool,
    return_path_too: bool,
) -> mlua::Result<Vec<String>> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)
        .map_err(mlua::Error::external)?
        .filter_map(Result::ok)
    {
        let entry_path = entry.path();
        if only_dirs && !entry_path.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().into_string().ok() else {
            continue;
        };
        if pattern.is_some_and(|pattern| !wildcard_matches(pattern, &name)) {
            continue;
        }
        entries.push(if return_path_too {
            file_path_string(entry_path.as_path())
        } else {
            name
        });
    }
    Ok(entries)
}

fn fileman_entry_name(path: &Path, return_path_too: bool) -> String {
    if return_path_too {
        return file_path_string(path);
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let pattern = pattern.as_bytes();
    let text = text.as_bytes();
    let (mut pattern_idx, mut text_idx) = (0, 0);
    let (mut star_idx, mut star_text_idx) = (None, 0);
    while text_idx < text.len() {
        if pattern_idx < pattern.len()
            && (pattern[pattern_idx] == b'?' || pattern[pattern_idx] == text[text_idx])
        {
            pattern_idx += 1;
            text_idx += 1;
        } else if pattern_idx < pattern.len() && pattern[pattern_idx] == b'*' {
            star_idx = Some(pattern_idx);
            star_text_idx = text_idx;
            pattern_idx += 1;
        } else if let Some(star) = star_idx {
            pattern_idx = star + 1;
            star_text_idx += 1;
            text_idx = star_text_idx;
        } else {
            return false;
        }
    }
    pattern[pattern_idx..].iter().all(|byte| *byte == b'*')
}
