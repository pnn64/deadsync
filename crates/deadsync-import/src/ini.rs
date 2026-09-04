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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_handles_root_values_sections_comments_and_overrides() {
        let content = "root = before\n[ Alpha ]\nKey = first\nOther=two\n[Beta]\nFlag=true\n[Alpha]\nKey=last\n";
        let mut ini = SimpleIni::new();
        ini.load_from_str(content);

        assert_eq!(ini.get("Alpha", "Key"), Some("last"));
        assert_eq!(ini.get("Alpha", "Other"), Some("two"));
        assert_eq!(ini.get("Beta", "Flag"), Some("true"));
        assert_eq!(ini.get("", "root"), Some("before"));
        assert_eq!(ini.get("missing", "Key"), None);
    }
}
