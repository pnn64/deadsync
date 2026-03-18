use super::noteskin_actor;
use bincode::{Decode, Encode};
use log::warn;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const CACHE_ROOT: &str = "cache/noteskins";
pub const CACHE_SCHEMA_VERSION: u32 = 1;
static CACHE_TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Encode, Decode, Default, PartialEq, Eq)]
pub struct CompiledLoader {
    pub version: u32,
    pub game: String,
    pub skin: String,
    pub entries: Vec<CompiledLoaderEntry>,
}

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
pub struct CompiledLoaderEntry {
    pub button: String,
    pub element: String,
    pub load_button: String,
    pub load_element: String,
    pub blank: bool,
    pub rotation_z: Option<i32>,
}

#[derive(Debug, Clone, Encode, Decode, Default)]
pub struct CompiledActors {
    pub version: u32,
    pub files: Vec<CompiledActorFile>,
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct CompiledActorFile {
    pub key: String,
    pub decl: noteskin_actor::ItgLuaActorDecl,
}

#[derive(Debug, Clone, Encode, Decode, Default)]
pub struct CompiledNoteskinBundle {
    pub version: u32,
    pub game: String,
    pub skin: String,
    pub source_hash: String,
    pub loader: CompiledLoader,
    pub actors: CompiledActors,
}

impl CompiledLoader {
    #[allow(dead_code)]
    pub fn find(&self, button: &str, element: &str) -> Option<&CompiledLoaderEntry> {
        self.entries.iter().find(|entry| {
            entry.button.eq_ignore_ascii_case(button) && entry.element.eq_ignore_ascii_case(element)
        })
    }
}

impl CompiledActors {
    #[allow(dead_code)]
    pub fn find(&self, key: &str) -> Option<&CompiledActorFile> {
        self.files
            .iter()
            .find(|file| file.key.eq_ignore_ascii_case(key))
    }
}

pub fn compiled_bundle_path(game: &str, skin: &str, source_hash: &str) -> PathBuf {
    Path::new(CACHE_ROOT)
        .join(format!("v{}", CACHE_SCHEMA_VERSION))
        .join(game.trim().to_ascii_lowercase())
        .join(skin.trim().to_ascii_lowercase())
        .join(format!("{source_hash}.bin"))
}

pub fn load_compiled_bundle(path: &Path) -> Option<CompiledNoteskinBundle> {
    let bytes = fs::read(path).ok()?;
    match bincode::decode_from_slice::<CompiledNoteskinBundle, _>(
        &bytes,
        bincode::config::standard(),
    ) {
        Ok((bundle, _)) if bundle.version == CACHE_SCHEMA_VERSION => Some(bundle),
        Ok((bundle, _)) => {
            warn!(
                "unsupported compiled noteskin cache version {} in '{}'",
                bundle.version,
                path.display()
            );
            None
        }
        Err(err) => {
            warn!(
                "failed to decode compiled noteskin cache '{}': {err}",
                path.display()
            );
            None
        }
    }
}

pub fn save_compiled_bundle(path: &Path, bundle: &CompiledNoteskinBundle) -> Result<(), String> {
    let bytes = bincode::encode_to_vec(bundle, bincode::config::standard())
        .map_err(|err| format!("failed to encode compiled noteskin cache: {err}"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create '{}': {err}", parent.display()))?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid cache filename '{}'", path.display()))?;
    let tmp_path = parent.join(format!(
        "{file_name}.{}.{}.tmp",
        std::process::id(),
        CACHE_TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&tmp_path, bytes)
        .map_err(|err| format!("failed to write '{}': {err}", tmp_path.display()))?;
    if let Err(err) = fs::rename(&tmp_path, path) {
        // Parallel test loads can race to publish the same cache bundle.
        // If another writer already installed the final file, keep it and
        // discard our temp file instead of deleting the shared destination.
        if path.is_file() {
            let _ = fs::remove_file(&tmp_path);
            return Ok(());
        }
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("failed to finalize '{}': {err}", path.display()));
    }
    Ok(())
}

#[allow(dead_code)]
pub fn actor_manifest_key(search_dirs: &[PathBuf], path: &Path) -> Option<String> {
    for dir in search_dirs {
        if !path.starts_with(dir) {
            continue;
        }
        return actor_manifest_key_for_dir(dir, path);
    }
    None
}

pub fn actor_manifest_key_for_dir(dir: &Path, path: &Path) -> Option<String> {
    let game = dir.parent()?.file_name()?.to_str()?;
    let skin = dir.file_name()?.to_str()?;
    let file = path.file_name()?.to_str()?;
    Some(format!("{game}/{skin}/{file}").to_ascii_lowercase())
}
