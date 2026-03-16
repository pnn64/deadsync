use super::{
    noteskin_actor,
    noteskin_compiled::{
        self, CompiledActorFile, CompiledActors, CompiledLoader, CompiledLoaderEntry,
        CompiledNoteskinBundle,
    },
    noteskin_itg,
};
use log::info;
use mlua::{Function, Lua, MultiValue, Table, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use twox_hash::XxHash64;

const COMPILER_VERSION: u32 = 2;
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

pub fn ensure_compiled(
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Result<CompileOutcome, String> {
    if let Some(path) = cached_bundle_path(game, &data.name)
        && noteskin_compiled::load_compiled_bundle(&path).is_some()
    {
        return Ok(CompileOutcome::Reused);
    }
    let source_hash = source_hash(game, data)?;
    remember_source_hash(game, &data.name, &source_hash);
    let path = noteskin_compiled::compiled_bundle_path(game, &data.name, &source_hash);
    if noteskin_compiled::load_compiled_bundle(&path).is_some() {
        return Ok(CompileOutcome::Reused);
    }
    info!("compiling noteskin cache for '{game}/{}'", data.name);
    let bundle = compile_data(game, data, &source_hash)?;
    noteskin_compiled::save_compiled_bundle(&path, &bundle)?;
    Ok(CompileOutcome::Built)
}

#[allow(dead_code)]
pub fn load_compiled(
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Option<CompiledNoteskinBundle> {
    let path = cached_bundle_path(game, &data.name)?;
    noteskin_compiled::load_compiled_bundle(&path)
}

fn cached_bundle_path(game: &str, skin: &str) -> Option<PathBuf> {
    let key = compiled_hash_cache_key(game, skin);
    let hash = COMPILED_HASH_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&key)
        .cloned()?;
    Some(noteskin_compiled::compiled_bundle_path(game, skin, &hash))
}

fn remember_source_hash(game: &str, skin: &str, source_hash: &str) {
    let key = compiled_hash_cache_key(game, skin);
    COMPILED_HASH_CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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
    paths.sort_by(|left, right| source_label(data, left).cmp(&source_label(data, right)));
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
    Ok(CompiledLoaderEntry {
        button: button.to_string(),
        element: element.to_string(),
        load_button,
        load_element,
        blank,
        rotation_z,
    })
}
