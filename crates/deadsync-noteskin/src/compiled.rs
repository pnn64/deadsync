use bincode::{Decode, Encode};
use log::warn;
use std::cmp::Ordering as CmpOrdering;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::actor as noteskin_actor;

pub const CACHE_SCHEMA_VERSION: u32 = 5;
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
    pub rotation_x: Option<i32>,
    pub rotation_y: Option<i32>,
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
    pub rotation_x: Option<i32>,
    pub rotation_y: Option<i32>,
    pub rotation_z: Option<i32>,
    pub init_command: Option<String>,
}

/// Read-only loader result borrowing either its entry or the fallback arguments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItgLoadRequestRef<'a> {
    pub blank: bool,
    pub load_button: &'a str,
    pub load_element: &'a str,
    pub rotation_x: Option<i32>,
    pub rotation_y: Option<i32>,
    pub rotation_z: Option<i32>,
    pub init_command: Option<&'a str>,
}

impl CompiledLoader {
    #[must_use]
    pub fn find(&self, button: &str, element: &str) -> Option<&CompiledLoaderEntry> {
        let index = self.entries.partition_point(|entry| {
            compiled_loader_entry_cmp(entry, button, element) == CmpOrdering::Less
        });
        self.entries.get(index).filter(|entry| {
            entry.button.eq_ignore_ascii_case(button) && entry.element.eq_ignore_ascii_case(element)
        })
    }

    #[must_use]
    pub fn load_request(&self, button: &str, element: &str) -> ItgLoadRequest {
        self.load_request_ref(button, element).into_owned()
    }

    #[must_use]
    pub fn load_request_ref<'a>(
        &'a self,
        button: &'a str,
        element: &'a str,
    ) -> ItgLoadRequestRef<'a> {
        if let Some(entry) = self.find(button, element) {
            return ItgLoadRequestRef {
                blank: entry.blank,
                load_button: &entry.load_button,
                load_element: &entry.load_element,
                rotation_x: entry.rotation_x,
                rotation_y: entry.rotation_y,
                rotation_z: entry.rotation_z,
                init_command: entry.init_command.as_deref(),
            };
        }
        warn!("compiled noteskin loader is missing '{button} {element}'");
        ItgLoadRequestRef {
            blank: false,
            load_button: button,
            load_element: element,
            rotation_x: None,
            rotation_y: None,
            rotation_z: None,
            init_command: None,
        }
    }
}

#[inline(always)]
fn ascii_case_insensitive_cmp(left: &str, right: &str) -> CmpOrdering {
    left.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
}

#[inline(always)]
fn compiled_loader_entry_cmp(
    entry: &CompiledLoaderEntry,
    button: &str,
    element: &str,
) -> CmpOrdering {
    ascii_case_insensitive_cmp(entry.button.as_str(), button)
        .then_with(|| ascii_case_insensitive_cmp(entry.element.as_str(), element))
}

impl ItgLoadRequest {
    #[must_use]
    pub fn maps_head_to_tap(&self) -> bool {
        !self.blank && self.load_element.eq_ignore_ascii_case("Tap Note")
    }
}

impl ItgLoadRequestRef<'_> {
    #[must_use]
    pub fn maps_head_to_tap(&self) -> bool {
        !self.blank && self.load_element.eq_ignore_ascii_case("Tap Note")
    }

    #[must_use]
    pub fn into_owned(self) -> ItgLoadRequest {
        ItgLoadRequest {
            blank: self.blank,
            load_button: self.load_button.to_owned(),
            load_element: self.load_element.to_owned(),
            rotation_x: self.rotation_x,
            rotation_y: self.rotation_y,
            rotation_z: self.rotation_z,
            init_command: self.init_command.map(str::to_owned),
        }
    }
}

impl CompiledActors {
    #[must_use]
    pub fn find(&self, key: &str) -> Option<&CompiledActorFile> {
        self.files
            .iter()
            .find(|file| file.key.eq_ignore_ascii_case(key))
    }

    #[must_use]
    pub fn decl_for_path(
        &self,
        search_dirs: &[PathBuf],
        path: &Path,
    ) -> Option<noteskin_actor::ItgLuaActorDecl> {
        self.decl_for_path_ref(search_dirs, path).cloned()
    }

    /// Borrow compiled metadata when a reader does not consume the actor graph.
    #[must_use]
    pub fn decl_for_path_ref(
        &self,
        search_dirs: &[PathBuf],
        path: &Path,
    ) -> Option<&noteskin_actor::ItgLuaActorDecl> {
        let key = actor_manifest_key(search_dirs, path)?;
        self.find(&key).map(|file| &file.decl)
    }

    /// The file as loaded for `button`, when its commands depend on the button.
    #[must_use]
    pub fn decl_for_button_path(
        &self,
        search_dirs: &[PathBuf],
        path: &Path,
        button: &str,
    ) -> Option<noteskin_actor::ItgLuaActorDecl> {
        let key = actor_manifest_key(search_dirs, path)?;
        self.find(&button_actor_key(&key, button))
            .or_else(|| self.find(&key))
            .map(|file| file.decl.clone())
    }
}

/// Key of an actor file's variant for one button, stored beside the file's
/// own key when the button changes its commands.
#[must_use]
pub fn button_actor_key(key: &str, button: &str) -> String {
    let mut key = format!("{key}#{button}");
    key.make_ascii_lowercase();
    key
}

#[must_use]
pub fn actor_visit_key(button: &str, element: &str) -> String {
    let mut key = String::with_capacity(button.len() + 1 + element.len());
    key.push_str(button);
    key.push('|');
    key.push_str(element);
    key.make_ascii_lowercase();
    key
}

#[must_use]
pub fn actor_file_visit_key(path: &Path) -> String {
    let mut key = String::with_capacity("file:".len() + path.as_os_str().len());
    write!(&mut key, "file:{}", path.display()).expect("writing to a String cannot fail");
    key.make_ascii_lowercase();
    key
}

#[must_use]
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

#[must_use]
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

#[must_use]
pub fn actor_manifest_key(search_dirs: &[PathBuf], path: &Path) -> Option<String> {
    for dir in search_dirs {
        if !path.starts_with(dir) {
            continue;
        }
        return actor_manifest_key_for_dir(dir, path);
    }
    None
}

#[must_use]
pub fn actor_manifest_key_for_dir(dir: &Path, path: &Path) -> Option<String> {
    let game = dir.parent()?.file_name()?.to_str()?;
    let skin = dir.file_name()?.to_str()?;
    let file = path.file_name()?.to_str()?;
    let mut key = String::with_capacity(game.len() + 1 + skin.len() + 1 + file.len());
    key.push_str(game);
    key.push('/');
    key.push_str(skin);
    key.push('/');
    key.push_str(file);
    key.make_ascii_lowercase();
    Some(key)
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
                rotation_x: None,
                rotation_y: None,
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
            rotation_x: None,
            rotation_y: None,
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
                rotation_x: Some(10),
                rotation_y: Some(20),
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
                rotation_x: Some(10),
                rotation_y: Some(20),
                rotation_z: Some(90),
                init_command: Some("zoom,2".to_string()),
            }
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
                decl: decl,
            }],
        };

        assert!(actors.decl_for_path(&[root], &path).is_some());
    }
}
