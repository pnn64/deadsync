// Ship release builds as a Windows GUI-subsystem app so launching the game
// doesn't pop up a console window. Debug builds keep the console for developer
// convenience. Runtime output is still reachable: see `deadsync_platform::console`
// (reattaches to a parent terminal, or opens a console when `ShowConsole`/
// `--console` is set). No effect on non-Windows targets.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use deadsync::{app, assets, config, game};
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

fn audio_device_lines(devices: &[deadsync_audio_stream::OutputDeviceInfo]) -> Vec<String> {
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

/// Resolve whether the console window should be shown at startup. An explicit
/// `--console` / `--no-console` argument wins; otherwise fall back to the
/// `ShowConsole` config preference (default off).
fn resolve_show_console() -> bool {
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--console" => return true,
            "--no-console" => return false,
            _ => {}
        }
    }
    config::bootstrap_show_console()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = deadsync_updater::cli::UpdaterCli::from_env();
    deadsync_platform::runtime_dir::set_current_dir_to_exe_dir()?;
    deadsync_platform::host_time::init();

    // Resolve and create platform-native data/cache directories.
    deadsync_platform::dirs::ensure_dirs_exist();

    // Reconcile the GUI-subsystem release build with terminal/opt-in output
    // before the logger starts, so the first log lines land in the console when
    // one is wanted (and no window appears when it isn't).
    deadsync_platform::console::init(resolve_show_console());

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
    deadsync_updater::action::set_install_enabled(cfg.updater_install_enabled);
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

    deadsync_updater::state::load_persisted_cache();
    if cli.no_update_check {
        log::info!("Startup update check disabled by --no-update-check");
    } else {
        deadsync_updater::state::spawn_startup_check();
    }

    // Initialize localization after config (which provides the language preference)
    // and before profile/audio/screens which may use tr() for display strings.
    let locale = assets::i18n::resolve_locale(cfg.language_flag);
    assets::i18n::init(&locale);

    #[cfg(windows)]
    let _windows_timing = deadsync_platform::windows_rt::boost_main_thread_timing();
    game::profile::load();
    if let Err(e) = deadsync_audio_stream::init(deadsync_audio_stream::InitConfig {
        output_device_index: cfg.audio_output_device_index,
        output_mode: cfg.audio_output_mode,
        #[cfg(target_os = "linux")]
        linux_backend: cfg.linux_audio_backend,
        sample_rate_hz: cfg.audio_sample_rate_hz,
    }) {
        // The game can run without audio; log the error and continue.
        log::error!("Failed to initialize audio runtime: {e}");
    } else {
        logging::write_report_block(
            "Startup audio devices",
            &audio_device_lines(&deadsync_audio_stream::startup_output_devices()),
        );

        // Pre-warm ReplayGain for the bundled menu/background music so the
        // first time one plays (fresh install, or after the cache was cleared)
        // it doesn't audibly adjust loudness a few seconds in. Background
        // priority keeps the foreground song preview ahead of this; already
        // cached tracks are a cheap disk hit, so this is a no-op once warmed.
        // Gated on the audio runtime initializing, since that is what sets up
        // the ReplayGain subsystem the prewarm workers depend on.
        if cfg.enable_replaygain {
            deadsync_audio_replaygain::prewarm_paths(
                assets::visual_styles::bundled_music_paths(),
                deadsync_audio_replaygain::Priority::Background,
            );
        }
    }

    app::run()
}
