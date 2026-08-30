use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::Path;

use rustc_hash::FxBuildHasher;

type IniSection<S> = HashMap<String, String, S>;
type IniSections<S> = HashMap<String, IniSection<S>, S>;

#[derive(Default)]
pub struct SimpleIni {
    sections: IniSections<FxBuildHasher>,
}

impl SimpleIni {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(&mut self, path: &Path) -> std::io::Result<()> {
        let content = std::fs::read_to_string(path)?;
        self.load_from_str(&content);
        Ok(())
    }

    pub fn load_from_str(&mut self, content: &str) {
        self.sections.clear();
        load_sections_borrowed(&mut self.sections, content);
    }

    pub fn get<'a>(&'a self, section: &str, key: &str) -> Option<&'a str> {
        self.sections
            .get(section)
            .and_then(|values| values.get(key))
            .map(String::as_str)
    }

    pub fn get_section(&self, section: &str) -> Option<&IniSection<FxBuildHasher>> {
        self.sections.get(section)
    }
}

fn load_sections_borrowed<S: BuildHasher + Default>(sections: &mut IniSections<S>, content: &str) {
    let mut current_section = "";

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            current_section = line[1..line.len() - 1].trim();
            if !sections.contains_key(current_section) {
                sections.insert(
                    current_section.to_owned(),
                    HashMap::with_hasher(S::default()),
                );
            }
            continue;
        }

        if let Some(eq_idx) = line.find('=') {
            let (key_raw, value_raw) = line.split_at(eq_idx);
            let key = key_raw.trim().to_owned();
            let value = value_raw[1..].trim().to_owned();
            if let Some(section) = sections.get_mut(current_section) {
                section.insert(key, value);
            } else {
                let mut section = HashMap::with_hasher(S::default());
                section.insert(key, value);
                sections.insert(current_section.to_owned(), section);
            }
        }
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn load_sections_cloned<S: BuildHasher + Default>(sections: &mut IniSections<S>, content: &str) {
    let mut current_section: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let section = line[1..line.len() - 1].trim().to_owned();
            current_section = Some(section.clone());
            sections
                .entry(section)
                .or_insert_with(|| HashMap::with_hasher(S::default()));
            continue;
        }

        if let Some(eq_idx) = line.find('=') {
            let (key_raw, value_raw) = line.split_at(eq_idx);
            let section = current_section.clone().unwrap_or_default();
            sections
                .entry(section)
                .or_insert_with(|| HashMap::with_hasher(S::default()))
                .insert(key_raw.trim().to_owned(), value_raw[1..].trim().to_owned());
        }
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub mod bench_support {
    use super::*;

    fn text_checksum(text: &str) -> u64 {
        text.bytes().fold(text.len() as u64, |checksum, byte| {
            checksum.rotate_left(5).wrapping_add(u64::from(byte))
        })
    }

    fn sections_checksum<S: BuildHasher>(sections: &IniSections<S>) -> u64 {
        sections.iter().fold(0u64, |checksum, (section, values)| {
            values.iter().fold(
                checksum.wrapping_add(text_checksum(section)),
                |checksum, (key, value)| {
                    checksum
                        .wrapping_add(text_checksum(key).rotate_left(11))
                        .wrapping_add(text_checksum(value).rotate_left(23))
                },
            )
        })
    }

    pub fn parse_cloned_sections(content: &str) -> u64 {
        let mut sections =
            IniSections::with_hasher(std::collections::hash_map::RandomState::default());
        load_sections_cloned(&mut sections, content);
        sections_checksum(&sections)
    }

    pub fn parse_borrowed_sections(content: &str) -> u64 {
        let mut sections =
            IniSections::with_hasher(std::collections::hash_map::RandomState::default());
        load_sections_borrowed(&mut sections, content);
        sections_checksum(&sections)
    }

    pub fn parse_fast_hash(content: &str) -> u64 {
        let mut sections = IniSections::with_hasher(FxBuildHasher);
        load_sections_borrowed(&mut sections, content);
        sections_checksum(&sections)
    }

    pub fn load(content: &str) -> SimpleIni {
        let mut ini = SimpleIni::new();
        ini.load_from_str(content);
        ini
    }

    pub fn lookup_cloned(ini: &SimpleIni, queries: &[(String, String)]) -> u64 {
        queries.iter().fold(0u64, |checksum, (section, key)| {
            let value = ini.get(section, key).map(str::to_owned).unwrap_or_default();
            checksum.wrapping_add(text_checksum(&value))
        })
    }

    pub fn lookup_borrowed(ini: &SimpleIni, queries: &[(String, String)]) -> u64 {
        queries.iter().fold(0u64, |checksum, (section, key)| {
            checksum.wrapping_add(text_checksum(ini.get(section, key).unwrap_or_default()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_parser_and_lookups_match_cloned_reference() {
        let content = "root = before\n[ Alpha ]\nKey = first\nOther=two\n[Beta]\nFlag=true\n[Alpha]\nKey=last\n";
        let mut reference =
            IniSections::with_hasher(std::collections::hash_map::RandomState::default());
        load_sections_cloned(&mut reference, content);

        let mut ini = SimpleIni::new();
        ini.load_from_str(content);
        for (section, values) in &reference {
            for (key, value) in values {
                assert_eq!(ini.get(section, key), Some(value.as_str()));
            }
        }
        assert_eq!(ini.get("Alpha", "Key"), Some("last"));
        assert_eq!(ini.get("", "root"), Some("before"));
        assert_eq!(ini.get("missing", "Key"), None);
    }
}
