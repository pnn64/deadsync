//! Stable per-pad `PadId` assignment config adapter.

use super::*;
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
    deadsync_input_native::reset_pad_order();
    for backend in deadsync_input_native::all_pad_order_backends() {
        let Some(raw) = conf.get("Options", ini_key(backend)) else {
            continue;
        };
        deadsync_input_native::load_pad_order_serialized(backend, &raw);
    }
}

/// Clear the persisted order (used when the config file can't be read).
pub(super) fn reset() {
    deadsync_input_native::reset_pad_order();
}

/// Comma-separated hex serialization of `backend`'s order (empty when none).
pub(super) fn serialized(backend: PadOrderBackend) -> String {
    deadsync_input_native::serialized_pad_order(backend)
}

pub(super) const fn all_backends() -> [PadOrderBackend; 6] {
    deadsync_input_native::all_pad_order_backends()
}

pub(super) const fn ini_key(backend: PadOrderBackend) -> &'static str {
    match backend {
        PadOrderBackend::RawInput => "PadOrderRawInput",
        PadOrderBackend::Wgi => "PadOrderWGI",
        PadOrderBackend::IoHid => "PadOrderIoHid",
        PadOrderBackend::Hidraw => "PadOrderHidraw",
        PadOrderBackend::LinuxEvdev => "PadOrderLinuxEvdev",
        PadOrderBackend::FreeBsdEvdev => "PadOrderFreeBsdEvdev",
    }
}
