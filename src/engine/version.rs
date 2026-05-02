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
pub fn current() -> Version {
    if let Ok(raw) = std::env::var("DEADSYNC_VERSION_OVERRIDE")
        && !raw.trim().is_empty()
    {
        match parse_release_tag(&raw) {
            Some(v) => return v,
            None => log::warn!(
                "DEADSYNC_VERSION_OVERRIDE={raw:?} is not valid semver; ignoring"
            ),
        }
    }
    Version::parse(env!("CARGO_PKG_VERSION"))
        .expect("CARGO_PKG_VERSION is not valid semver; check Cargo.toml")
}

/// `format!("v{}", current())`.  Centralised so callers display the same
/// tag string the updater will compare against.
#[inline]
pub fn current_tag() -> String {
    format!("v{}", current())
}

/// Parse a GitHub release tag such as `v0.3.871` (with or without the
/// leading `v`) into a [`Version`].  Returns `None` for tags that are not
/// valid semver, e.g. `latest`, `nightly-2026-04-29`.
#[inline]
pub fn parse_release_tag(tag: &str) -> Option<Version> {
    let trimmed = tag.trim();
    let stripped = trimmed.strip_prefix('v').unwrap_or(trimmed);
    Version::parse(stripped).ok()
}

/// Returns `true` when `latest` is strictly greater than `current` per
/// semver precedence rules (`1.0.0-rc.1 < 1.0.0`, `0.3.10 > 0.3.9`, etc.).
#[inline]
pub fn is_newer(latest: &Version, current: &Version) -> bool {
    latest > current
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;
    // Tests in this module mutate a shared process-wide env var. Cargo runs
    // tests in parallel by default, so we serialise the env-touching cases
    // through a single mutex.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn current_matches_cargo_pkg_version() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: serialised via ENV_LOCK; only touched in these tests.
        unsafe { std::env::remove_var("DEADSYNC_VERSION_OVERRIDE") };
        let v = current();
        assert_eq!(v.to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn override_env_takes_precedence_when_valid() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: see above.
        unsafe { std::env::set_var("DEADSYNC_VERSION_OVERRIDE", "0.0.1") };
        let v = current();
        unsafe { std::env::remove_var("DEADSYNC_VERSION_OVERRIDE") };
        assert_eq!(v, Version::new(0, 0, 1));
    }

    #[test]
    fn override_env_ignored_when_garbage() {
        let _g = ENV_LOCK.lock().unwrap();
        // SAFETY: see above.
        unsafe { std::env::set_var("DEADSYNC_VERSION_OVERRIDE", "not-a-version") };
        let v = current();
        unsafe { std::env::remove_var("DEADSYNC_VERSION_OVERRIDE") };
        assert_eq!(v.to_string(), env!("CARGO_PKG_VERSION"));
    }

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