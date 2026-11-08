use deadsync_config::ini::{BorrowedIniSections, borrowed_ini_sections};

#[derive(Debug, Default)]
pub(super) struct ProfileIni<'a> {
    sections: BorrowedIniSections<'a>,
}

impl<'a> ProfileIni<'a> {
    pub(super) fn parse(content: &'a str) -> Self {
        Self {
            sections: borrowed_ini_sections(content),
        }
    }

    pub(super) fn get(&self, section: &str, key: &str) -> Option<String> {
        self.sections
            .get(section)
            .and_then(|properties| properties.get(key))
            .map(|value| (*value).to_owned())
    }

    pub(super) fn section_has_any(&self, section: &str) -> bool {
        self.sections.get(section).is_some_and(|s| !s.is_empty())
    }
}
