mod app;
mod assets;
mod config;
mod core;
mod game;
mod screens;
mod ui;

#[cfg(debug_assertions)]
fn set_runtime_dir() -> Result<(), Box<dyn std::error::Error>> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().ok_or_else(|| {
        std::io::Error::other(format!(
            "Cannot resolve executable directory from '{}'",
            exe_path.display()
        ))
    })?;
    let cwd = std::env::current_dir()?;
    if cwd == exe_dir {
        return Ok(());
    }

    let exe_has_markers = exe_dir.join("assets").is_dir()
        || exe_dir.join("songs").is_dir()
        || exe_dir.join("Songs").is_dir()
        || exe_dir.join("save").is_dir()
        || exe_dir.join("deadsync.ini").is_file();
    let cwd_has_markers = cwd.join("assets").is_dir()
        || cwd.join("songs").is_dir()
        || cwd.join("Songs").is_dir();
    if exe_has_markers || !cwd_has_markers {
        std::env::set_current_dir(exe_dir)?;
    }
    Ok(())
}

#[cfg(not(debug_assertions))]
fn set_runtime_dir() -> Result<(), Box<dyn std::error::Error>> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().ok_or_else(|| {
        std::io::Error::other(format!(
            "Cannot resolve executable directory from '{}'",
            exe_path.display()
        ))
    })?;
    std::env::set_current_dir(exe_dir)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    set_runtime_dir()?;

    // Install logger immediately, then set runtime max level from config after loading it.
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Trace)
        .try_init();
    // Startup default when config is missing or malformed.
    log::set_max_level(log::LevelFilter::Warn);

    config::load();
    log::set_max_level(config::get().log_level.as_level_filter());
    game::profile::load();
    if let Err(e) = core::audio::init() {
        // The game can run without audio; log the error and continue.
        log::error!("Failed to initialize audio engine: {e}");
    }
    app::run()
}
