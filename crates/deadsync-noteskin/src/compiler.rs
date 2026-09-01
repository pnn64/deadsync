pub use crate::compiled::{CompiledActors, CompiledLoader, actor_manifest_key};
use crate::{
    actor as noteskin_actor, compiled as noteskin_compiled,
    compiled::{CompiledActorFile, CompiledLoaderEntry, CompiledNoteskinBundle},
    itg as noteskin_itg,
};
use log::{info, warn};
use mlua::{Function, Lua, MultiValue, Table, Value};
use smallvec::SmallVec;
use std::cmp::Ordering;
use std::collections::HashMap;
#[cfg(any(test, feature = "bench-support"))]
use std::collections::HashSet;
use std::fs;
#[cfg(any(test, feature = "bench-support"))]
use std::hash::BuildHasherDefault;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use twox_hash::XxHash64;

const COMPILER_VERSION: u32 = 13;
static COMPILED_HASH_CACHE: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
const PUMP_BUTTONS: [&str; 5] = ["DownLeft", "UpLeft", "Center", "UpRight", "DownRight"];
const DANCE_BUTTONS: [&str; 4] = ["Left", "Down", "Up", "Right"];
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
const ASCII_CASE_HASH_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const ASCII_CASE_HASH_PRIME: u64 = 0x0100_0000_01b3;
#[cfg(any(test, feature = "bench-support"))]
type TrustedHashSet<T> = HashSet<T, BuildHasherDefault<XxHash64>>;
// The built-in loader domain has 33 elements. Keeping 64 fingerprints inline
// also covers typical third-party additions without heap storage.
const INLINE_LOADER_DOMAIN_KEYS: usize = 64;
type LoaderFingerprints = SmallVec<[u64; INLINE_LOADER_DOMAIN_KEYS]>;

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

#[must_use]
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
                warn!("noteskin cache compile failed for '{label}': {err}");
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
    let game = game.trim();
    let skin = skin.trim();
    let mut key = String::with_capacity(game.len() + 1 + skin.len());
    key.push_str(game);
    key.push('/');
    key.push_str(skin);
    key.make_ascii_lowercase();
    key
}

fn source_hash(game: &str, data: &noteskin_itg::NoteskinData) -> Result<String, String> {
    let sources = labeled_source_paths(data, source_paths(data));
    let mut hasher = XxHash64::default();
    hasher.write_u32(noteskin_compiled::CACHE_SCHEMA_VERSION);
    hasher.write_u32(COMPILER_VERSION);
    hasher.write(game.as_bytes());
    hasher.write(data.name.as_bytes());
    for (label, path) in sources {
        hasher.write(label.as_bytes());
        let bytes = fs::read(&path)
            .map_err(|err| format!("failed to read '{}' for hashing: {err}", path.display()))?;
        hasher.write(&bytes);
    }
    Ok(format!("{:016x}", hasher.finish()))
}

fn labeled_source_paths(
    data: &noteskin_itg::NoteskinData,
    paths: Vec<PathBuf>,
) -> Vec<(String, PathBuf)> {
    let mut sources: Vec<_> = paths
        .into_iter()
        .map(|path| (source_label(data, &path), path))
        .collect();
    sources.sort_by(|left, right| left.0.cmp(&right.0));
    sources
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
        let rel = path.strip_prefix(dir).unwrap_or(path).to_string_lossy();
        let mut label = String::with_capacity(game.len() + skin.len() + rel.len() + 2);
        label.push_str(game);
        label.push('/');
        label.push_str(skin);
        label.push('/');
        push_normalized_path(&mut label, &rel);
        label.make_ascii_lowercase();
        return label;
    }
    let path = path.to_string_lossy();
    let mut label = String::with_capacity(path.len());
    push_normalized_path(&mut label, &path);
    label.make_ascii_lowercase();
    label
}

fn push_normalized_path(out: &mut String, path: &str) {
    let mut parts = path.split('\\');
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    for part in parts {
        out.push('/');
        out.push_str(part);
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn compiled_hash_cache_key_reference(game: &str, skin: &str) -> String {
    format!(
        "{}/{}",
        game.trim().to_ascii_lowercase(),
        skin.trim().to_ascii_lowercase()
    )
}

#[cfg(any(test, feature = "bench-support"))]
fn source_label_reference(data: &noteskin_itg::NoteskinData, path: &Path) -> String {
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
            .map(|path| path.to_string_lossy().replace('\\', "/"))
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

#[cfg(any(test, feature = "bench-support"))]
fn labeled_source_paths_reference(
    data: &noteskin_itg::NoteskinData,
    mut paths: Vec<PathBuf>,
) -> Vec<(String, PathBuf)> {
    paths.sort_by_key(|path| source_label(data, path));
    paths
        .into_iter()
        .map(|path| (source_label(data, &path), path))
        .collect()
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub mod compiler_bench_support {
    use super::{
        CompiledLoaderEntry, LoaderFingerprints, TrustedHashSet, compiled_hash_cache_key,
        compiled_hash_cache_key_reference, labeled_source_paths, labeled_source_paths_reference,
        normalize_table_aliases, normalize_table_aliases_reference, noteskin_itg, push_unique,
        push_unique_full_scan_reference, push_unique_reference, sort_compiled_loader_entries,
        sort_compiled_loader_entries_reference, source_label,
    };
    use mlua::{Lua, Table, Value};
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    #[must_use]
    pub fn cache_key_current(game: &str, skin: &str) -> String {
        compiled_hash_cache_key(game, skin)
    }

    #[must_use]
    pub fn cache_key_reference(game: &str, skin: &str) -> String {
        compiled_hash_cache_key_reference(game, skin)
    }

    #[must_use]
    pub fn source_label_current(data: &noteskin_itg::NoteskinData, path: &Path) -> String {
        source_label(data, path)
    }

    #[must_use]
    pub fn source_label_reference(data: &noteskin_itg::NoteskinData, path: &Path) -> String {
        super::source_label_reference(data, path)
    }

    fn labels_checksum(sources: Vec<(String, PathBuf)>) -> u64 {
        sources.into_iter().fold(0_u64, |mut checksum, (label, _)| {
            checksum = checksum
                .wrapping_mul(1_099_511_628_211)
                .wrapping_add(label.len() as u64);
            for byte in label.bytes() {
                checksum = checksum
                    .wrapping_mul(1_099_511_628_211)
                    .wrapping_add(u64::from(byte));
            }
            checksum
        })
    }

    #[must_use]
    pub fn source_order_current(data: &noteskin_itg::NoteskinData, paths: &[PathBuf]) -> u64 {
        labels_checksum(labeled_source_paths(data, paths.to_vec()))
    }

    #[must_use]
    pub fn source_order_reference(data: &noteskin_itg::NoteskinData, paths: &[PathBuf]) -> u64 {
        labels_checksum(labeled_source_paths_reference(data, paths.to_vec()))
    }

    fn loader_entries(cases: &[(&str, &str)]) -> Vec<CompiledLoaderEntry> {
        cases
            .iter()
            .map(|&(button, element)| CompiledLoaderEntry {
                button: button.to_string(),
                element: element.to_string(),
                load_button: String::new(),
                load_element: String::new(),
                blank: false,
                rotation_x: None,
                rotation_y: None,
                rotation_z: None,
                init_command: None,
            })
            .collect()
    }

    fn loader_entries_checksum(entries: Vec<CompiledLoaderEntry>) -> u64 {
        entries.into_iter().fold(0_u64, |checksum, entry| {
            let checksum = entry.button.bytes().fold(checksum, |sum, byte| {
                sum.wrapping_mul(1_099_511_628_211)
                    .wrapping_add(u64::from(byte))
            });
            entry.element.bytes().fold(checksum, |sum, byte| {
                sum.wrapping_mul(1_099_511_628_211)
                    .wrapping_add(u64::from(byte))
            })
        })
    }

    #[must_use]
    pub fn loader_entry_sort_current(cases: &[(&str, &str)]) -> u64 {
        let mut entries = loader_entries(cases);
        sort_compiled_loader_entries(&mut entries);
        loader_entries_checksum(entries)
    }

    #[must_use]
    pub fn loader_entry_sort_reference(cases: &[(&str, &str)]) -> u64 {
        let mut entries = loader_entries(cases);
        sort_compiled_loader_entries_reference(&mut entries);
        loader_entries_checksum(entries)
    }

    fn strings_checksum(values: Vec<String>) -> u64 {
        values.into_iter().fold(0_u64, |checksum, value| {
            value.bytes().fold(checksum, |sum, byte| {
                sum.wrapping_mul(1_099_511_628_211)
                    .wrapping_add(u64::from(byte))
            })
        })
    }

    fn indexed_domain(values: &[&str], reserve: bool) -> u64 {
        let mut out = if reserve {
            Vec::with_capacity(values.len())
        } else {
            Vec::new()
        };
        let mut seen = LoaderFingerprints::new();
        for value in values {
            push_unique(&mut out, &mut seen, value);
        }
        strings_checksum(out)
    }

    #[must_use]
    pub fn loader_domain_current(values: &[&str]) -> u64 {
        indexed_domain(values, true)
    }

    #[must_use]
    pub fn unreserved_loader_domain_old(values: &[&str]) -> u64 {
        indexed_domain(values, false)
    }

    #[must_use]
    pub fn reserved_loader_domain_new(values: &[&str]) -> u64 {
        indexed_domain(values, true)
    }

    #[must_use]
    pub fn heap_fingerprint_scan_old(values: &[&str]) -> u64 {
        let mut out = Vec::with_capacity(values.len());
        let mut seen = TrustedHashSet::with_capacity_and_hasher(values.len(), Default::default());
        for value in values {
            push_unique_full_scan_reference(&mut out, &mut seen, value);
        }
        strings_checksum(out)
    }

    #[must_use]
    pub fn stack_fingerprint_index_new(values: &[&str]) -> u64 {
        indexed_domain(values, true)
    }

    #[must_use]
    pub fn loader_domain_reference(values: &[&str]) -> u64 {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for value in values {
            push_unique_reference(&mut out, &mut seen, value);
        }
        strings_checksum(out)
    }

    fn alias_keys() -> &'static [String] {
        static KEYS: OnceLock<Vec<String>> = OnceLock::new();
        KEYS.get_or_init(|| ["Left", "Down", "Up", "Right"].map(str::to_owned).into())
    }

    #[must_use]
    pub fn alias_fixture(lua: &Lua) -> Table {
        let noteskin = lua.create_table().expect("benchmark noteskin table");
        let aliases = lua.create_table().expect("benchmark alias table");
        for (key, value) in [
            ("left", "L"),
            ("DOWN", "D"),
            ("uP", "U"),
            ("right", "R"),
            ("Center", "C"),
            ("Corner", "X"),
        ] {
            aliases.set(key, value).expect("benchmark alias entry");
        }
        noteskin
            .set("ButtonRedir", aliases)
            .expect("benchmark alias map");
        noteskin
    }

    fn alias_normalization(noteskin: &Table, evaluations: usize, optimized: bool) -> u64 {
        let aliases = noteskin
            .get::<Table>("ButtonRedir")
            .expect("benchmark alias map");
        (0..evaluations).fold(0_u64, |mut checksum, _| {
            for want in alias_keys() {
                aliases
                    .set(want.as_str(), Value::Nil)
                    .expect("reset canonical alias");
            }
            if optimized {
                normalize_table_aliases(noteskin, "ButtonRedir", alias_keys())
            } else {
                normalize_table_aliases_reference(noteskin, "ButtonRedir", alias_keys())
            }
            .expect("normalize benchmark aliases");
            for want in alias_keys() {
                let value = aliases
                    .get::<String>(want.as_str())
                    .expect("normalized benchmark alias");
                checksum = value.bytes().fold(checksum, |sum, byte| {
                    sum.wrapping_mul(1_099_511_628_211)
                        .wrapping_add(u64::from(byte))
                });
            }
            checksum
        })
    }

    #[must_use]
    pub fn owned_alias_snapshot_old(noteskin: &Table, evaluations: usize) -> u64 {
        alias_normalization(noteskin, evaluations, false)
    }

    #[must_use]
    pub fn stack_alias_snapshot_new(noteskin: &Table, evaluations: usize) -> u64 {
        alias_normalization(noteskin, evaluations, true)
    }
}

fn compile_data(
    game: &str,
    data: &noteskin_itg::NoteskinData,
    source_hash: &str,
) -> Result<CompiledNoteskinBundle, String> {
    let scripts = noteskin_paths(data);
    let lua = Lua::new();
    install_host(&lua, data).map_err(|err| err.to_string())?;
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
            entries: compile_entries(&lua, &noteskin, game, data)?,
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

fn install_host(lua: &Lua, data: &noteskin_itg::NoteskinData) -> mlua::Result<()> {
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
        let actor_mt = actor_mt;
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
            make_actor_for_path(&make_actor, value)
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
    let sprite_fn = {
        let make_actor = make_actor.clone();
        lua.create_function(move |_, value: Value| -> mlua::Result<Table> {
            let Value::Table(props) = value else {
                return make_actor.call((false, None::<String>, None::<String>));
            };
            let actor = match props.get::<Value>("Texture")? {
                Value::Nil => make_actor.call((false, None::<String>, None::<String>))?,
                texture => make_actor_for_path(&make_actor, texture)?,
            };
            copy_actor_fields(&props, &actor)?;
            Ok(actor)
        })?
    };
    def.set("Sprite", sprite_fn)?;
    globals.set("Def", def)?;
    let data = data.clone();
    let loadfile = {
        let make_actor = make_actor;
        lua.create_function(move |lua, value: Value| -> mlua::Result<Value> {
            let Some((button, element)) = loadfile_target(&data, value)? else {
                return Ok(Value::Nil);
            };
            let make_actor = make_actor.clone();
            let func = lua.create_function(move |_, _args: MultiValue| -> mlua::Result<Table> {
                make_actor.call((false, button.clone(), element.clone()))
            })?;
            Ok(Value::Function(func))
        })?
    };
    globals.set("loadfile", loadfile)?;
    Ok(())
}

fn make_actor_for_path(make_actor: &Function, value: Value) -> mlua::Result<Table> {
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
}

fn copy_actor_fields(src: &Table, dst: &Table) -> mlua::Result<()> {
    for key in [
        "InitCommand",
        "BaseRotationX",
        "BaseRotationY",
        "BaseRotationZ",
    ] {
        let value = src.get::<Value>(key)?;
        if !matches!(value, Value::Nil) {
            dst.set(key, value)?;
        }
    }
    Ok(())
}

fn loadfile_target(
    data: &noteskin_itg::NoteskinData,
    value: Value,
) -> mlua::Result<Option<(Option<String>, Option<String>)>> {
    match value {
        Value::Table(path) => {
            let button = path.get::<Option<String>>("load_button")?;
            let element = path.get::<Option<String>>("load_element")?;
            let Some(element_value) = element.as_deref() else {
                return Ok(None);
            };
            let button_value = button.as_deref().unwrap_or("");
            let Some(path) = data.resolve_path(button_value, element_value) else {
                return Ok(None);
            };
            Ok(path_is_lua(&path).then_some((button, element)))
        }
        Value::String(path) => {
            let path = PathBuf::from(path.to_str()?.as_ref());
            Ok(path_is_lua(&path).then_some((None, None)))
        }
        _ => Ok(None),
    }
}

fn path_is_lua(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
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
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> Result<Vec<CompiledLoaderEntry>, String> {
    let (buttons, elements) = collect_loader_domain(game, data);
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
    sort_compiled_loader_entries(&mut out);
    Ok(out)
}

#[derive(Debug, Eq, PartialEq)]
struct LoaderSortKey {
    normalized: String,
    button_len: usize,
}

impl LoaderSortKey {
    fn new(button: &str, element: &str) -> Self {
        let mut normalized = String::with_capacity(button.len() + element.len());
        normalized.push_str(button);
        let button_len = normalized.len();
        normalized.push_str(element);
        normalized.make_ascii_lowercase();
        Self {
            normalized,
            button_len,
        }
    }

    fn button(&self) -> &str {
        &self.normalized[..self.button_len]
    }

    fn element(&self) -> &str {
        &self.normalized[self.button_len..]
    }
}

impl Ord for LoaderSortKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.button()
            .cmp(other.button())
            .then_with(|| self.element().cmp(other.element()))
    }
}

impl PartialOrd for LoaderSortKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn sort_compiled_loader_entries(entries: &mut [CompiledLoaderEntry]) {
    entries.sort_by_cached_key(|entry| LoaderSortKey::new(&entry.button, &entry.element));
}

#[cfg(any(test, feature = "bench-support"))]
fn sort_compiled_loader_entries_reference(entries: &mut [CompiledLoaderEntry]) {
    entries.sort_by_cached_key(|entry| {
        (
            entry.button.to_ascii_lowercase(),
            entry.element.to_ascii_lowercase(),
        )
    });
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
    let mut existing = SmallVec::<[(u64, mlua::LuaString, Value); 8]>::new();
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(text) = key else {
            continue;
        };
        let Ok(text_ref) = text.to_str() else {
            continue;
        };
        let fingerprint = ascii_case_hash(&text_ref);
        existing.push((fingerprint, text, value));
    }
    for want in canonical_keys {
        if table.contains_key(want.as_str())? {
            continue;
        }
        let fingerprint = ascii_case_hash(want);
        if let Some((_, _, value)) = existing.iter().find(|(hash, have, _)| {
            *hash == fingerprint
                && have
                    .to_str()
                    .is_ok_and(|have| have.eq_ignore_ascii_case(want))
        }) {
            table.set(want.as_str(), value.clone())?;
        }
    }
    Ok(())
}

#[cfg(any(test, feature = "bench-support"))]
fn normalize_table_aliases_reference(
    noteskin: &Table,
    table_key: &str,
    canonical_keys: &[String],
) -> mlua::Result<()> {
    let Some(table) = noteskin.get::<Option<Table>>(table_key)? else {
        return Ok(());
    };
    let mut existing = Vec::new();
    for pair in table.pairs::<Value, Value>() {
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

fn game_buttons(game: &str) -> &'static [&'static str] {
    if game.trim().eq_ignore_ascii_case("pump") {
        &PUMP_BUTTONS
    } else {
        &DANCE_BUTTONS
    }
}

fn collect_loader_domain(
    game: &str,
    data: &noteskin_itg::NoteskinData,
) -> (Vec<String>, Vec<String>) {
    let game_buttons = game_buttons(game);
    let mut buttons = Vec::with_capacity(game_buttons.len());
    let mut button_seen = LoaderFingerprints::new();
    for &button in game_buttons {
        push_unique(&mut buttons, &mut button_seen, button);
    }
    let mut elements = Vec::with_capacity(CORE_ELEMENTS.len());
    let mut element_seen = LoaderFingerprints::new();
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
            let Some((button, element)) = split_prefixed_stem(stem, game_buttons) else {
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

fn ascii_case_hash(value: &str) -> u64 {
    value.bytes().fold(ASCII_CASE_HASH_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte.to_ascii_lowercase())).wrapping_mul(ASCII_CASE_HASH_PRIME)
    })
}

fn push_unique(out: &mut Vec<String>, seen: &mut LoaderFingerprints, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    let fingerprint = ascii_case_hash(trimmed);
    for (index, &existing_fingerprint) in seen.iter().enumerate() {
        if existing_fingerprint == fingerprint && out[index].eq_ignore_ascii_case(trimmed) {
            return;
        }
    }
    seen.push(fingerprint);
    out.push(trimmed.to_string());
}

#[cfg(any(test, feature = "bench-support"))]
fn push_unique_full_scan_reference(
    out: &mut Vec<String>,
    seen: &mut TrustedHashSet<u64>,
    value: &str,
) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    let fingerprint = ascii_case_hash(trimmed);
    if seen.insert(fingerprint)
        || !out
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(trimmed))
    {
        out.push(trimmed.to_string());
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn push_unique_reference(out: &mut Vec<String>, seen: &mut HashSet<String>, value: &str) {
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

fn split_prefixed_stem<'a>(stem: &'a str, buttons: &[&str]) -> Option<(Option<&'a str>, &'a str)> {
    let trimmed = stem.trim();
    if let Some(rest) = trimmed.strip_prefix("Fallback ") {
        return Some((None, rest.trim()));
    }
    for &button in buttons {
        let Some(rest) = trimmed.strip_prefix(button) else {
            continue;
        };
        let Some(rest) = rest.strip_prefix(' ') else {
            continue;
        };
        return Some((Some(&trimmed[..button.len()]), rest.trim()));
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
    let rotation_x = actor.get::<Option<i32>>("BaseRotationX").unwrap_or(None);
    let rotation_y = actor.get::<Option<i32>>("BaseRotationY").unwrap_or(None);
    let rotation_z = actor.get::<Option<i32>>("BaseRotationZ").unwrap_or(None);
    let init_command = actor_loader_command(actor)?;
    Ok(CompiledLoaderEntry {
        button: button.to_string(),
        element: element.to_string(),
        load_button,
        load_element,
        blank,
        rotation_x,
        rotation_y,
        rotation_z,
        init_command,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        CompiledLoaderEntry, LoaderFingerprints, TrustedHashSet, ascii_case_hash,
        compiled_bundle_path, compiled_hash_cache_key, compiled_hash_cache_key_reference,
        labeled_source_paths, labeled_source_paths_reference, normalize_table_aliases,
        normalize_table_aliases_reference, noteskin_actor, noteskin_compiled, noteskin_itg,
        push_unique, push_unique_full_scan_reference, push_unique_reference,
        sort_compiled_loader_entries, sort_compiled_loader_entries_reference, source_label,
        source_label_reference,
    };
    use mlua::Lua;
    use std::{
        collections::HashSet,
        ffi::OsStr,
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_noteskin_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-noteskin-compiler-{name}-{}-{suffix}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn compiled_hash_cache_keys_match_owned_normalization() {
        for (game, skin) in [
            (" Dance ", " Default "),
            ("PUMP", "CeL"),
            ("techno", "Café"),
            ("  ", ""),
        ] {
            assert_eq!(
                compiled_hash_cache_key(game, skin),
                compiled_hash_cache_key_reference(game, skin)
            );
        }
        assert_eq!(
            compiled_hash_cache_key(" Dance ", " Default "),
            "dance/default"
        );
    }

    #[test]
    fn source_labels_match_replace_and_lowercase_behavior() {
        let dir = PathBuf::from("Assets")
            .join("NoteSkins")
            .join("DaNcE")
            .join("DeFaUlT");
        let data = noteskin_itg::NoteskinData {
            name: "Default".to_string(),
            metrics: noteskin_itg::IniData::default(),
            search_dirs: vec![dir.clone()],
        };
        let cases = [
            dir.join("Down Receptor.LUA"),
            dir.join("Café Tap Note.lua"),
            PathBuf::from("Outside\\MiXeD/Actor.LUA"),
            PathBuf::new(),
        ];
        for path in &cases {
            assert_eq!(
                source_label(&data, path),
                source_label_reference(&data, path),
                "path {path:?}"
            );
        }
        assert_eq!(
            source_label(&data, &dir.join("Down Receptor.LUA")),
            "dance/default/down receptor.lua"
        );
    }

    #[test]
    fn cached_source_labels_preserve_hash_order() {
        let dir = PathBuf::from("Assets")
            .join("NoteSkins")
            .join("Pump")
            .join("Café");
        let data = noteskin_itg::NoteskinData {
            name: "Café".to_string(),
            metrics: noteskin_itg::IniData::default(),
            search_dirs: vec![dir.clone()],
        };
        let paths = vec![
            dir.join("Zeta.lua"),
            dir.join("alpha.lua"),
            dir.join("Center Tap Note.lua"),
            PathBuf::from("External\\Fallback.lua"),
        ];
        assert_eq!(
            labeled_source_paths(&data, paths.clone()),
            labeled_source_paths_reference(&data, paths)
        );
    }

    #[test]
    fn single_buffer_loader_sort_keys_preserve_tuple_ordering() {
        let entries = [
            ("Up", "Tap Note"),
            ("left", "Receptor"),
            ("LEFT", "Hold Body Active"),
            ("Down", "Tap Mine"),
            ("CafÃ©", "Ã‰clair"),
            ("Left", "Explosion"),
        ];
        let build = || {
            entries
                .iter()
                .map(|&(button, element)| CompiledLoaderEntry {
                    button: button.to_string(),
                    element: element.to_string(),
                    load_button: button.to_string(),
                    load_element: element.to_string(),
                    blank: false,
                    rotation_x: None,
                    rotation_y: None,
                    rotation_z: None,
                    init_command: None,
                })
                .collect::<Vec<_>>()
        };
        let mut current = build();
        let mut reference = build();
        sort_compiled_loader_entries(&mut current);
        sort_compiled_loader_entries_reference(&mut reference);
        assert_eq!(current, reference);
    }

    #[test]
    fn hashed_loader_domain_dedup_preserves_case_and_order() {
        let values = [
            " Left ", "left", "Tap Note", "TAP NOTE", "CafÃ©", "cafÃ©", "", "  ", "Receptor",
        ];
        let mut current = Vec::new();
        let mut current_seen = LoaderFingerprints::new();
        let mut prior = Vec::new();
        let mut prior_seen = TrustedHashSet::default();
        let mut reference = Vec::new();
        let mut reference_seen = HashSet::new();
        for value in values {
            push_unique(&mut current, &mut current_seen, value);
            push_unique_full_scan_reference(&mut prior, &mut prior_seen, value);
            push_unique_reference(&mut reference, &mut reference_seen, value);
        }
        assert_eq!(current, prior);
        assert_eq!(current, reference);
        assert_eq!(current, ["Left", "Tap Note", "CafÃ©", "Receptor"]);

        let mut collision_out = vec!["Left".to_string()];
        let mut collision_seen = LoaderFingerprints::new();
        collision_seen.push(ascii_case_hash("Right"));
        push_unique(&mut collision_out, &mut collision_seen, "Right");
        assert_eq!(collision_out, ["Left", "Right"]);
    }

    #[test]
    fn borrowed_alias_snapshot_matches_owned_key_normalization() {
        let canonical = ["Left", "Down", "Up", "Right"].map(str::to_owned);
        let build = |lua: &Lua| {
            let noteskin = lua.create_table().unwrap();
            let aliases = lua.create_table().unwrap();
            for (key, value) in [
                ("left", "L"),
                ("DOWN", "D"),
                ("uP", "U"),
                ("Right", "exact"),
                ("right", "fallback"),
            ] {
                aliases.set(key, value).unwrap();
            }
            noteskin.set("ButtonRedir", aliases).unwrap();
            noteskin
        };
        let current_lua = Lua::new();
        let reference_lua = Lua::new();
        let current = build(&current_lua);
        let reference = build(&reference_lua);

        normalize_table_aliases(&current, "ButtonRedir", &canonical).unwrap();
        normalize_table_aliases_reference(&reference, "ButtonRedir", &canonical).unwrap();

        let current = current.get::<mlua::Table>("ButtonRedir").unwrap();
        let reference = reference.get::<mlua::Table>("ButtonRedir").unwrap();
        for key in canonical {
            assert_eq!(
                current.get::<String>(key.as_str()).unwrap(),
                reference.get::<String>(key.as_str()).unwrap(),
                "key {key:?}"
            );
        }
    }

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
    fn loader_loadfile_accepts_noteskin_path_tables() {
        let root = temp_noteskin_dir("loadfile-path-table");
        let skin_dir = root.join("dance/sch");
        fs::create_dir_all(&skin_dir).unwrap();
        fs::write(
            skin_dir.join("NoteSkin.lua"),
            r#"local skin = {}
skin.ButtonRedir = { Left = "Down", Down = "Down", Up = "Down", Right = "Down" }
skin.PartsToRotate = { Receptor = true, ["Hold Body Active"] = true }
skin.Rotate = { Left = 90, Down = 0, Up = 180, Right = -90 }

function skin.Load()
    local button = Var "Button"
    local element = Var "Element"
    local load_button = skin.ButtonRedir[button] or button
    local actor_file = loadfile(NOTESKIN:GetPath(load_button, element))
    local actor
    if type(actor_file) == "function" then
        actor = actor_file(nil)
    else
        actor = Def.Sprite {
            Texture = NOTESKIN:GetPath(load_button, element),
            BaseRotationX = 180,
            BaseRotationY = 180,
        }
    end
    if skin.PartsToRotate[element] then
        actor.BaseRotationZ = skin.Rotate[button]
    end
    return actor
end

return skin
"#,
        )
        .unwrap();
        fs::write(
            skin_dir.join("Down Receptor.lua"),
            r#"return Def.Sprite { Texture=NOTESKIN:GetPath("Down", "Receptor") }"#,
        )
        .unwrap();
        fs::write(skin_dir.join("Down Hold Body Active.png"), []).unwrap();
        fs::write(
            skin_dir.join("Fallback Explosion.lua"),
            r#"return Def.Actor {}"#,
        )
        .unwrap();

        let data = noteskin_itg::NoteskinData {
            name: "sch".to_string(),
            metrics: noteskin_itg::IniData::default(),
            search_dirs: vec![skin_dir],
        };
        let bundle = super::compile_data("dance", &data, "testhash").expect("compile data");
        let receptor = bundle.loader.load_request("Left", "Receptor");
        let hold_body = bundle.loader.load_request("Left", "Hold Body Active");
        let explosion = bundle.loader.load_request("Left", "Explosion");

        assert_eq!(receptor.load_button, "Down");
        assert_eq!(receptor.load_element, "Receptor");
        assert_eq!(receptor.rotation_z, Some(90));
        assert_eq!(hold_body.load_button, "Down");
        assert_eq!(hold_body.load_element, "Hold Body Active");
        assert_eq!(hold_body.rotation_x, Some(180));
        assert_eq!(hold_body.rotation_y, Some(180));
        assert_eq!(hold_body.rotation_z, Some(90));
        assert_eq!(explosion.load_button, "Down");
        assert_eq!(explosion.load_element, "Explosion");

        let _ = fs::remove_dir_all(root);
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
