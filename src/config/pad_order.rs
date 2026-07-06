//! Stable per-pad `PadId` assignment config adapter.

use super::*;
use deadsync_config::runtime_state::load_pad_order_entries;
use deadsync_input_native::PadOrderBackend;

/// Stable `PadId` index for `uuid` on the given backend.
pub fn pad_index_for_uuid(backend: PadOrderBackend, uuid: [u8; 16]) -> u32 {
    let assignment = deadsync_input_native::pad_index_for_uuid(backend, uuid);
    if assignment.changed {
        save_without_keymaps();
    }
    assignment.index
}

/// Replace the in-memory order from the loaded config file.
pub(super) fn load_order_from_ini(conf: &SimpleIni) {
    let entries = load_pad_order_entries(conf);
    let entries = entries.as_ref().map(|entries| {
        entries
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    });
    deadsync_input_native::load_pad_order_from_ini_entries(entries);
}

/// Clear the persisted order (used when the config file can't be read).
pub(super) fn reset() {
    deadsync_input_native::reset_pad_order();
}
