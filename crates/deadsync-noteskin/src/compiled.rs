use bincode::{Decode, Encode};
use log::warn;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::actor as noteskin_actor;

pub const CACHE_SCHEMA_VERSION: u32 = 3;
pub const ACTOR_RECURSION_MAX_DEPTH: usize = 24;
pub const ACTOR_FILE_RECURSION_MAX_DEPTH: usize = 48;
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
    pub init_command: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItgLoadRequest {
    pub blank: bool,
    pub load_button: String,
    pub load_element: String,
    pub rotation_z: Option<i32>,
    pub init_command: Option<String>,
}

impl CompiledLoader {
    pub fn find(&self, button: &str, element: &str) -> Option<&CompiledLoaderEntry> {
        self.entries.iter().find(|entry| {
            entry.button.eq_ignore_ascii_case(button) && entry.element.eq_ignore_ascii_case(element)
        })
    }

    pub fn load_request(&self, button: &str, element: &str) -> ItgLoadRequest {
        if let Some(entry) = self.find(button, element) {
            return ItgLoadRequest {
                blank: entry.blank,
                load_button: entry.load_button.clone(),
                load_element: entry.load_element.clone(),
                rotation_z: entry.rotation_z,
                init_command: entry.init_command.clone(),
            };
        }
        warn!("compiled noteskin loader is missing '{button} {element}'");
        ItgLoadRequest {
            blank: false,
            load_button: button.to_string(),
            load_element: element.to_string(),
            rotation_z: None,
            init_command: None,
        }
    }
}

impl ItgLoadRequest {
    pub fn maps_head_to_tap(&self) -> bool {
        !self.blank && self.load_element.eq_ignore_ascii_case("Tap Note")
    }
}

impl CompiledActors {
    pub fn find(&self, key: &str) -> Option<&CompiledActorFile> {
        self.files
            .iter()
            .find(|file| file.key.eq_ignore_ascii_case(key))
    }

    pub fn decl_for_path(
        &self,
        search_dirs: &[PathBuf],
        path: &Path,
    ) -> Option<noteskin_actor::ItgLuaActorDecl> {
        let key = actor_manifest_key(search_dirs, path)?;
        self.find(&key).cloned().map(|file| file.decl)
    }
}

pub fn actor_visit_key(button: &str, element: &str) -> String {
    format!(
        "{}|{}",
        button.to_ascii_lowercase(),
        element.to_ascii_lowercase()
    )
}

pub fn actor_file_visit_key(path: &Path) -> String {
    format!("file:{}", path.display().to_string().to_ascii_lowercase())
}

pub fn compiled_bundle_path(
    cache_dir: &Path,
    game: &str,
    skin: &str,
    source_hash: &str,
) -> PathBuf {
    cache_dir
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
        if path.is_file() {
            let _ = fs::remove_file(&tmp_path);
            return Ok(());
        }
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("failed to finalize '{}': {err}", path.display()));
    }
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_request_falls_back_to_requested_actor() {
        let loader = CompiledLoader::default();

        assert_eq!(
            loader.load_request("Down", "Receptor"),
            ItgLoadRequest {
                blank: false,
                load_button: "Down".to_string(),
                load_element: "Receptor".to_string(),
                rotation_z: None,
                init_command: None,
            }
        );
    }

    #[test]
    fn load_request_detects_head_to_tap_mapping() {
        let request = ItgLoadRequest {
            blank: false,
            load_button: "Down".to_string(),
            load_element: "Tap Note".to_string(),
            rotation_z: None,
            init_command: None,
        };

        assert!(request.maps_head_to_tap());
    }

    #[test]
    fn load_request_preserves_compiled_entry_data() {
        let loader = CompiledLoader {
            version: CACHE_SCHEMA_VERSION,
            game: "dance".to_string(),
            skin: "default".to_string(),
            entries: vec![CompiledLoaderEntry {
                button: "Down".to_string(),
                element: "Hold Explosion".to_string(),
                load_button: "Left".to_string(),
                load_element: "Roll Explosion".to_string(),
                blank: true,
                rotation_z: Some(90),
                init_command: Some("zoom,2".to_string()),
            }],
        };

        assert_eq!(
            loader.load_request("down", "hold explosion"),
            ItgLoadRequest {
                blank: true,
                load_button: "Left".to_string(),
                load_element: "Roll Explosion".to_string(),
                rotation_z: Some(90),
                init_command: Some("zoom,2".to_string()),
            }
        );
    }

    #[test]
    fn actor_recursion_keys_are_case_normalized() {
        assert_eq!(actor_visit_key("Down", "Tap Note"), "down|tap note");
        assert_eq!(
            actor_file_visit_key(Path::new("Dance/Default/Down Receptor.lua")),
            "file:dance/default/down receptor.lua"
        );
    }

    #[test]
    fn compiled_actors_find_decl_for_noteskin_path() {
        let root = PathBuf::from("assets/noteskins/dance/default");
        let path = root.join("Down Receptor.lua");
        let decl = noteskin_actor::ItgLuaActorDecl::default();
        let actors = CompiledActors {
            version: CACHE_SCHEMA_VERSION,
            files: vec![CompiledActorFile {
                key: "dance/default/down receptor.lua".to_string(),
                decl: decl.clone(),
            }],
        };

        assert!(actors.decl_for_path(&[root], &path).is_some());
    }
}
