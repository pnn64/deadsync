use crate::ini::SimpleIni;

pub fn has_missing_fields(conf: &SimpleIni, expected_content: &str) -> bool {
    let mut expected = SimpleIni::new();
    expected.load_str(expected_content);
    expected.sections().iter().any(|(section, props)| {
        let Some(current_props) = conf.get_section(section) else {
            return true;
        };
        props.keys().any(|key| !current_props.contains_key(key))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ini(content: &str) -> SimpleIni {
        let mut conf = SimpleIni::new();
        conf.load_str(content);
        conf
    }

    #[test]
    fn detects_missing_generated_option() {
        let conf = ini("[Options]\nExisting=1\n[Theme]\nThemeKey=1\n");
        let expected = "[Options]\nExisting=1\nNewOption=1\n[Theme]\nThemeKey=1\n";
        assert!(has_missing_fields(&conf, expected));
    }

    #[test]
    fn detects_missing_generated_theme_key() {
        let conf = ini("[Options]\nOptionKey=1\n[Theme]\nExisting=1\n");
        let expected = "[Options]\nOptionKey=1\n[Theme]\nExisting=1\nNewThemeKey=1\n";
        assert!(has_missing_fields(&conf, expected));
    }

    #[test]
    fn detects_missing_generated_keymap() {
        let conf = ini("[Options]\nOptionKey=1\n[Keymaps]\nP1_Back=KeyCode::Escape\n");
        let expected = "\
[Options]\nOptionKey=1\n\
[Keymaps]\nP1_Back=KeyCode::Escape\nP1_Restart=KeyCode::F1\n";
        assert!(has_missing_fields(&conf, expected));
    }

    #[test]
    fn complete_config_ignores_value_differences() {
        let conf = ini("[Options]\nOptionKey=9\n[Theme]\nThemeKey=0\n");
        let expected = "[Options]\nOptionKey=1\n[Theme]\nThemeKey=1\n";
        assert!(!has_missing_fields(&conf, expected));
    }
}
