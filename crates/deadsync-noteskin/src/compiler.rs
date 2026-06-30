pub use crate::compiled::{CompiledActors, CompiledLoader, actor_manifest_key};
use crate::{
    actor as noteskin_actor, compiled as noteskin_compiled,
    compiled::{CompiledActorFile, CompiledLoaderEntry, CompiledNoteskinBundle},
    itg as noteskin_itg,
};
use log::{info, warn};
use mlua::{Function, Lua, MultiValue, Table, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use twox_hash::XxHash64;

const COMPILER_VERSION: u32 = 8;
static COMPILED_HASH_CACHE: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
const DANCE_BUTTONS: [&str; 6] = ["UpLeft", "UpRight", "Left", "Down", "Up", "Right"];
const CORE_ELEMENTS: [&str; 33] = [
    "Explosion",
    "Go Receptor",
    "HitMine Explosion",
    "Hold Body Active",
    "Hold Body Inactive",
    "Hold BottomCap Active",
    "Hold BottomCap Inactive",
    "Hold Explosion",
    "Hold Head Active",
    "Hold Head Inactive",
    "Hold Tail Active",
    "Hold Tail Inactive",
    "Hold TopCap Active",
    "Hold TopCap Inactive",
    "Ready Receptor",
    "Receptor",
    "Roll Body Active",
    "Roll Body Inactive",
    "Roll BottomCap Active",
    "Roll BottomCap Inactive",
    "Roll Explosion",
    "Roll Head Active",
    "Roll Head Inactive",
    "Roll Tail Active",
    "Roll Tail Inactive",
    "Roll TopCap Active",
    "Roll TopCap Inactive",
    "Tap Explosion Bright",
    "Tap Explosion Dim",
    "Tap Fake",
    "Tap Lift",
    "Tap Mine",
    "Tap Note",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileOutcome {
    Reused,
    Built,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompileAllItgSummary {
    pub total: usize,
    pub built: usize,
    pub reused: usize,
    pub failed: usize,
}

pub fn ensure_compiled(
    cache_dir: &Path,
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Result<CompileOutcome, String> {
    if let Some(path) = cached_bundle_path(cache_dir, game, &data.name)
        && noteskin_compiled::load_compiled_bundle(&path).is_some()
    {
        return Ok(CompileOutcome::Reused);
    }
    let source_hash = source_hash(game, data)?;
    remember_source_hash(game, &data.name, &source_hash);
    let path = compiled_bundle_path(cache_dir, game, &data.name, &source_hash);
    if noteskin_compiled::load_compiled_bundle(&path).is_some() {
        return Ok(CompileOutcome::Reused);
    }
    info!("compiling noteskin cache for '{game}/{}'", data.name);
    let bundle = compile_data(game, data, &source_hash)?;
    noteskin_compiled::save_compiled_bundle(&path, &bundle)?;
    Ok(CompileOutcome::Built)
}

pub fn load_compiled(
    cache_dir: &Path,
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Option<CompiledNoteskinBundle> {
    let path = cached_bundle_path(cache_dir, game, &data.name)?;
    noteskin_compiled::load_compiled_bundle(&path)
}

pub fn load_or_compile(
    cache_dir: &Path,
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Result<CompiledNoteskinBundle, String> {
    if let Some(bundle) = load_compiled(cache_dir, game, data) {
        return Ok(bundle);
    }
    ensure_compiled(cache_dir, game, data).map_err(|err| {
        format!(
            "failed to compile noteskin cache for '{game}/{}': {err}",
            data.name
        )
    })?;
    load_compiled(cache_dir, game, data).ok_or_else(|| {
        format!(
            "compiled noteskin cache missing for '{game}/{}' after successful compilation",
            data.name
        )
    })
}

pub fn compile_all_itg_caches_with_progress<F>(
    cache_dir: &Path,
    roots: &[PathBuf],
    game: &str,
    mut on_progress: F,
) -> CompileAllItgSummary
where
    F: FnMut(usize, usize, &str, &str),
{
    let skins = noteskin_itg::discover_skins(roots, game);
    let total = skins.len();
    let mut summary = CompileAllItgSummary {
        total,
        ..CompileAllItgSummary::default()
    };

    for (idx, skin) in skins.iter().enumerate() {
        let label = format!("{game}/{skin}");
        let result = load_data_from_roots(roots, game, skin).and_then(|data| {
            ensure_compiled(cache_dir, game, &data).map(|outcome| (data, outcome))
        });
        match result {
            Ok((_data, CompileOutcome::Built)) => {
                summary.built += 1;
                on_progress(idx + 1, total, &label, "compiled");
            }
            Ok((_data, CompileOutcome::Reused)) => {
                summary.reused += 1;
                on_progress(idx + 1, total, &label, "");
            }
            Err(err) => {
                summary.failed += 1;
                warn!("noteskin cache compile failed for '{}': {}", label, err);
                on_progress(idx + 1, total, &label, "failed");
            }
        }
    }

    summary
}

fn load_data_from_roots(
    roots: &[PathBuf],
    game: &str,
    skin: &str,
) -> Result<noteskin_itg::NoteskinData, String> {
    let mut last_load_err = None;
    for root in roots {
        match noteskin_itg::load_noteskin_data(root, game, skin) {
            Ok(data) => return Ok(data),
            Err(err) => last_load_err = Some(err),
        }
    }
    Err(last_load_err.unwrap_or_else(|| format!("noteskin '{game}/{skin}' not found in any root")))
}

fn cached_bundle_path(cache_dir: &Path, game: &str, skin: &str) -> Option<PathBuf> {
    let key = compiled_hash_cache_key(game, skin);
    let hash = COMPILED_HASH_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned()?;
    Some(compiled_bundle_path(cache_dir, game, skin, &hash))
}

fn compiled_bundle_path(cache_dir: &Path, game: &str, skin: &str, source_hash: &str) -> PathBuf {
    noteskin_compiled::compiled_bundle_path(cache_dir, game, skin, source_hash)
}

fn remember_source_hash(game: &str, skin: &str, source_hash: &str) {
    let key = compiled_hash_cache_key(game, skin);
    COMPILED_HASH_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, source_hash.to_string());
}

fn compiled_hash_cache_key(game: &str, skin: &str) -> String {
    format!(
        "{}/{}",
        game.trim().to_ascii_lowercase(),
        skin.trim().to_ascii_lowercase()
    )
}

fn source_hash(game: &str, data: &noteskin_itg::NoteskinData) -> Result<String, String> {
    let mut paths = source_paths(data);
    paths.sort_by_key(|left| source_label(data, left));
    let mut hasher = XxHash64::default();
    hasher.write_u32(noteskin_compiled::CACHE_SCHEMA_VERSION);
    hasher.write_u32(COMPILER_VERSION);
    hasher.write(game.as_bytes());
    hasher.write(data.name.as_bytes());
    for path in paths {
        let label = source_label(data, &path);
        hasher.write(label.as_bytes());
        let bytes = fs::read(&path)
            .map_err(|err| format!("failed to read '{}' for hashing: {err}", path.display()))?;
        hasher.write(&bytes);
    }
    Ok(format!("{:016x}", hasher.finish()))
}

fn source_paths(data: &noteskin_itg::NoteskinData) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in &data.search_dirs {
        for name in ["metrics.ini", "NoteSkin.lua"] {
            let path = dir.join(name);
            if path.is_file() {
                out.push(path);
            }
        }
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_actor_lua = path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
                && path.file_name().is_none_or(|name| name != "NoteSkin.lua");
            if is_actor_lua {
                out.push(path);
            }
        }
    }
    out
}

fn source_label(data: &noteskin_itg::NoteskinData, path: &Path) -> String {
    for dir in &data.search_dirs {
        if !path.starts_with(dir) {
            continue;
        }
        let game = dir
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let skin = dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let rel = path
            .strip_prefix(dir)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"));
        return format!(
            "{}/{}/{}",
            game.to_ascii_lowercase(),
            skin.to_ascii_lowercase(),
            rel.to_ascii_lowercase()
        );
    }
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn compile_data(
    game: &str,
    data: &noteskin_itg::NoteskinData,
    source_hash: &str,
) -> Result<CompiledNoteskinBundle, String> {
    let scripts = noteskin_paths(data);
    let lua = Lua::new();
    install_host(&lua).map_err(|err| err.to_string())?;
    let noteskin = load_noteskin_table(&lua, &scripts)?;
    Ok(CompiledNoteskinBundle {
        version: noteskin_compiled::CACHE_SCHEMA_VERSION,
        game: game.to_string(),
        skin: data.name.clone(),
        source_hash: source_hash.to_string(),
        loader: CompiledLoader {
            version: COMPILER_VERSION,
            game: game.to_string(),
            skin: data.name.clone(),
            entries: compile_entries(&lua, &noteskin, data)?,
        },
        actors: CompiledActors {
            version: COMPILER_VERSION,
            files: compile_actor_files(data)?,
        },
    })
}

fn noteskin_paths(data: &noteskin_itg::NoteskinData) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in data.search_dirs.iter().rev() {
        let path = dir.join("NoteSkin.lua");
        if path.is_file() {
            out.push(path);
        }
    }
    out
}

fn compile_actor_files(
    data: &noteskin_itg::NoteskinData,
) -> Result<Vec<CompiledActorFile>, String> {
    let mut out = Vec::new();
    for dir in &data.search_dirs {
        let entries = fs::read_dir(dir)
            .map_err(|err| format!("failed to read '{}': {err}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let is_lua = path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
                && path.file_name().is_none_or(|name| name != "NoteSkin.lua");
            if !is_lua {
                continue;
            }
            let content = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read '{}': {err}", path.display()))?;
            let Some(key) = noteskin_compiled::actor_manifest_key_for_dir(dir, &path) else {
                continue;
            };
            out.push(CompiledActorFile {
                key,
                decl: noteskin_actor::parse_actor_decl(&content, &data.metrics),
            });
        }
    }
    out.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(out)
}

fn install_host(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let actor_mt = lua.create_table()?;
    let actor_methods = lua.create_table()?;
    for name in [
        "x",
        "y",
        "z",
        "addx",
        "addy",
        "addz",
        "rotationx",
        "rotationy",
        "rotationz",
        "addrotationx",
        "addrotationy",
        "addrotationz",
        "zoom",
        "zoomx",
        "zoomy",
        "zoomz",
        "diffuse",
        "diffusealpha",
        "glow",
        "vertalign",
        "valign",
        "blend",
        "visible",
        "SetTextureFiltering",
    ] {
        let command = name.to_string();
        actor_methods.set(
            name,
            lua.create_function(move |lua, (actor, args): (Table, MultiValue)| {
                append_actor_command(lua, &actor, &command, args)?;
                Ok(actor)
            })?,
        )?;
    }
    actor_mt.set("__index", actor_methods)?;
    actor_mt.set(
        "__concat",
        lua.create_function(|_, (lhs, _rhs): (Table, Value)| Ok(lhs))?,
    )?;
    let make_actor = {
        let actor_mt = actor_mt.clone();
        lua.create_function(
            move |lua, (blank, button, element): (bool, Option<String>, Option<String>)| {
                let actor = lua.create_table()?;
                actor.set("__blank", blank)?;
                if let Some(button) = button {
                    actor.set("__load_button", button)?;
                }
                if let Some(element) = element {
                    actor.set("__load_element", element)?;
                }
                let _ = actor.set_metatable(Some(actor_mt.clone()));
                Ok(actor)
            },
        )?
    };
    let load_actor = {
        let make_actor = make_actor.clone();
        lua.create_function(move |_, value: Value| -> mlua::Result<Table> {
            match value {
                Value::String(text) => {
                    let text = text.to_str()?.to_string();
                    make_actor.call((
                        text.eq_ignore_ascii_case("_blank"),
                        None::<String>,
                        Some(text),
                    ))
                }
                Value::Table(path) => {
                    let button = path.get::<Option<String>>("load_button")?;
                    let element = path.get::<Option<String>>("load_element")?;
                    let blank = element
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case("_blank"));
                    make_actor.call((blank, button, element))
                }
                _ => make_actor.call((false, None::<String>, None::<String>)),
            }
        })?
    };
    globals.set("LoadActor", load_actor)?;
    let var_fn = lua.create_function(|lua, name: String| {
        let globals = lua.globals();
        match name.as_str() {
            "Button" => Ok(Value::String(
                lua.create_string(&globals.get::<String>("__itg_button")?)?,
            )),
            "Element" => Ok(Value::String(
                lua.create_string(&globals.get::<String>("__itg_element")?)?,
            )),
            "SpriteOnly" => Ok(Value::Boolean(
                globals.get::<bool>("__itg_sprite_only").unwrap_or(false),
            )),
            _ => Ok(Value::Nil),
        }
    })?;
    globals.set("Var", var_fn)?;
    globals.set(
        "cmd",
        lua.create_function(|_, _args: MultiValue| Ok(Value::Nil))?,
    )?;
    let noteskin = lua.create_table()?;
    noteskin.set(
        "GetPath",
        lua.create_function(|lua, (_self, button, element): (Table, String, String)| {
            let path = lua.create_table()?;
            path.set("load_button", button)?;
            path.set("load_element", element)?;
            Ok(path)
        })?,
    )?;
    globals.set("NOTESKIN", noteskin)?;
    let def = lua.create_table()?;
    let actor_fn = {
        let make_actor = make_actor.clone();
        lua.create_function(move |_, _value: Value| -> mlua::Result<Table> {
            make_actor.call((true, None::<String>, None::<String>))
        })?
    };
    def.set("Actor", actor_fn)?;
    globals.set("Def", def)?;
    Ok(())
}

fn append_actor_command(
    lua: &Lua,
    actor: &Table,
    command: &str,
    args: MultiValue,
) -> mlua::Result<()> {
    let commands = actor
        .get::<Option<Table>>("__loader_commands")?
        .unwrap_or(lua.create_table()?);
    let mut token = command.to_string();
    for arg in args {
        token.push(',');
        token.push_str(&lua_command_arg(arg)?);
    }
    commands.raw_set(commands.raw_len() + 1, token)?;
    actor.set("__loader_commands", commands)
}

fn lua_command_arg(value: Value) -> mlua::Result<String> {
    Ok(match value {
        Value::Nil => String::new(),
        Value::Boolean(v) => v.to_string(),
        Value::Integer(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(v) => v.to_str()?.to_string(),
        _ => String::new(),
    })
}

fn load_noteskin_table(lua: &Lua, paths: &[PathBuf]) -> Result<Table, String> {
    let mut current = None;
    for path in paths {
        let content = fs::read_to_string(path)
            .map_err(|err| format!("failed to read '{}': {err}", path.display()))?;
        let chunk = lua.load(&content).set_name(path.to_string_lossy().as_ref());
        let function = chunk
            .into_function()
            .map_err(|err| format!("failed to compile '{}': {err}", path.display()))?;
        let next = if let Some(value) = current.take() {
            function
                .call(value)
                .map_err(|err| format!("failed to execute '{}': {err}", path.display()))?
        } else {
            function
                .call(())
                .map_err(|err| format!("failed to execute '{}': {err}", path.display()))?
        };
        current = Some(next);
    }
    current.ok_or_else(|| "no NoteSkin.lua files were found in fallback chain".to_string())
}

fn compile_entries(
    lua: &Lua,
    noteskin: &Table,
    data: &noteskin_itg::NoteskinData,
) -> Result<Vec<CompiledLoaderEntry>, String> {
    let (buttons, elements) = collect_loader_domain(data);
    normalize_noteskin_tables(noteskin, &buttons, &elements)
        .map_err(|err| format!("failed to normalize noteskin loader tables: {err}"))?;
    let load = noteskin
        .get::<Function>("Load")
        .map_err(|err| format!("compiled noteskin is missing Load(): {err}"))?;
    let globals = lua.globals();
    let mut out = Vec::with_capacity(buttons.len() * elements.len());
    for button in &buttons {
        for element in &elements {
            globals
                .set("__itg_button", button.as_str())
                .map_err(|err| err.to_string())?;
            globals
                .set("__itg_element", element.as_str())
                .map_err(|err| err.to_string())?;
            globals
                .set("__itg_sprite_only", true)
                .map_err(|err| err.to_string())?;
            let actor = load
                .call::<Table>(())
                .map_err(|err| format!("Load() failed for '{button} {element}': {err}"))?;
            out.push(read_entry(button, element, &actor)?);
        }
    }
    out.sort_by(|left, right| {
        (
            left.button.to_ascii_lowercase(),
            left.element.to_ascii_lowercase(),
        )
            .cmp(&(
                right.button.to_ascii_lowercase(),
                right.element.to_ascii_lowercase(),
            ))
    });
    Ok(out)
}

fn normalize_noteskin_tables(
    noteskin: &Table,
    buttons: &[String],
    elements: &[String],
) -> mlua::Result<()> {
    for key in ["RedirTable", "ButtonRedir", "ButtonRedirs", "Rotate"] {
        normalize_table_aliases(noteskin, key, buttons)?;
    }
    for key in [
        "ElementRedir",
        "ElementRedirs",
        "PartsToRotate",
        "Blank",
        "bBlanks",
    ] {
        normalize_table_aliases(noteskin, key, elements)?;
    }
    Ok(())
}

fn normalize_table_aliases(
    noteskin: &Table,
    table_key: &str,
    canonical_keys: &[String],
) -> mlua::Result<()> {
    let Some(table) = noteskin.get::<Option<Table>>(table_key)? else {
        return Ok(());
    };
    let mut existing = Vec::new();
    for pair in table.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(text) = key else {
            continue;
        };
        let Ok(text) = text.to_str() else {
            continue;
        };
        existing.push((text.to_string(), value));
    }
    for want in canonical_keys {
        if table.contains_key(want.as_str())? {
            continue;
        }
        if let Some((_, value)) = existing
            .iter()
            .find(|(have, _)| have.eq_ignore_ascii_case(want))
        {
            table.set(want.as_str(), value.clone())?;
        }
    }
    Ok(())
}

fn collect_loader_domain(data: &noteskin_itg::NoteskinData) -> (Vec<String>, Vec<String>) {
    let mut buttons = Vec::new();
    let mut button_seen = HashSet::new();
    for button in ["Left", "Down", "Up", "Right"] {
        push_unique(&mut buttons, &mut button_seen, button);
    }
    let mut elements = Vec::new();
    let mut element_seen = HashSet::new();
    for element in CORE_ELEMENTS {
        push_unique(&mut elements, &mut element_seen, element);
    }
    for dir in &data.search_dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let stem = trim_variant_suffix(name);
            let Some((button, element)) = split_prefixed_stem(stem) else {
                continue;
            };
            if let Some(button) = button {
                push_unique(&mut buttons, &mut button_seen, button);
            }
            push_unique(&mut elements, &mut element_seen, element);
        }
    }
    (buttons, elements)
}

fn push_unique(out: &mut Vec<String>, seen: &mut HashSet<String>, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    let key = trimmed.to_ascii_lowercase();
    if seen.insert(key) {
        out.push(trimmed.to_string());
    }
}

fn trim_variant_suffix(name: &str) -> &str {
    let stem = name.rsplit_once('.').map_or(name, |(head, _)| head).trim();
    let no_paren = stem
        .rsplit_once(" (")
        .map_or(stem, |(head, _)| head)
        .trim_end();
    match no_paren.rsplit_once(' ') {
        Some((head, tail))
            if tail
                .split_once('x')
                .is_some_and(|(w, h)| digits_only(w) && digits_only(h)) =>
        {
            head.trim_end()
        }
        _ => no_paren,
    }
}

fn split_prefixed_stem(stem: &str) -> Option<(Option<&str>, &str)> {
    let trimmed = stem.trim();
    if let Some(rest) = trimmed.strip_prefix("Fallback ") {
        return Some((None, rest.trim()));
    }
    for button in DANCE_BUTTONS {
        let Some(rest) = trimmed.strip_prefix(button) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix(' ') else {
            continue;
        };
        return Some((Some(button), rest.trim()));
    }
    None
}

fn digits_only(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

fn actor_loader_command(actor: &Table) -> Result<Option<String>, String> {
    let value = actor
        .get::<Value>("InitCommand")
        .map_err(|err| err.to_string())?;
    match value {
        Value::Function(f) => {
            f.call::<()>(actor.clone()).map_err(|err| err.to_string())?;
        }
        Value::String(s) => {
            let command = s.to_str().map_err(|err| err.to_string())?.to_string();
            if !command.trim().is_empty() {
                return Ok(Some(command));
            }
        }
        _ => {}
    }
    let Some(commands) = actor
        .get::<Option<Table>>("__loader_commands")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let mut out = Vec::with_capacity(commands.raw_len());
    for command in commands.sequence_values::<String>() {
        let command = command.map_err(|err| err.to_string())?;
        if !command.trim().is_empty() {
            out.push(command);
        }
    }
    Ok((!out.is_empty()).then(|| out.join(";")))
}

fn read_entry(button: &str, element: &str, actor: &Table) -> Result<CompiledLoaderEntry, String> {
    let blank = actor.get::<bool>("__blank").unwrap_or(false);
    let load_button = actor
        .get::<Option<String>>("__load_button")
        .map_err(|err| err.to_string())?
        .unwrap_or_else(|| button.to_string());
    let load_element = actor
        .get::<Option<String>>("__load_element")
        .map_err(|err| err.to_string())?
        .unwrap_or_else(|| element.to_string());
    let rotation_z = actor.get::<Option<i32>>("BaseRotationZ").unwrap_or(None);
    let init_command = actor_loader_command(actor)?;
    Ok(CompiledLoaderEntry {
        button: button.to_string(),
        element: element.to_string(),
        load_button,
        load_element,
        blank,
        rotation_z,
        init_command,
    })
}

#[cfg(test)]
mod tests {
    use super::{compiled_bundle_path, noteskin_actor, noteskin_compiled, noteskin_itg};
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn compiled_bundle_path_omits_version_dir() {
        let path = compiled_bundle_path(Path::new("noteskins"), " Dance ", " Default ", "hash123");
        let suffix = Path::new("noteskins")
            .join("dance")
            .join("default")
            .join("hash123.bin");
        let version_dir = format!("v{}", noteskin_compiled::CACHE_SCHEMA_VERSION);
        assert!(path.ends_with(&suffix));
        assert!(
            path.components()
                .all(|component| component.as_os_str() != OsStr::new(&version_dir))
        );
    }

    #[test]
    fn actor_decl_ignores_non_color_local_assignments() {
        let decl = noteskin_actor::parse_actor_decl(
            r#"
local button = Var "Button"
local path = NOTESKIN:GetPath(button, "Tap Note")
return Def.Sprite {
    Texture=path;
    InitCommand=cmd(diffusealpha,1);
}
"#,
            &noteskin_itg::IniData::default(),
        );

        let sprite = decl.sprites.first().expect("sprite should parse");
        assert_eq!(
            sprite.commands.get("initcommand").map(String::as_str),
            Some("diffusealpha,1")
        );
    }

    #[test]
    fn actor_decl_preserves_repeated_sprite_frame_states() {
        let decl = noteskin_actor::parse_actor_decl(
            r#"
return Def.Sprite {
    Texture=NOTESKIN:GetPath('_Down', 'roll body active');
    Frame0000=0;
    Delay0000=0.44;
    Frame0001=1;
    Delay0001=0.03;
    Frame0002=2;
    Delay0002=0.03;
    Frame0003=3;
    Delay0003=0.44;
    Frame0004=2;
    Delay0004=0.03;
    Frame0005=1;
    Delay0005=0.03;
};
"#,
            &noteskin_itg::IniData::default(),
        );

        let sprite = decl.sprites.first().expect("sprite should parse");
        assert_eq!(sprite.frame0, 0);
        assert_eq!(sprite.frame_count, 6);
        assert_eq!(
            sprite.frame_indices.as_deref(),
            Some([0, 1, 2, 3, 2, 1].as_slice())
        );
        let delays = sprite
            .frame_delays
            .as_deref()
            .expect("sprite frame delays should parse");
        assert_eq!(delays, [0.44, 0.03, 0.03, 0.44, 0.03, 0.03]);
        assert!((delays.iter().sum::<f32>() - 1.0).abs() <= 1e-6);
    }

    #[test]
    fn actor_decl_expands_local_lua_command_helpers() {
        let decl = noteskin_actor::parse_actor_decl(
            r##"
local W2colour = color("#FFC917")
local Lastcolour = color("#00C8FF")

local function flashadd(thecolour, updatelast)
    return function(self)
        if updatelast then
            Lastcolour = thecolour
        end
        self:finishtweening()
        :diffuse(thecolour)
        :blend(Blend.Add)
        :diffusealpha(1.0)
        :linear(1/60)
        :diffusealpha(0.5)
        :linear(3/60)
        :diffusealpha(0.0)
    end
end

local function flashnormal(thecolour, uselast)
    return function(self)
        if uselast then
            self:finishtweening()
            :diffusealpha(1.0)
            :linear(10/60)
            :diffusealpha(0.0)
        else
            self:finishtweening()
            :diffuse(thecolour)
            :diffusealpha(1.0)
            :linear(10/60)
            :diffusealpha(0.0)
        end
    end
end

return Def.ActorFrame {
    Def.Sprite {
        Texture=NOTESKIN:GetPath(Var "Button", "Flash");
        InitCommand=cmd(diffusealpha,0);
        W2Command=flashadd(W2colour,true);
        HeldCommand=flashnormal(Lastcolour,true);
        ECommand=function(self) self:blend("BlendMode_Normal"):diffusealpha(1.0):zoom(0.75):accelerate(64/60):diffusealpha(0.0):zoom(1.0):setstate(0):animate(true) end;
        JudgmentCommand=function(self) end;
    };
}
"##,
            &noteskin_itg::IniData::default(),
        );
        let sprite = decl.sprites.first().expect("sprite should parse");

        let w2 = sprite
            .commands
            .get("w2command")
            .expect("W2 command should compile");
        assert!(w2.contains("diffuse,1,0.7882353,0.09019608,1"));
        assert!(w2.contains("blend,Blend.Add"));
        assert!(w2.contains("linear,1/60"));
        assert!(!w2.contains("flashadd"));

        let held = sprite
            .commands
            .get("heldcommand")
            .expect("Held command should compile");
        assert!(held.contains("diffusealpha,1"));
        assert!(held.contains("linear,10/60"));
        assert!(!held.contains("diffuse,0,0.78431374,1,1"));
        assert!(!held.contains("flashnormal"));

        let e = sprite
            .commands
            .get("ecommand")
            .expect("E command should compile");
        assert!(e.contains("blend,\"BlendMode_Normal\""));
        assert!(e.contains("diffusealpha,1.0"));
        assert!(e.contains("zoom,0.75"));
        assert!(e.contains("accelerate,64/60"));
        assert!(e.contains("setstate,0"));
        assert!(e.contains("animate,true"));

        assert_eq!(
            sprite.commands.get("judgmentcommand").map(String::as_str),
            Some("")
        );
    }
}
