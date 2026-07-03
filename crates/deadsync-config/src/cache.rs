use crate::ini::SimpleIni;

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
}
