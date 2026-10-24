use std::path::{Path, PathBuf};

/// Locate resources only for the standard Foo.app/Contents/MacOS layout.
pub fn app_bundle_resources_from_exe(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    if macos.file_name()? != "MacOS"
        || contents.file_name()? != "Contents"
        || bundle.extension()? != "app"
    {
        return None;
    }
    let resources = contents.join("Resources");
    resources.is_dir().then_some(resources)
}

pub fn app_bundle_resources() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        return app_bundle_resources_from_exe(&std::env::current_exe().ok()?);
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub fn exe_dir() -> std::io::Result<PathBuf> {
    let exe_path = std::env::current_exe()?;
    exe_path.parent().map(PathBuf::from).ok_or_else(|| {
        std::io::Error::other(format!(
            "Cannot resolve executable directory from '{}'",
            exe_path.display()
        ))
    })
}

pub fn set_current_dir_to_exe_dir() -> std::io::Result<()> {
    let root = app_bundle_resources().map_or_else(exe_dir, Ok)?;
    std::env::set_current_dir(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_resources_require_the_complete_application_layout() {
        let root = std::env::temp_dir().join(format!("deadsync-app-layout-{}", std::process::id()));
        let executable = root.join("DeadSync.app/Contents/MacOS/deadsync");
        assert_eq!(app_bundle_resources_from_exe(&executable), None);
        let resources = root.join("DeadSync.app/Contents/Resources");
        std::fs::create_dir_all(&resources).unwrap();
        assert_eq!(app_bundle_resources_from_exe(&executable), Some(resources));
        assert_eq!(
            app_bundle_resources_from_exe(&root.join("Contents/MacOS/deadsync")),
            None
        );
        assert_eq!(
            app_bundle_resources_from_exe(Path::new("/usr/local/bin/deadsync")),
            None
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
