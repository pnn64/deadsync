//! Frozen production functions from bda7ac85e.
use semver::Version;

#[inline]
pub fn current() -> Version {
    if let Ok(raw) = std::env::var("DEADSYNC_VERSION_OVERRIDE")
        && !raw.trim().is_empty()
    {
        match parse_release_tag(&raw) {
            Some(v) => return v,
            None => log::warn!("DEADSYNC_VERSION_OVERRIDE={raw:?} is not valid semver; ignoring"),
        }
    }
    Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("CARGO_PKG_VERSION is not valid semver; check Cargo.toml")
}

#[inline]
pub fn parse_release_tag(tag: &str) -> Option<Version> {
    let trimmed = tag.trim();
    let stripped = trimmed.strip_prefix('v').unwrap_or(trimmed);
    Version::parse(stripped).ok()
}

#[inline]
pub fn user_agent() -> String {
    format!("DeadSync/{}", current())
}

#[inline]
pub fn current_tag() -> String {
    format!("v{}", current())
}
