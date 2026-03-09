mod app;
mod assets;
mod config;
mod core;
mod game;
mod screens;
mod ui;

#[cfg(windows)]
struct WindowsTimingGuard {
    timer_period_ms: u32,
}

#[cfg(windows)]
impl Drop for WindowsTimingGuard {
    fn drop(&mut self) {
        use windows::Win32::Media::timeEndPeriod;

        unsafe {
            let _ = timeEndPeriod(self.timer_period_ms);
        }
    }
}

#[cfg(windows)]
fn boost_windows_runtime_timing() -> WindowsTimingGuard {
    use windows::Win32::Media::timeBeginPeriod;
    use windows::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
    };

    let timer_period_ms = 1u32;
    unsafe {
        let timer_result = timeBeginPeriod(timer_period_ms);
        if timer_result == 0 {
            log::debug!("Requested Windows timer resolution: {}ms", timer_period_ms);
        } else {
            log::warn!(
                "Failed to request Windows timer resolution {}ms: MMRESULT={}",
                timer_period_ms,
                timer_result
            );
        }

        if let Err(e) = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) {
            log::warn!("Failed to raise main thread priority: {e}");
        } else {
            log::debug!("Raised main thread priority to HIGHEST");
        }
    }

    WindowsTimingGuard { timer_period_ms }
}

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
    let cwd_has_markers =
        cwd.join("assets").is_dir() || cwd.join("songs").is_dir() || cwd.join("Songs").is_dir();
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
    core::logging::init(config::bootstrap_log_to_file());
    // Startup default when config is missing or malformed.
    log::set_max_level(log::LevelFilter::Warn);

    config::load();
    log::set_max_level(config::get().log_level.as_level_filter());
    #[cfg(windows)]
    let _windows_timing = boost_windows_runtime_timing();
    game::profile::load();
    if let Err(e) = core::audio::init() {
        // The game can run without audio; log the error and continue.
        log::error!("Failed to initialize audio engine: {e}");
    }
    app::run()
}
