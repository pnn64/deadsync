use crate::app_config::Config;
use crate::backfill::has_missing_fields;
use crate::ini::SimpleIni;
use crate::load::load_bootstrap_bool;
use crate::runtime::{
    RUNTIME_CONFIG, config_path, create_default_config_file, current_save_content, get,
    queue_save_write,
};
use crate::runtime_state::PublishedConfigEffects;
use crate::update::dedicated_menu_navigation_label;
use deadlib_platform::logging;
use log::{info, warn};

#[must_use]
pub fn bootstrap_log_to_file() -> bool {
    let default = Config::default().log_to_file;
    load_bootstrap_bool(config_path(), "LogToFile", default)
}

#[must_use]
pub fn bootstrap_show_console() -> bool {
    let default = Config::default().show_console;
    load_bootstrap_bool(config_path(), "ShowConsole", default)
}

/// Read the install policy without loading or rewriting the game configuration.
#[must_use]
pub fn bootstrap_update_install() -> bool {
    let default = Config::default().updater_install_enabled;
    load_bootstrap_bool(config_path(), "UpdaterInstallEnabled", default)
}

/// Load settings and return the resolved values for the host to initialize its subsystems.
pub fn load() -> PublishedConfigEffects {
    ensure_config_file();

    let mut conf = SimpleIni::new();
    let path = config_path();
    let effects = match conf.load(path) {
        Ok(()) => load_from_ini(&conf),
        Err(e) => {
            warn!(
                "Failed to load '{}': {e}. Using default values.",
                path.display()
            );
            RUNTIME_CONFIG.reset_load_state();
            PublishedConfigEffects::from_config(&Config::default())
        }
    };

    apply_input_runtime_state();
    effects
}

fn ensure_config_file() {
    if !config_path().exists()
        && let Err(e) = create_default_config_file()
    {
        warn!("Failed to create default config file: {e}");
    }
}

fn load_from_ini(conf: &SimpleIni) -> PublishedConfigEffects {
    let effects = RUNTIME_CONFIG.load_from_ini(conf, Config::default());
    logging::set_file_logging_enabled(effects.log_to_file);
    info!("Configuration loaded from '{}'.", config_path().display());
    backfill_missing_fields(conf);
    effects
}

fn backfill_missing_fields(conf: &SimpleIni) {
    let content = current_save_content();
    if has_missing_fields(conf, &content) {
        queue_save_write(content);
        info!(
            "'{}' updated with default values for any missing fields.",
            config_path().display()
        );
    } else {
        info!("Configuration OK; no write needed.");
    }
}

fn apply_input_runtime_state() {
    let cfg = get();
    let supported =
        deadsync_input::any_player_has_dedicated_menu_buttons_for_mode(cfg.three_key_navigation);
    let state = RUNTIME_CONFIG.apply_input_runtime_state(supported);
    if state.dedicated.disabled_by_missing_bindings {
        warn!(
            "only_dedicated_menu_buttons is enabled but no player has the required dedicated menu buttons mapped for {} mode - disabling.",
            dedicated_menu_navigation_label(state.three_key_navigation)
        );
    }
}
