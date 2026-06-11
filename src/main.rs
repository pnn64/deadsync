use deadsync::{app, assets, config, engine, game};
use deadsync_platform::logging::{self, StartupBuildInfo};
use std::backtrace::Backtrace;
use std::panic::PanicHookInfo;

fn startup_lines(cfg: &config::Config) -> Vec<String> {
    let dirs = deadsync_platform::dirs::app_dirs();
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
    let cli = deadsync_updater::cli::UpdaterCli::from_env();
    deadsync_platform::runtime_dir::set_current_dir_to_exe_dir()?;
    deadsync_platform::host_time::init();

    // Resolve and create platform-native data/cache directories.
    deadsync_platform::dirs::ensure_dirs_exist();

    // Install logger immediately, then set runtime max level from config after loading it.
    logging::init(
        config::bootstrap_log_to_file(),
        deadsync_platform::dirs::app_dirs().log_path(),
    );
    install_panic_hook();
    // Startup default when config is missing or malformed.
    log::set_max_level(log::LevelFilter::Warn);

    if let Some(request) = cli.apply_update.clone() {
        let code = deadsync_updater::cli::run_apply_helper(request);
        log::logger().flush();
        std::process::exit(code);
    }

    config::load();
    let cfg = config::get();
    log::set_max_level(cfg.log_level.as_level_filter());
    logging::write_startup_report(
        StartupBuildInfo {
            name: "deadsync",
            version: env!("CARGO_PKG_VERSION"),
            build_hash: option_env!("DEADSYNC_BUILD_HASH").unwrap_or("unknown"),
            build_stamp: option_env!("DEADSYNC_BUILD_STAMP").unwrap_or("unknown"),
        },
        &startup_lines(&cfg),
    );

    if cli.restart {
        log::info!(
            "Restarted after self-update to {}",
            deadsync_version::current_tag()
        );
    }

    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::PathBuf::from))
    {
        let _ = cli.cleanup_old.as_deref();
        let report = deadsync_updater::apply_journal::recover(&exe_dir);
        if report.journal_removed {
            log::info!(
                "Updater recovery: backups_removed={} backups_restored={} installed_removed={} staging_removed={}",
                report.backups_removed,
                report.backups_restored,
                report.installed_removed,
                report.staging_removed,
            );
        }
    }

    engine::updater::state::load_persisted_cache();
    if cli.no_update_check {
        log::info!("Startup update check disabled by --no-update-check");
    } else {
        engine::updater::state::spawn_startup_check();
    }

    // Initialize localization after config (which provides the language preference)
    // and before profile/audio/screens which may use tr() for display strings.
    let locale = assets::i18n::resolve_locale(cfg.language_flag);
    assets::i18n::init(&locale);

    #[cfg(windows)]
    let _windows_timing = deadsync_platform::windows_rt::boost_main_thread_timing();
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
        logging::write_report_block(
            "Startup audio devices",
            &audio_device_lines(&engine::audio::startup_output_devices()),
        );
    }
    app::run()
}
