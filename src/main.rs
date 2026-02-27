mod app;
mod assets;
mod config;
mod core;
mod game;
mod screens;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
