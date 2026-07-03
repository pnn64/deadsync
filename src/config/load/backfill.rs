use super::*;
use deadlib_platform::dirs;
use deadsync_config::backfill::has_missing_fields;

pub(super) fn write_missing_fields(conf: &SimpleIni) {
    let content = current_save_content();
    if has_missing_fields(conf, &content) {
        queue_save_write(content);
        info!(
            "'{}' updated with default values for any missing fields.",
            dirs::app_dirs().config_path().display()
        );
    } else {
        info!("Configuration OK; no write needed.");
    }
}
