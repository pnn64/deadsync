use deadsync_profile::pad_config::PadConfigProfile;

pub fn config_matches(profile: &PadConfigProfile, backend: &str, pad_type: Option<&str>) -> bool {
    profile.backend == backend
        && match (profile.pad_type.as_deref(), pad_type) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
}

pub fn is_default_for(profile: &PadConfigProfile, serial: &str) -> bool {
    profile.default_for_serials.iter().any(|s| s == serial)
}

pub fn resolve<'a>(
    profiles: &'a [PadConfigProfile],
    backend: &str,
    pad_type: Option<&str>,
    serial: &str,
) -> Option<&'a PadConfigProfile> {
    let compatible = |p: &&PadConfigProfile| config_matches(p, backend, pad_type);
    profiles
        .iter()
        .filter(compatible)
        .find(|p| is_default_for(p, serial))
        .or_else(|| {
            profiles
                .iter()
                .filter(compatible)
                .find(|p| p.global_default)
        })
}
