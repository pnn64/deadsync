use rustc_hash::FxHashMap;
use std::path::Path;

pub type IniSection = FxHashMap<String, String>;
pub type IniSections = FxHashMap<String, IniSection>;

#[derive(Debug, Default)]
pub struct SimpleIni {
    sections: IniSections,
}

impl SimpleIni {
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

        let mut parsed_sections = Vec::<(String, Vec<(String, String)>)>::new();
        let mut current_section = None;

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
                let name = &line[1..line.len() - 1];
                parsed_sections.push((name.trim().to_string(), Vec::new()));
                current_section = Some(parsed_sections.len() - 1);
                continue;
            }

            if let Some(eq_idx) = line.find('=') {
                let (key_raw, value_raw) = line.split_at(eq_idx);
                let key = key_raw.trim();
                if key.is_empty() {
                    continue;
                }
                let value = value_raw[1..].trim().to_string();
                let section_index = *current_section.get_or_insert_with(|| {
                    parsed_sections.push((String::new(), Vec::new()));
                    parsed_sections.len() - 1
                });
                parsed_sections[section_index]
                    .1
                    .push((key.to_string(), value));
            }
        }

        self.sections.reserve(parsed_sections.len());
        for (section, values) in parsed_sections {
            let properties = self.sections.entry(section).or_default();
            properties.reserve(values.len());
            properties.extend(values);
        }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .get(section)
            .and_then(|properties| properties.get(key))
            .map(String::as_str)
    }

    pub fn get_section(&self, section: &str) -> Option<&IniSection> {
        self.sections.get(section)
    }

    pub const fn sections(&self) -> &IniSections {
        &self.sections
    }

    pub fn into_sections(self) -> IniSections {
        self.sections
    }
}

/// Unescape INI string escape sequences used by localized string values.
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
