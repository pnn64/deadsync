// Frozen from fd44d31f1 (0.5.1145); the language conversion excludes file I/O.
#![allow(dead_code)]
use super::LanguageMap;
use rustc_hash::FxHashMap;
use std::path::Path;
use std::sync::Arc;

pub type IniSection = FxHashMap<String, String>;
pub type IniSections = FxHashMap<String, IniSection>;

#[derive(Debug, Default)]
pub struct SimpleIni {
    sections: IniSections,
}

impl SimpleIni {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load<P: AsRef<Path>>(&mut self, path: P) -> Result<(), std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        self.load_str(&content);
        Ok(())
    }

    pub fn load_str(&mut self, content: &str) {
        self.sections.clear();

        let mut entries = ini_entries(content).peekable();
        let mut values = Vec::new();
        while let Some(entry) = entries.next() {
            let (name, first) = match entry {
                IniEntry::Section(name) => (
                    name,
                    entries.next_if(|entry| matches!(entry, IniEntry::Property(..))),
                ),
                IniEntry::Property(..) => ("", Some(entry)),
            };
            while let Some(IniEntry::Property(key, value)) =
                entries.next_if(|entry| matches!(entry, IniEntry::Property(..)))
            {
                values.push((key, value));
            }
            // Stage borrowed properties for one section, reserving its final map
            // once. Reuse this buffer across sections instead of retaining copies.
            let properties = self.sections.entry(name.to_owned()).or_default();
            properties.reserve(values.len() + usize::from(first.is_some()));
            if let Some(IniEntry::Property(key, value)) = first {
                properties.insert(key.to_owned(), value.to_owned());
            }
            for &(key, value) in &values {
                properties.insert(key.to_owned(), value.to_owned());
            }
            values.clear();
        }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(section)
            .and_then(|properties| properties.get(key))
            .map(String::as_str)
    }

    #[must_use]
    pub fn get_section(&self, section: &str) -> Option<&IniSection> {
        self.sections.get(section)
    }

    #[must_use]
    pub const fn sections(&self) -> &IniSections {
        &self.sections
    }

    #[must_use]
    pub fn into_sections(self) -> IniSections {
        self.sections
    }
}

enum IniEntry<'a> {
    Section(&'a str),
    Property(&'a str, &'a str),
}

fn ini_entries(content: &str) -> impl Iterator<Item = IniEntry<'_>> {
    content.lines().filter_map(|raw| {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            return None;
        }
        if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
            return Some(IniEntry::Section(line[1..line.len() - 1].trim()));
        }
        let (key, value) = line.split_once('=')?;
        let key = key.trim();
        (!key.is_empty()).then(|| IniEntry::Property(key, value.trim()))
    })
}

/// Read a single INI value without allocating a map or copying unrelated text.
/// Repeated sections merge and the last matching key wins, as in `SimpleIni`.
#[must_use]
pub fn ini_value<'a>(content: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut selected = section.is_empty();
    let mut value = None;
    for entry in ini_entries(content) {
        match entry {
            IniEntry::Section(name) => selected = name == section,
            IniEntry::Property(name, text) if selected && name == key => value = Some(text),
            IniEntry::Property(..) => {}
        }
    }
    value
}

/// Unescape INI string escape sequences used by localized string values.
#[must_use]
pub fn unescape_ini_value(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn language_map(ini: SimpleIni) -> LanguageMap {
    let mut sections = LanguageMap::default();
    for (section, props) in ini.into_sections() {
        let entries = sections.entry(section.into_boxed_str()).or_default();
        for (key, value) in props {
            if value.trim() == "@skip" {
                continue;
            }
            let value = if value.contains('\\') {
                Arc::from(unescape_ini_value(&value))
            } else {
                Arc::from(value)
            };
            entries.insert(key.into_boxed_str(), value);
        }
    }
    sections
}
