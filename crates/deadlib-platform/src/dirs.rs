//! Native per-application roots and host-friendly path display.

use std::path::{Path, PathBuf};

/// Filesystem namespace supplied by the application, interpreted using host conventions.
#[derive(Clone, Copy)]
pub struct AppIdentity<'a> {
    pub qualifier: &'a str,
    pub organization: &'a str,
    pub application: &'a str,
}

/// Standard OS locations, without portable-mode or application layout policy.
#[derive(Clone, Debug)]
pub struct NativeDirs {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub config_dir: PathBuf,
}

/// Discover native roots for the supplied application identity.
pub fn native_dirs(identity: AppIdentity<'_>) -> Option<NativeDirs> {
    let dirs = directories::ProjectDirs::from(
        identity.qualifier,
        identity.organization,
        identity.application,
    )?;
    Some(NativeDirs {
        data_dir: dirs.data_dir().to_path_buf(),
        cache_dir: dirs.cache_dir().to_path_buf(),
        config_dir: dirs.config_dir().to_path_buf(),
    })
}

/// Returns a host-friendly shorthand for an absolute path when a stable home
/// or application-data environment prefix is available.
#[must_use]
pub fn path_shorthand(path: &Path) -> String {
    if let Some(short) = try_path_shorthand(path) {
        return short;
    }
    path.display().to_string()
}

#[cfg(target_os = "windows")]
fn try_path_shorthand(path: &Path) -> Option<String> {
    for (var, label) in [
        ("APPDATA", "%APPDATA%"),
        ("LOCALAPPDATA", "%LOCALAPPDATA%"),
        ("USERPROFILE", "%USERPROFILE%"),
    ] {
        if let Some(replaced) = replace_path_prefix(path, &std::env::var_os(var)?, label) {
            return Some(replaced);
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
fn try_path_shorthand(path: &Path) -> Option<String> {
    replace_path_prefix(path, &std::env::var_os("HOME")?, "~")
}

fn replace_path_prefix(path: &Path, prefix: &std::ffi::OsStr, label: &str) -> Option<String> {
    let rest = path.strip_prefix(Path::new(prefix)).ok()?;
    if rest.as_os_str().is_empty() {
        return Some(label.to_owned());
    }
    Some(format!(
        "{label}{}{}",
        std::path::MAIN_SEPARATOR,
        rest.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_roots_follow_the_supplied_identity() {
        let roots = |application| {
            native_dirs(AppIdentity {
                qualifier: "org",
                organization: "Example",
                application,
            })
            .expect("native roots are available on the test host")
        };
        let first = roots("FirstApp");
        let second = roots("SecondApp");
        assert_ne!(first.data_dir, second.data_dir);
        assert_ne!(first.cache_dir, second.cache_dir);
        assert_ne!(first.config_dir, second.config_dir);
        assert!(first.data_dir.is_absolute());
        assert!(first.cache_dir.is_absolute());
    }

    #[test]
    fn path_prefix_replacement_keeps_the_same_relative_target() {
        let path = Path::new("/home/user/.example-app/save");
        let result = replace_path_prefix(path, std::ffi::OsStr::new("/home/user"), "~");
        assert_eq!(
            result,
            Some(format!("~{}.example-app/save", std::path::MAIN_SEPARATOR))
        );
    }

    #[test]
    fn path_prefix_replacement_uses_label_for_exact_match() {
        assert_eq!(
            replace_path_prefix(
                Path::new("/home/user"),
                std::ffi::OsStr::new("/home/user"),
                "~"
            )
            .as_deref(),
            Some("~")
        );
    }

    #[test]
    fn path_prefix_replacement_rejects_an_unrelated_root() {
        assert_eq!(
            replace_path_prefix(
                Path::new("/var/log/example-app.log"),
                std::ffi::OsStr::new("/home/user"),
                "~"
            ),
            None
        );
    }

    #[test]
    fn path_shorthand_falls_back_to_the_full_path() {
        let path = Path::new("/definitely/not/a/home/dir/x");
        assert_eq!(path_shorthand(path), path.display().to_string());
    }
}
