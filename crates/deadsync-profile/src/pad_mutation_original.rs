// Frozen from 3fed54203393396b976e539ece4453cd06a60026 for exact mutation and capacity comparisons.
use super::PadConfigProfile;
pub fn set_default_config(list: &mut [PadConfigProfile], serial: &str, name: &str) -> bool {
    if !list.iter().any(|p| p.name.eq_ignore_ascii_case(name)) {
        return false;
    }
    for p in list.iter_mut() {
        let should = p.name.eq_ignore_ascii_case(name);
        p.default_for_serials.retain(|s| s != serial);
        if should {
            p.default_for_serials.push(serial.to_string());
        }
    }
    true
}
pub fn rename_config(list: &mut [PadConfigProfile], old: &str, new: &str) -> bool {
    let new = new.trim();
    if new.is_empty()
        || !list.iter().any(|p| p.name.eq_ignore_ascii_case(old))
        || list
            .iter()
            .any(|p| !p.name.eq_ignore_ascii_case(old) && p.name.eq_ignore_ascii_case(new))
    {
        return false;
    }
    if let Some(p) = list.iter_mut().find(|p| p.name.eq_ignore_ascii_case(old)) {
        p.name = new.to_string();
    }
    true
}
pub fn migrated_config_name(owner: &str, name: &str) -> String {
    let owner = owner.trim();
    if owner.is_empty() {
        name.to_owned()
    } else {
        format!("{owner} - {name}")
    }
}
