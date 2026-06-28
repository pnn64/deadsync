//! User pad-config profile storage.
//!
//! This module owns only the app-specific filesystem boundary. The profile
//! crate owns the pad-config data model, parsing, resolution, and list rules.

use crate::game::profile::local_profile_dir_for_id;
use deadsync_profile::pad_config::{
    PadConfigProfile, delete_config, parse, rename_config, serialize, set_default_config,
    upsert_config,
};
use log::warn;
use std::path::PathBuf;

fn padconfig_path(profile_id: &str) -> PathBuf {
    local_profile_dir_for_id(profile_id).join("padconfig.ini")
}

pub fn load(profile_id: &str) -> Vec<PadConfigProfile> {
    match std::fs::read_to_string(padconfig_path(profile_id)) {
        Ok(content) => parse(&content),
        Err(_) => Vec::new(),
    }
}

pub fn save(profile_id: &str, profiles: &[PadConfigProfile]) {
    let path = padconfig_path(profile_id);
    if let Err(e) = std::fs::write(&path, serialize(profiles)) {
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
    let mut list = load(profile_id);
    if upsert_config(
        &mut list,
        name,
        backend,
        pad_type,
        serial,
        make_default,
        settings,
    ) {
        save(profile_id, &list);
    }
}

pub fn set_default(profile_id: &str, serial: &str, name: &str) {
    let mut list = load(profile_id);
    if set_default_config(&mut list, serial, name) {
        save(profile_id, &list);
    }
}

pub fn rename(profile_id: &str, old: &str, new: &str) {
    let mut list = load(profile_id);
    if rename_config(&mut list, old, new) {
        save(profile_id, &list);
    }
}

pub fn delete(profile_id: &str, name: &str) {
    let mut list = load(profile_id);
    if delete_config(&mut list, name) {
        save(profile_id, &list);
    }
}
