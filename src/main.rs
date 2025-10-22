mod core;
mod ui;
mod screens;
mod app;
mod config;
mod gameplay;
mod assets;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    config::load();
    gameplay::profile::load();
    if let Err(e) = core::audio::init() {
        // The game can run without audio; log the error and continue.
        log::error!("Failed to initialize audio engine: {}", e);
    }
    core::network::init();
    // env_logger is initialized in app::run()
    app::run().await
}
