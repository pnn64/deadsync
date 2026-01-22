mod app;
mod assets;
mod config;
mod core;
mod game;
mod screens;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger as early as possible so startup subsystems (including audio)
    // can emit INFO diagnostics.
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .try_init();

    config::load();
    game::profile::load();
    if let Err(e) = core::audio::init() {
        // The game can run without audio; log the error and continue.
        log::error!("Failed to initialize audio engine: {e}");
    }
    app::run()
}
