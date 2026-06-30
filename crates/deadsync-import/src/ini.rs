use std::collections::HashMap;
use std::path::Path;

pub struct SimpleIni {
    sections: HashMap<String, HashMap<String, String>>,
}

impl SimpleIni {
    pub fn new() -> Self {
        Self {
            sections: HashMap::new(),
        }
    }

    pub fn load(&mut self, path: &Path) -> std::io::Result<()> {
        let content = std::fs::read_to_string(path)?;
        self.load_from_str(&content);
        Ok(())
    }

    pub fn load_from_str(&mut self, content: &str) {
        self.sections.clear();
        let mut current_section: Option<String> = None;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                let name = &line[1..line.len() - 1];
                let section = name.trim().to_string();
                current_section = Some(section.clone());
                self.sections.entry(section).or_default();
                continue;
            }

            if let Some(eq_idx) = line.find('=') {
                let (key_raw, value_raw) = line.split_at(eq_idx);
                let key = key_raw.trim().to_string();
                let value = value_raw[1..].trim().to_string();
                let section = current_section.clone().unwrap_or_default();
                self.sections.entry(section).or_default().insert(key, value);
            }
        }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        self.sections.get(section).and_then(|s| s.get(key)).cloned()
    }

    pub fn get_section(&self, section: &str) -> Option<&HashMap<String, String>> {
        self.sections.get(section)
    }
}
