use super::LanguageMap;
use deadsync_config::ini::{BorrowedIniSections, borrowed_ini_sections, unescape_ini_value};
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub(super) fn parse_language_map(content: &str) -> LanguageMap {
    build_language_map(&borrowed_ini_sections(content))
}

pub(super) fn build_language_map(sections: &BorrowedIniSections<'_>) -> LanguageMap {
    let mut output = LanguageMap::with_capacity_and_hasher(sections.len(), Default::default());
    for (&section, props) in sections {
        // Count retained values so sections containing only @skip allocate no
        // entry table. Duplicates have already been resolved by the INI parser.
        let kept = props
            .values()
            .filter(|value| value.trim() != "@skip")
            .count();
        let mut entries = FxHashMap::with_capacity_and_hasher(kept, Default::default());
        if kept == 0 {
            output.insert(Box::from(section), entries);
            continue;
        }
        for (&key, &value) in props {
            if value.trim() == "@skip" {
                continue;
            }
            let value = if value.contains('\\') {
                Arc::from(unescape_ini_value(value))
            } else {
                Arc::from(value)
            };
            entries.insert(Box::from(key), value);
        }
        output.insert(Box::from(section), entries);
    }
    output
}
