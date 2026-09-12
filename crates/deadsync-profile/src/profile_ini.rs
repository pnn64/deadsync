use deadsync_config::ini::SimpleIni;
use std::path::Path;

#[derive(Debug, Default)]
pub(super) struct ProfileIni {
    sections: SimpleIni,
}

impl ProfileIni {
    pub(super) fn load(path: &Path) -> Result<Self, std::io::Error> {
        let mut sections = SimpleIni::new();
        sections.load(path)?;
        Ok(Self { sections })
    }

    pub(super) fn parse(content: &str) -> Self {
        let mut sections = SimpleIni::new();
        sections.load_str(content);
        Self { sections }
    }

    pub(super) fn get(&self, section: &str, key: &str) -> Option<String> {
        self.sections.get(section, key).map(str::to_owned)
    }

    pub(super) fn section_has_any(&self, section: &str) -> bool {
        self.sections
            .get_section(section)
            .is_some_and(|s| !s.is_empty())
    }
}
