use crate::game::profile;
use std::collections::HashSet;

pub fn sync_known_packs(profile_ids: &[String], scanned_pack_names: &[String]) -> HashSet<String> {
    if profile_ids.is_empty() {
        return HashSet::new();
    }
    let mut out = HashSet::new();
    for profile_id in profile_ids {
        let known_pack_names =
            profile::known_pack_names_for_local_profile(profile_id).unwrap_or_default();
        if known_pack_names.is_empty() && !scanned_pack_names.is_empty() {
            profile::mark_known_pack_names_for_local_profile(
                profile_id,
                scanned_pack_names.iter().map(String::as_str),
            );
            continue;
        }
        out.extend(
            scanned_pack_names
                .iter()
                .filter(|name| !known_pack_names.contains(name.as_str()))
                .cloned(),
        );
    }
    out
}

pub fn mark_pack_known(profile_ids: &[String], name: &str) {
    mark_packs_known(profile_ids, std::iter::once(name));
}

pub fn mark_packs_known<'a>(profile_ids: &[String], pack_names: impl IntoIterator<Item = &'a str>) {
    let pack_names: Vec<&str> = pack_names.into_iter().collect();
    if profile_ids.is_empty() || pack_names.is_empty() {
        return;
    }
    for profile_id in profile_ids {
        profile::mark_known_pack_names_for_local_profile(profile_id, pack_names.iter().copied());
    }
}
