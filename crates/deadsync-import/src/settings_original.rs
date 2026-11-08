// Frozen from db09a02d3; only visibility is adapted.
use super::*;

pub(super) fn read_simply_love(dir: &Path) -> HashMap<String, String> {
    let mut ini = SimpleIni::new();
    if let Some(path) = find_case_insensitive(dir, "Simply Love UserPrefs.ini")
        && ini.load(&path).is_ok()
        && let Some(section) = ini.get_section("Simply Love")
    {
        return section
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
    }
    HashMap::new()
}
