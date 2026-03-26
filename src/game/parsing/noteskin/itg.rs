#![allow(dead_code)]

use log::warn;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const MAX_FALLBACK_DEPTH: usize = 20;
const MAX_REDIR_DEPTH: usize = 100;

static CHILD_DIR_CACHE: OnceLock<Mutex<HashMap<(String, String), Option<PathBuf>>>> =
    OnceLock::new();
static FILE_PREFIX_CACHE: OnceLock<Mutex<HashMap<(String, String), Option<PathBuf>>>> =
    OnceLock::new();

#[derive(Debug, Clone, Default)]
pub struct IniData {
    sections: HashMap<String, HashMap<String, String>>,
}

impl IniData {
    pub fn parse_file(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)
            .map_err(|e| format!("failed to read ini '{}': {e}", path.display()))?;
        let mut out = Self::default();
        let mut section = String::new();

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') && line.len() > 2 {
                section = line[1..line.len() - 1].trim().to_ascii_lowercase();
                out.sections.entry(section.clone()).or_default();
                continue;
            }
            let Some((key_raw, value_raw)) = line.split_once('=') else {
                continue;
            };
            let key = key_raw.trim();
            if key.is_empty() {
                continue;
            }
            let value = value_raw.trim().to_string();
            out.sections
                .entry(section.clone())
                .or_default()
                .insert(key.to_ascii_lowercase(), value);
        }

        Ok(out)
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(&section.to_ascii_lowercase())
            .and_then(|s| s.get(&key.to_ascii_lowercase()))
            .map(String::as_str)
    }

    pub fn merge_missing_from(&mut self, other: &Self) {
        for (section, values) in &other.sections {
            let dst = self.sections.entry(section.clone()).or_default();
            for (key, value) in values {
                dst.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct NoteskinData {
    pub name: String,
    pub metrics: IniData,
    pub search_dirs: Vec<PathBuf>,
}

impl NoteskinData {
    pub fn get_metric(&self, button: &str, value: &str) -> Option<&str> {
        self.metrics
            .get(button, value)
            .or_else(|| self.metrics.get("notedisplay", value))
    }

    pub fn resolve_path(&self, button: &str, element: &str) -> Option<PathBuf> {
        let mut path = self.resolve_path_once(button, element)?;

        for _ in 0..MAX_REDIR_DEPTH {
            if !is_redir(&path) {
                return Some(path);
            }

            let target = fs::read_to_string(&path).ok()?.trim().to_string();
            if target.is_empty() {
                warn!("noteskin redirect '{}' was empty", path.display());
                return None;
            }

            let Some(next) = self.resolve_file_from_search_dirs(&target) else {
                warn!(
                    "noteskin redirect '{}' -> '{}' did not resolve",
                    path.display(),
                    target
                );
                return None;
            };
            path = next;
        }

        warn!(
            "noteskin redirect depth exceeded while resolving '{} {}'",
            button, element
        );
        None
    }

    fn resolve_path_once(&self, button: &str, element: &str) -> Option<PathBuf> {
        let pref = if button.is_empty() {
            element.to_string()
        } else if element.is_empty() {
            button.to_string()
        } else {
            format!("{button} {element}")
        };

        if let Some(path) = self.resolve_file_from_search_dirs(&pref) {
            return Some(path);
        }

        if button.is_empty() {
            return None;
        }

        self.resolve_file_from_search_dirs(&format!("Fallback {element}"))
    }

    fn resolve_file_from_search_dirs(&self, prefix: &str) -> Option<PathBuf> {
        for dir in &self.search_dirs {
            if let Some(path) = find_file_with_prefix(dir, prefix) {
                return Some(path);
            }
        }
        None
    }
}

pub fn load_noteskin_data(root: &Path, game: &str, skin: &str) -> Result<NoteskinData, String> {
    let mut metrics = IniData::default();
    let mut search_dirs = Vec::new();

    let mut current = skin.trim().to_ascii_lowercase();
    if current.is_empty() {
        return Err("noteskin name was empty".to_string());
    }

    let mut loaded_default = false;
    let mut loaded_common = false;
    let mut seen = HashSet::new();

    for _ in 0..MAX_FALLBACK_DEPTH {
        if !seen.insert(current.clone()) {
            return Err(format!(
                "circular noteskin fallback detected while loading '{skin}' (stuck on '{}')",
                current
            ));
        }

        let Some(dir) = resolve_skin_dir(root, game, &current) else {
            return Err(format!(
                "noteskin '{}' not found under '{}/{}' or '{}/common'",
                current,
                root.display(),
                game,
                root.display()
            ));
        };

        let ini = IniData::parse_file(&dir.join("metrics.ini"))?;
        metrics.merge_missing_from(&ini);
        search_dirs.push(dir);

        if current.eq_ignore_ascii_case("default") {
            loaded_default = true;
        }
        if current.eq_ignore_ascii_case("common") {
            loaded_common = true;
        }

        let next = match ini.get("global", "fallbacknoteskin") {
            Some(value) if !value.trim().is_empty() => Some(value.trim().to_ascii_lowercase()),
            _ if !loaded_default => Some("default".to_string()),
            _ if !loaded_common => Some("common".to_string()),
            _ => None,
        };

        let Some(next_skin) = next else {
            return Ok(NoteskinData {
                name: skin.to_ascii_lowercase(),
                metrics,
                search_dirs,
            });
        };
        if next_skin == current {
            return Ok(NoteskinData {
                name: skin.to_ascii_lowercase(),
                metrics,
                search_dirs,
            });
        }
        if seen.contains(&next_skin) {
            return Ok(NoteskinData {
                name: skin.to_ascii_lowercase(),
                metrics,
                search_dirs,
            });
        }
        current = next_skin;
    }

    Err(format!(
        "noteskin fallback depth exceeded while loading '{skin}'"
    ))
}

fn resolve_skin_dir(root: &Path, game: &str, skin: &str) -> Option<PathBuf> {
    find_child_dir_case_insensitive(&root.join(game), skin)
        .or_else(|| find_child_dir_case_insensitive(&root.join("common"), skin))
}

fn find_child_dir_case_insensitive(parent: &Path, name: &str) -> Option<PathBuf> {
    let want = name.to_ascii_lowercase();
    let key = (parent.to_string_lossy().to_ascii_lowercase(), want.clone());
    let cache = CHILD_DIR_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&key)
        .cloned()
    {
        return cached;
    }
    let entries = fs::read_dir(parent).ok()?;
    let mut found = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let matches = entry
            .file_name()
            .to_str()
            .is_some_and(|n| n.eq_ignore_ascii_case(&want));
        if matches {
            found = Some(path);
            break;
        }
    }
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key, found.clone());
    found
}

fn find_file_with_prefix(dir: &Path, prefix: &str) -> Option<PathBuf> {
    let want = prefix.to_ascii_lowercase();
    let key = (dir.to_string_lossy().to_ascii_lowercase(), want.clone());
    let cache = FILE_PREFIX_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&key)
        .cloned()
    {
        return cached;
    }
    let entries = fs::read_dir(dir).ok()?;
    let mut matches: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|name| name.to_ascii_lowercase().starts_with(want.as_str()))
        })
        .collect();

    if matches.is_empty() {
        cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key, None);
        return None;
    }

    matches.sort_by(|a, b| {
        let a_name = a
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let b_name = b
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        a_name.cmp(&b_name)
    });

    if matches.len() > 1 {
        warn!(
            "multiple noteskin files matched prefix '{}' in '{}'; using '{}', ignoring {} others",
            prefix,
            dir.display(),
            matches[0].display(),
            matches.len() - 1
        );
    }
    let chosen = matches.into_iter().next();
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key, chosen.clone());
    chosen
}

fn is_redir(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("redir"))
}
