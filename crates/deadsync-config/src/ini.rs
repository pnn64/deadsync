use rustc_hash::FxHashMap;
use std::path::Path;

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

        load_ini_sections(&mut self.sections, content);
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

/// INI sections whose names, keys, and values borrow the input text.
pub type BorrowedIniSections<'a> = FxHashMap<&'a str, FxHashMap<&'a str, &'a str>>;

/// Parse complete INI data while allocating only maps and reusable section storage.
/// Duplicate sections and keys follow the same rules as `SimpleIni`.
#[must_use]
pub fn borrowed_ini_sections(content: &str) -> BorrowedIniSections<'_> {
    let mut sections = FxHashMap::default();
    load_ini_sections(&mut sections, content);
    sections
}

fn load_ini_sections<'a, K, V>(sections: &mut FxHashMap<K, FxHashMap<K, V>>, content: &'a str)
where
    K: From<&'a str> + Eq + std::hash::Hash,
    V: From<&'a str>,
{
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
        let properties = sections.entry(K::from(name)).or_default();
        properties.reserve(values.len() + usize::from(first.is_some()));
        if let Some(IniEntry::Property(key, value)) = first {
            properties.insert(K::from(key), V::from(value));
        }
        for &(key, value) in &values {
            properties.insert(K::from(key), V::from(value));
        }
        values.clear();
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
    let mut remaining = raw;
    // Copy long unchanged spans in bulk. Small or escape-dense tails use the
    // scalar decoder to avoid paying for a substring search for every escape.
    while remaining.len() >= 32 {
        let Some(slash) = remaining.find('\\') else {
            out.push_str(remaining);
            return out;
        };
        if slash < 32 {
            break;
        }
        out.push_str(&remaining[..slash]);
        let escaped = &remaining[slash + 1..];
        match escaped.as_bytes().first() {
            Some(b'n') => {
                out.push('\n');
                remaining = &escaped[1..];
            }
            Some(b't') => {
                out.push('\t');
                remaining = &escaped[1..];
            }
            Some(b'\\') => {
                out.push('\\');
                remaining = &escaped[1..];
            }
            _ => {
                out.push('\\');
                remaining = escaped;
            }
        }
    }
    let mut chars = remaining.chars();
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unescape_ini_value_handles_supported_escapes() {
        assert_eq!(unescape_ini_value(r"line\nnext"), "line\nnext");
        assert_eq!(unescape_ini_value(r"col\tvalue"), "col\tvalue");
        assert_eq!(unescape_ini_value(r"path\\file"), r"path\file");
    }

    #[test]
    fn unescape_ini_value_preserves_unknown_and_trailing_slashes() {
        assert_eq!(unescape_ini_value(r"\q"), r"\q");
        assert_eq!(unescape_ini_value(r"tail\"), r"tail\");
    }
}
