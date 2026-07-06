use crate::ini::SimpleIni;
use crate::runtime::save_without_keymaps;
use crate::runtime_state::load_pad_order_entries;
use deadsync_input_native::PadOrderBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadIndexUpdate {
    pub index: u32,
    pub changed: bool,
}

pub fn pad_index_for_uuid(backend: PadOrderBackend, uuid: [u8; 16]) -> PadIndexUpdate {
    let assignment = deadsync_input_native::pad_index_for_uuid(backend, uuid);
    PadIndexUpdate {
        index: assignment.index,
        changed: assignment.changed,
    }
}

pub fn pad_index_for_uuid_saved(backend: PadOrderBackend, uuid: [u8; 16]) -> u32 {
    let update = pad_index_for_uuid(backend, uuid);
    if update.changed {
        save_without_keymaps();
    }
    update.index
}

pub fn load_order_from_ini(conf: &SimpleIni) {
    let entries = load_pad_order_entries(conf);
    let entries = entries.as_ref().map(|entries| {
        entries
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
    });
    deadsync_input_native::load_pad_order_from_ini_entries(entries);
}

pub fn reset() {
    deadsync_input_native::reset_pad_order();
}
