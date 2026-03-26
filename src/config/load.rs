use super::store::create_default_config_file;
use super::*;

#[path = "load/apply.rs"]
mod apply;
#[path = "load/backfill.rs"]
mod backfill;
#[path = "load/options.rs"]
mod options;
#[path = "load/shared.rs"]
mod shared;
#[path = "load/theme.rs"]
mod theme_load;

pub fn bootstrap_log_to_file() -> bool {
    shared::bootstrap_log_to_file()
}

pub fn load() {
    ensure_config_file();

    let mut conf = SimpleIni::new();
    match conf.load(CONFIG_PATH) {
        Ok(()) => load_from_ini(&conf),
        Err(e) => {
            warn!("Failed to load '{CONFIG_PATH}': {e}. Using default values.");
            apply::load_defaults_after_error();
        }
    }

    apply::apply_input_runtime_state();
}

fn ensure_config_file() {
    if !std::path::Path::new(CONFIG_PATH).exists()
        && let Err(e) = create_default_config_file()
    {
        warn!("Failed to create default config file: {e}");
    }
}

fn load_from_ini(conf: &SimpleIni) {
    shared::load_runtime_state(conf);

    let default = Config::default();
    let mut cfg = default;
    options::load(conf, default, &mut cfg);
    theme_load::load(conf, default, &mut cfg);

    apply::publish_config(cfg);
    apply::publish_keymap(conf);
    backfill::write_missing_fields(conf);
}
