// Frozen from d53d34a2e (0.5.1144); profile visibility widened for comparisons.
#![allow(dead_code)]
use deadsync_config::ini::{IniSection, IniSections};
use std::path::Path;
use std::{collections::HashMap, fs};

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

#[derive(Debug, Default)]
pub struct ProfileIni {
    sections: HashMap<String, HashMap<String, String>>,
}

impl ProfileIni {
    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        let content = fs::read_to_string(path)?;
        Ok(Self::parse(content.as_str()))
    }

    pub fn parse(content: &str) -> Self {
        let mut ini = Self::default();
        let mut current_section: Option<String> = None;

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
                let section = line[1..line.len() - 1].trim().to_string();
                current_section = Some(section.clone());
                ini.sections.entry(section).or_default();
                continue;
            }

            let Some(eq_idx) = line.find('=') else {
                continue;
            };
            let key = line[..eq_idx].trim();
            if key.is_empty() {
                continue;
            }
            let value = line[eq_idx + 1..].trim().to_string();
            let section = current_section.clone().unwrap_or_default();
            ini.sections
                .entry(section)
                .or_default()
                .insert(key.to_string(), value);
        }

        ini
    }

    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        self.sections
            .get(section)
            .and_then(|section| section.get(key))
            .cloned()
    }

    pub fn section_has_any(&self, section: &str) -> bool {
        self.sections.get(section).is_some_and(|s| !s.is_empty())
    }
}
