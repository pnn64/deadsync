//! User pad-config profile storage.
//!
//! This module owns only the app-specific filesystem boundary. The profile
//! crate owns the pad-config data model, parsing, resolution, and list rules.

use crate::game::profile::local_profile_dir_for_id;
use deadsync_profile::pad_config::{
    PadConfigProfile, delete_path, load_path, rename_path, save_path, set_default_path, upsert_path,
};
use log::warn;
use std::path::PathBuf;

fn padconfig_path(profile_id: &str) -> PathBuf {
    local_profile_dir_for_id(profile_id).join("padconfig.ini")
}

pub fn load(profile_id: &str) -> Vec<PadConfigProfile> {
    load_path(&padconfig_path(profile_id)).unwrap_or_default()
}

pub fn save(profile_id: &str, profiles: &[PadConfigProfile]) {
    let path = padconfig_path(profile_id);
    if let Err(e) = save_path(&path, profiles) {
        warn!("Failed to save {}: {e}", path.display());
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
    let path = padconfig_path(profile_id);
    if let Err(e) = upsert_path(
        &path,
        name,
        backend,
        pad_type,
        serial,
        make_default,
        settings,
    ) {
        warn!("Failed to save {}: {e}", path.display());
    }
}

pub fn set_default(profile_id: &str, serial: &str, name: &str) {
    let path = padconfig_path(profile_id);
    if let Err(e) = set_default_path(&path, serial, name) {
        warn!("Failed to save {}: {e}", path.display());
    }
}

pub fn rename(profile_id: &str, old: &str, new: &str) {
    let path = padconfig_path(profile_id);
    if let Err(e) = rename_path(&path, old, new) {
        warn!("Failed to save {}: {e}", path.display());
    }
}

pub fn delete(profile_id: &str, name: &str) {
    let path = padconfig_path(profile_id);
    if let Err(e) = delete_path(&path, name) {
        warn!("Failed to save {}: {e}", path.display());
    }
}
