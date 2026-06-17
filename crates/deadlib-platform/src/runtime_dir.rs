use std::path::PathBuf;

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
    std::env::set_current_dir(exe_dir()?)
}
