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

#[cfg(any(test, feature = "bench-support"))]
fn load_str_legacy(
    content: &str,
) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
    use std::collections::HashMap;

    let mut sections = HashMap::<String, HashMap<String, String>>::new();
    let mut current_section: Option<String> = None;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
            let name = &line[1..line.len() - 1];
            let section = name.trim().to_string();
            current_section = Some(section.clone());
            sections.entry(section).or_default();
            continue;
        }

        if let Some(eq_idx) = line.find('=') {
            let (key_raw, value_raw) = line.split_at(eq_idx);
            let key = key_raw.trim();
            if key.is_empty() {
                continue;
            }
            let value = value_raw[1..].trim().to_string();
            let section = current_section.clone().unwrap_or_default();
            sections
                .entry(section)
                .or_default()
                .insert(key.to_string(), value);
        }
    }

    sections
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn simple_ini_workload_for_bench(content: &str, lookups: &[(String, String)]) -> usize {
    let mut ini = SimpleIni::new();
    ini.load_str(content);
    let mut checksum = ini.sections().len();
    for (section, key) in lookups {
        checksum = checksum.wrapping_add(
            ini.get(section, key)
                .map_or(0, |value| value.len().wrapping_add(1)),
        );
    }
    checksum
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn simple_ini_workload_legacy_for_bench(content: &str, lookups: &[(String, String)]) -> usize {
    let ini = load_str_legacy(content);
    let mut checksum = ini.len();
    for (section, key) in lookups {
        let value = ini
            .get(section)
            .and_then(|properties| properties.get(key))
            .cloned();
        checksum = checksum.wrapping_add(value.map_or(0, |value| value.len().wrapping_add(1)));
    }
    checksum
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
    use std::collections::BTreeMap;

    #[test]
    fn optimized_parser_matches_legacy_behavior_exactly() {
        let content = concat!(
            "default = before section\r\n",
            " = ignored\r\n",
            "; comment\r\n",
            "# another comment\r\n",
            "[ First ]\r\n",
            "alpha = one\r\n",
            "duplicate = old\r\n",
            "[Empty]\r\n",
            "[First]\r\n",
            "duplicate = new\r\n",
            "spaced =   trimmed value   \r\n",
            "[first]\r\n",
            "case = distinct\r\n",
            "[]\r\n",
            "default = explicit empty section\r\n",
            "not a property\r\n",
        );
        let legacy = load_str_legacy(content);
        let mut optimized = SimpleIni::new();
        optimized.load_str(content);

        let legacy = legacy
            .iter()
            .map(|(section, properties)| {
                (
                    section.clone(),
                    properties
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect::<BTreeMap<_, _>>(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let optimized = optimized
            .sections()
            .iter()
            .map(|(section, properties)| {
                (
                    section.clone(),
                    properties
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect::<BTreeMap<_, _>>(),
                )
            })
            .collect::<BTreeMap<_, _>>();

        assert_eq!(optimized, legacy);
    }

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
