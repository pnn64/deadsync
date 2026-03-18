use deadsync::{app, config, core, game};
use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;

#[cfg(windows)]
struct WindowsTimingGuard {
    timer_period_ms: u32,
    _thread_policy: core::windows_rt::ThreadPolicyGuard,
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
    }

    WindowsTimingGuard {
        timer_period_ms,
        _thread_policy: core::windows_rt::boost_current_thread(core::windows_rt::ThreadRole::Main),
    }
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

fn startup_lines(cfg: &config::Config) -> Vec<String> {
    vec![
        format!(
            "Log file: {}",
            if cfg.log_to_file {
                "deadsync.log"
            } else {
                "disabled"
            }
        ),
        format!("Log level: {}", cfg.log_level.as_str()),
        format!("Video renderer: {}", cfg.video_renderer),
        format!("Display: {}", display_line(cfg)),
        format!(
            "Present: vsync={} policy={}",
            if cfg.vsync { "on" } else { "off" },
            cfg.present_mode_policy
        ),
        audio_request_line(cfg),
    ]
}

fn display_line(cfg: &config::Config) -> String {
    match cfg.display_mode() {
        config::DisplayMode::Windowed => {
            format!("Windowed {}x{}", cfg.display_width, cfg.display_height)
        }
        config::DisplayMode::Fullscreen(config::FullscreenType::Exclusive) => format!(
            "Fullscreen Exclusive {}x{} monitor={}",
            cfg.display_width, cfg.display_height, cfg.display_monitor
        ),
        config::DisplayMode::Fullscreen(config::FullscreenType::Borderless) => format!(
            "Fullscreen Borderless {}x{} monitor={}",
            cfg.display_width, cfg.display_height, cfg.display_monitor
        ),
    }
}

fn audio_request_line(cfg: &config::Config) -> String {
    let device = cfg
        .audio_output_device_index
        .map_or_else(|| "Auto".to_string(), |idx| format!("index {idx}"));
    let rate = cfg
        .audio_sample_rate_hz
        .map_or_else(|| "Auto".to_string(), |hz| format!("{hz} Hz"));
    #[cfg(target_os = "linux")]
    {
        return format!(
            "Audio request: device={device}, mode={}, backend={}, rate={rate}",
            cfg.audio_output_mode.as_str(),
            cfg.linux_audio_backend.as_str()
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        format!(
            "Audio request: device={device}, mode={}, rate={rate}",
            cfg.audio_output_mode.as_str()
        )
    }
}

fn audio_device_lines(devices: &[core::audio::OutputDeviceInfo]) -> Vec<String> {
    devices
        .iter()
        .enumerate()
        .map(|(idx, device)| {
            let default = if device.is_default { " (default)" } else { "" };
            let rates = if device.sample_rates_hz.is_empty() {
                String::new()
            } else {
                format!(
                    " [{} Hz]",
                    device
                        .sample_rates_hz
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            format!("Sound device {idx}: {}{default}{rates}", device.name)
        })
        .collect()
}

fn panic_payload(info: &PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed");
        let location = info.location().map_or_else(
            || "<unknown>".to_string(),
            |loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()),
        );
        let payload = panic_payload(info);
        let backtrace = Backtrace::force_capture();
        log::error!("Panic on thread '{thread_name}' at {location}: {payload}");
        log::error!("{backtrace}");
        log::logger().flush();
    }));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    set_runtime_dir()?;
    core::host_time::init();

    // Install logger immediately, then set runtime max level from config after loading it.
    core::logging::init(config::bootstrap_log_to_file());
    install_panic_hook();
    // Startup default when config is missing or malformed.
    log::set_max_level(log::LevelFilter::Warn);

    config::load();
    let cfg = config::get();
    log::set_max_level(cfg.log_level.as_level_filter());
    core::logging::write_startup_report(&startup_lines(&cfg));
    #[cfg(windows)]
    let _windows_timing = boost_windows_runtime_timing();
    game::profile::load();
    if let Err(e) = core::audio::init() {
        // The game can run without audio; log the error and continue.
        log::error!("Failed to initialize audio engine: {e}");
    } else {
        core::logging::write_report_block(
            "Startup audio devices",
            &audio_device_lines(&core::audio::startup_output_devices()),
        );
    }
    app::run()
}
