use crate::ini::SimpleIni;
use crate::writer::push_line;

pub fn load_never_cache_list(conf: &SimpleIni) -> Vec<String> {
    conf.get("Options", "NeverCacheList")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn never_cache_list_value(list: &[String]) -> String {
    list.join(",")
}

pub fn group_is_never_cached(list: &[String], group: &str) -> bool {
    let group = group.trim();
    if group.is_empty() {
        return false;
    }
    list.iter().any(|entry| entry.eq_ignore_ascii_case(group))
}

pub fn push_never_cache_list_option_line(content: &mut String, list: &[String]) {
    push_line(content, "NeverCacheList", never_cache_list_value(list));
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
    fn never_cache_list_parses_and_trims_entries() {
        let conf = ini("[Options]\nNeverCacheList= WIP Pack , ,Another \n");

        assert_eq!(
            load_never_cache_list(&conf),
            vec!["WIP Pack".to_string(), "Another".to_string()]
        );
    }

    #[test]
    fn never_cache_list_empty_when_missing_or_blank() {
        assert!(load_never_cache_list(&ini("[Options]\n")).is_empty());
        assert!(load_never_cache_list(&ini("[Options]\nNeverCacheList=\n")).is_empty());
    }

    #[test]
    fn never_cache_list_value_joins_entries() {
        assert_eq!(
            never_cache_list_value(&["Pack A".to_string(), "Pack B".to_string()]),
            "Pack A,Pack B"
        );
    }

    #[test]
    fn group_never_cache_match_trims_and_ignores_case() {
        let list = ["WIP Pack".to_string(), "Other".to_string()];

        assert!(group_is_never_cached(&list, " wip pack "));
        assert!(!group_is_never_cached(&list, "Missing"));
        assert!(!group_is_never_cached(&list, " "));
    }

    #[test]
    fn writes_never_cache_list_option_line() {
        let mut content = String::new();

        push_never_cache_list_option_line(
            &mut content,
            &["Pack A".to_string(), "Pack B".to_string()],
        );

        assert_eq!(content, "NeverCacheList=Pack A,Pack B\n");
    }
}
