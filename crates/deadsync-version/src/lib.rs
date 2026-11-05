//! Version utilities for the running build and for parsing GitHub release
//! tags such as `v0.3.871`.
//!
//! Centralised so the in-app updater (and any other consumer) does not have
//! to duplicate the leading-`v` strip or the comparison rules.

use semver::Version;

/// Parse the build's [`CARGO_PKG_VERSION`] into a [`Version`].
///
/// Honours the `DEADSYNC_VERSION_OVERRIDE` environment variable so e2e
/// tests can rebuild the binary once and pretend to be an older version.
/// The override is read fresh on every call (cheap, no caching) so tests
/// can flip it between scenarios within a single process.  Invalid override
/// values are ignored with a warning to avoid surprising production users
/// who happen to set the variable.
///
/// Panics at runtime only if Cargo is configured with a non-semver version,
/// which would also break the build's package metadata.
#[inline]
#[must_use]
/// # Panics
///
/// Panics if an internal state invariant is violated.
pub fn current() -> Version {
    if let Some(version) = version_override() {
        return version;
    }
    Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("CARGO_PKG_VERSION is not valid semver; check Cargo.toml")
}

#[inline]
fn version_override() -> Option<Version> {
    if let Ok(raw) = std::env::var("DEADSYNC_VERSION_OVERRIDE")
        && !raw.trim().is_empty()
    {
        match parse_release_tag(&raw) {
            Some(v) => return Some(v),
            None => log::warn!("DEADSYNC_VERSION_OVERRIDE={raw:?} is not valid semver; ignoring"),
        }
    }
    None
}

#[inline(always)]
#[must_use]
pub const fn current_static() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// User-Agent product identifying this build in outgoing HTTP requests.
#[inline]
#[must_use]
pub fn user_agent() -> String {
    match version_override() {
        Some(version) => format!("DeadSync/{version}"),
        None => concat!("DeadSync/", env!("CARGO_PKG_VERSION")).to_owned(),
    }
}

/// `format!("v{}", current())`.  Centralised so callers display the same
/// tag string the updater will compare against.
#[inline]
#[must_use]
pub fn current_tag() -> String {
    match version_override() {
        Some(version) => format!("v{version}"),
        None => concat!("v", env!("CARGO_PKG_VERSION")).to_owned(),
    }
}

/// Parse a GitHub release tag such as `v0.3.871` (with or without the
/// leading `v`) into a [`Version`].  Returns `None` for tags that are not
/// valid semver, e.g. `latest`, `nightly-2026-04-29`.
#[inline]
#[must_use]
pub fn parse_release_tag(tag: &str) -> Option<Version> {
    let trimmed = tag.trim();
    let stripped = trimmed.strip_prefix('v').unwrap_or(trimmed);
    Version::parse(stripped).ok()
}

/// Returns `true` when `latest` is strictly greater than `current` per
/// semver precedence rules (`1.0.0-rc.1 < 1.0.0`, `0.3.10 > 0.3.9`, etc.).
#[inline]
#[must_use]
pub fn is_newer(latest: &Version, current: &Version) -> bool {
    latest > current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_release_tag_strips_v_prefix() {
        let v = parse_release_tag("v0.3.871").expect("valid tag");
        assert_eq!(v, Version::new(0, 3, 871));
    }

    #[test]
    fn parse_release_tag_accepts_no_prefix() {
        let v = parse_release_tag("1.2.3").expect("valid tag");
        assert_eq!(v, Version::new(1, 2, 3));
    }

    #[test]
    fn parse_release_tag_trims_whitespace() {
        let v = parse_release_tag("  v0.3.871\n").expect("valid tag");
        assert_eq!(v, Version::new(0, 3, 871));
    }

    #[test]
    fn parse_release_tag_rejects_garbage() {
        assert!(parse_release_tag("master").is_none());
        assert!(parse_release_tag("nightly-2026-04-29").is_none());
        assert!(parse_release_tag("").is_none());
        assert!(parse_release_tag("v").is_none());
    }

    #[test]
    fn parse_release_tag_keeps_prerelease() {
        let v = parse_release_tag("v1.0.0-rc.1").expect("valid pre-release");
        assert_eq!(v.pre.as_str(), "rc.1");
    }

    #[test]
    fn is_newer_basic_ordering() {
        let a = Version::new(0, 3, 871);
        let b = Version::new(0, 3, 872);
        assert!(is_newer(&b, &a));
        assert!(!is_newer(&a, &b));
        assert!(!is_newer(&a, &a));
    }

    #[test]
    fn is_newer_respects_prerelease_precedence() {
        let stable = Version::parse("1.0.0").unwrap();
        let rc = Version::parse("1.0.0-rc.1").unwrap();
        // Per semver: 1.0.0 > 1.0.0-rc.1
        assert!(is_newer(&stable, &rc));
        assert!(!is_newer(&rc, &stable));
    }
}
