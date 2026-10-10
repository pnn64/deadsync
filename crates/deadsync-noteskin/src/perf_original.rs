// Frozen from 1ce6f146b28b0e7862f6cbf6ab7f6c37ca9e8267; test-only baseline.
use super::*;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OriginalCacheKey {
    num_cols: usize,
    num_players: usize,
    skin: String,
}

pub struct OriginalRuntimeCache<T> {
    entries: Mutex<HashMap<OriginalCacheKey, Weak<T>>>,
}

impl<T> Default for OriginalRuntimeCache<T> {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl<T> OriginalRuntimeCache<T> {
    /// Borrow an existing full runtime without constructing one on a cache miss.
    pub fn get(&self, style: &Style, skin: &str) -> Option<Arc<T>> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&original_cache_key(style, skin))
            .and_then(Weak::upgrade)
    }

    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    pub fn get_or_load<F>(&self, style: &Style, skin: &str, load: F) -> Result<Arc<T>, String>
    where
        F: FnOnce() -> Result<T, String>,
    {
        let key = original_cache_key(style, skin);
        if let Some(cached) = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .and_then(Weak::upgrade)
        {
            return Ok(cached);
        }

        let loaded = Arc::new(load()?);
        let mut guard = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(cached) = guard.get(&key).and_then(Weak::upgrade) {
            return Ok(cached);
        }
        guard.insert(key, Arc::downgrade(&loaded));
        Ok(loaded)
    }
}

pub fn original_cache_key(style: &Style, skin: &str) -> OriginalCacheKey {
    OriginalCacheKey {
        num_cols: style.num_cols,
        num_players: style.num_players,
        skin: normalized_skin_name(skin),
    }
}

#[derive(Debug, Clone, Default)]
pub struct OriginalIniData {
    pub(super) sections: BorrowMap<IniKey, BorrowMap<IniKey, String>>,
}

impl OriginalIniData {
    pub fn parse_file(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)
            .map_err(|e| format!("failed to read ini '{}': {e}", path.display()))?;
        Ok(Self::parse(&content))
    }

    pub fn parse(content: &str) -> Self {
        let mut out = Self::default();
        let mut section = IniKey::new("");
        let mut section_present = false;

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') && line.len() > 2 {
                section = IniKey::new(line[1..line.len() - 1].trim());
                out.sections.entry(section.clone()).or_default();
                section_present = true;
                continue;
            }
            let Some((key_raw, value_raw)) = line.split_once('=') else {
                continue;
            };
            let key = key_raw.trim();
            if key.is_empty() {
                continue;
            }
            if !section_present {
                out.sections.entry(section.clone()).or_default();
                section_present = true;
            }
            let value = value_raw.trim().to_string();
            out.sections
                .get_mut(&IniKeyRef(&section.0))
                .expect("current INI section must exist")
                .insert(IniKey::new(key), value);
        }

        out
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(&IniKeyRef(section))
            .and_then(|values| values.get(&IniKeyRef(key)))
            .map(String::as_str)
    }

    pub(crate) fn set(&mut self, section: &str, key: &str, value: &str) {
        self.sections
            .entry(IniKey::new(section))
            .or_default()
            .insert(IniKey::new(key), value.to_owned());
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
