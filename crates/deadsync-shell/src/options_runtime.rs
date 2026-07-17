use deadsync_config::prelude::SimpleIni;
use deadsync_profile::compat as profile;
use deadsync_theme_simply_love::SimplyLoveScoreImportProfile;

pub(crate) fn score_import_profiles() -> Vec<SimplyLoveScoreImportProfile> {
    let mut profiles = Vec::new();
    for summary in profile::scan_local_profiles() {
        let profile_dir = profile::local_profile_dir_for_id(&summary.id);
        let mut groovestats = SimpleIni::new();
        let mut arrowcloud = SimpleIni::new();
        let groovestats_api_key = if groovestats
            .load(profile_dir.join("groovestats.ini"))
            .is_ok()
        {
            groovestats
                .get("GrooveStats", "ApiKey")
                .map_or_else(String::new, |value| value.trim().to_owned())
        } else {
            String::new()
        };
        let groovestats_username = if groovestats_api_key.is_empty() {
            String::new()
        } else {
            groovestats
                .get("GrooveStats", "Username")
                .map_or_else(String::new, |value| value.trim().to_owned())
        };
        let arrowcloud_api_key = if arrowcloud.load(profile_dir.join("arrowcloud.ini")).is_ok() {
            arrowcloud
                .get("ArrowCloud", "ApiKey")
                .map_or_else(String::new, |value| value.trim().to_owned())
        } else {
            String::new()
        };
        profiles.push(SimplyLoveScoreImportProfile {
            id: summary.id,
            display_name: summary.display_name.trim().to_owned(),
            groovestats_api_key,
            groovestats_username,
            arrowcloud_api_key,
        });
    }
    profiles.sort_by(|left, right| {
        left.display_name
            .to_ascii_lowercase()
            .cmp(&right.display_name.to_ascii_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    profiles
}
