//! User pad-config profile storage.
//!
//! This module owns only the app-specific filesystem boundary. The profile
//! crate owns the pad-config data model, parsing, resolution, and list rules.

use crate::game::profile::local_profile_dir_for_id;
use deadsync_profile::pad_config::{
    PadConfigProfile, delete_dir, load_dir, pad_config_path, rename_dir, save_dir, set_default_dir,
    upsert_dir,
};
use log::warn;
use std::path::PathBuf;

fn profile_dir(profile_id: &str) -> PathBuf {
    local_profile_dir_for_id(profile_id)
}

pub fn load(profile_id: &str) -> Vec<PadConfigProfile> {
    load_dir(&profile_dir(profile_id))
}

pub fn save(profile_id: &str, profiles: &[PadConfigProfile]) {
    let dir = profile_dir(profile_id);
    if let Err(e) = save_dir(&dir, profiles) {
        warn!("Failed to save {}: {e}", pad_config_path(&dir).display());
    }
}

#[allow(clippy::too_many_arguments)]
pub fn upsert(
    profile_id: &str,
    name: &str,
    backend: &str,
    pad_type: Option<String>,
    serial: Option<String>,
    make_default: bool,
    settings: Vec<(String, String)>,
) {
    let dir = profile_dir(profile_id);
    if let Err(e) = upsert_dir(
        &dir,
        name,
        backend,
        pad_type,
        serial,
        make_default,
        settings,
    ) {
        warn!("Failed to save {}: {e}", pad_config_path(&dir).display());
    }
}

pub fn set_default(profile_id: &str, serial: &str, name: &str) {
    let dir = profile_dir(profile_id);
    if let Err(e) = set_default_dir(&dir, serial, name) {
        warn!("Failed to save {}: {e}", pad_config_path(&dir).display());
    }
}

pub fn rename(profile_id: &str, old: &str, new: &str) {
    let dir = profile_dir(profile_id);
    if let Err(e) = rename_dir(&dir, old, new) {
        warn!("Failed to save {}: {e}", pad_config_path(&dir).display());
    }
}

pub fn delete(profile_id: &str, name: &str) {
    let dir = profile_dir(profile_id);
    if let Err(e) = delete_dir(&dir, name) {
        warn!("Failed to save {}: {e}", pad_config_path(&dir).display());
    }
}
