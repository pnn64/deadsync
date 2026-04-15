use deadsync::{app, config, engine, game, i18n};
use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;

#[cfg(windows)]
struct WindowsTimingGuard {
    timer_period_ms: u32,
    _thread_policy: engine::windows_rt::ThreadPolicyGuard,
}

#[cfg(windows)]
impl Drop for WindowsTimingGuard {
    fn drop(&mut self) {
        use windows::Win32::Media::timeEndPeriod;

        // SAFETY: `timeEndPeriod` takes only the timer-resolution value. We pass
        // the same value we requested at startup and ignore any OS-level failure
        // because this is best-effort cleanup during shutdown.
        unsafe {
            let _ = timeEndPeriod(self.timer_period_ms);
        }
    }
}

#[cfg(windows)]
fn boost_windows_runtime_timing() -> WindowsTimingGuard {
    use windows::Win32::Media::timeBeginPeriod;

    let timer_period_ms = 1u32;
    // SAFETY: `timeBeginPeriod` takes only the requested resolution and does not
    // retain pointers into Rust memory. We handle the return code explicitly.
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
        _thread_policy: engine::windows_rt::boost_current_thread(
            engine::windows_rt::ThreadRole::Main,
        ),
    }
}

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
    let dirs = config::dirs::app_dirs();
    vec![
        format!("Portable mode: {}", dirs.portable),
        format!("Data directory: {}", dirs.data_dir.display()),
        format!("Cache directory: {}", dirs.cache_dir.display()),
        format!(
            "Log file: {}",
            if cfg.log_to_file {
                dirs.log_path().display().to_string()
            } else {
                "disabled".to_string()
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
        format!(
            "Audio request: device={device}, mode={}, backend={}, rate={rate}",
            cfg.audio_output_mode.as_str(),
            cfg.linux_audio_backend.as_str()
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        format!(
            "Audio request: device={device}, mode={}, rate={rate}",
            cfg.audio_output_mode.as_str()
        )
    }
}

fn audio_device_lines(devices: &[engine::audio::OutputDeviceInfo]) -> Vec<String> {
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
    engine::host_time::init();

    // Resolve and create platform-native data/cache directories.
    config::dirs::ensure_dirs_exist();

    // Install logger immediately, then set runtime max level from config after loading it.
    engine::logging::init(config::bootstrap_log_to_file());
    install_panic_hook();
    // Startup default when config is missing or malformed.
    log::set_max_level(log::LevelFilter::Warn);

    // Log resolved directories and migrate data from exe dir if necessary.
    config::dirs::maybe_migrate_from_exe_dir();

    config::load();
    let cfg = config::get();
    log::set_max_level(cfg.log_level.as_level_filter());
    engine::logging::write_startup_report(&startup_lines(&cfg));

    // Initialize localization after config (which provides the language preference)
    // and before profile/audio/screens which may use tr() for display strings.
    let locale = match cfg.language_flag {
        config::LanguageFlag::Auto => i18n::detect_os_locale(),
        flag => flag.locale_code().to_string(),
    };
    i18n::init(&locale);

    #[cfg(windows)]
    let _windows_timing = boost_windows_runtime_timing();
    game::profile::load();
    if let Err(e) = engine::audio::init(engine::audio::InitConfig {
        output_device_index: cfg.audio_output_device_index,
        output_mode: cfg.audio_output_mode,
        #[cfg(target_os = "linux")]
        linux_backend: cfg.linux_audio_backend,
        sample_rate_hz: cfg.audio_sample_rate_hz,
    }) {
        // The game can run without audio; log the error and continue.
        log::error!("Failed to initialize audio engine: {e}");
    } else {
        engine::logging::write_report_block(
            "Startup audio devices",
            &audio_device_lines(&engine::audio::startup_output_devices()),
        );
    }
    app::run()
}
