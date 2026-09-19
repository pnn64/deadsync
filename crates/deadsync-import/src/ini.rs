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
    let mut current_section = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let section_name = line[1..line.len() - 1].trim();
            if !sections.contains_key(section_name) {
                sections.insert(section_name.to_owned(), HashMap::with_hasher(S::default()));
            }
            current_section = sections.get_mut(section_name);
            continue;
        }

        if let Some(eq_idx) = line.find('=') {
            let (key_raw, value_raw) = line.split_at(eq_idx);
            let key = key_raw.trim().to_owned();
            let value = value_raw[1..].trim().to_owned();
            if current_section.is_none() {
                current_section = Some(sections.entry(String::new()).or_default());
            }
            current_section
                .as_mut()
                .expect("current INI section must exist")
                .insert(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_keeps_root_empty_sections_and_repeated_headers() {
        let mut ini = SimpleIni::new();
        ini.load_from_str("# no values\n; comment\nignored\n");
        assert!(ini.sections.is_empty());

        ini.load_from_str(
            "root=before\n[Empty]\n[Alpha]\nkey=first\n[Beta]\nkey=other\n\
             [ Alpha ]\nkey=last\n[]=root value\n[]\nroot=after\n=empty key\n[alpha]\nkey=lower\n",
        );
        assert!(ini.get_section("Empty").unwrap().is_empty());
        assert_eq!(ini.get("Alpha", "key"), Some("last"));
        assert_eq!(ini.get("Alpha", "[]"), Some("root value"));
        assert_eq!(ini.get("Beta", "key"), Some("other"));
        assert_eq!(ini.get("alpha", "key"), Some("lower"));
        assert_eq!(ini.get("", "root"), Some("after"));
        assert_eq!(ini.get("", ""), Some("empty key"));

        ini.load_from_str("[Empty]\n");
        assert_eq!(ini.sections.len(), 1);
        assert!(ini.get_section("Empty").unwrap().is_empty());
    }

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
