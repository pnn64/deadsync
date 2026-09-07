//! Bounded stable device-slot assignment for native input backends.

use std::sync::{LazyLock, Mutex};

use arrayvec::ArrayVec;
use deadlib_platform::input::PAD_ID_COUNT_CAP;

use crate::backend::{PAD_ORDER_BACKENDS, PadOrderBackend};

/// Maximum stable device slots per backend before the shared overflow slot.
pub const PAD_ORDER_CAP: usize = PAD_ID_COUNT_CAP - 1;
const PAD_ORDER_BACKEND_COUNT: usize = PAD_ORDER_BACKENDS.len();

/// Bounded UUID order; snapshots copy at most 64 identifiers without allocating.
pub type PadOrderList = ArrayVec<[u8; 16], PAD_ORDER_CAP>;

struct PadDeviceOrder {
    lists: [PadOrderList; PAD_ORDER_BACKEND_COUNT],
}

impl Default for PadDeviceOrder {
    fn default() -> Self {
        Self {
            lists: std::array::from_fn(|_| PadOrderList::new()),
        }
    }
}

impl PadDeviceOrder {
    #[inline(always)]
    fn list(&self, backend: PadOrderBackend) -> &PadOrderList {
        &self.lists[pad_order_backend_index(backend)]
    }

    #[inline(always)]
    fn list_mut(&mut self, backend: PadOrderBackend) -> &mut PadOrderList {
        &mut self.lists[pad_order_backend_index(backend)]
    }

    fn clear(&mut self) {
        for list in &mut self.lists {
            list.clear();
        }
    }
}

#[inline(always)]
const fn pad_order_backend_index(backend: PadOrderBackend) -> usize {
    match backend {
        PadOrderBackend::RawInput => 0,
        PadOrderBackend::Wgi => 1,
        PadOrderBackend::IoHid => 2,
        PadOrderBackend::Hidraw => 3,
        PadOrderBackend::LinuxEvdev => 4,
        PadOrderBackend::FreeBsdEvdev => 5,
    }
}

/// Stable pad index assignment result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadOrderAssignment {
    pub index: u32,
    pub changed: bool,
}

/// Append-only, per-backend order of pad device UUIDs. The index of a UUID in
/// its backend list is the stable `PadId` that pad receives. Storage is bounded
/// and inline so discovering a pad never enters the allocator.
static PAD_DEVICE_ORDER: LazyLock<Mutex<PadDeviceOrder>> =
    LazyLock::new(|| Mutex::new(PadDeviceOrder::default()));

/// Stable `PadId` index for `uuid` on the given backend.
///
/// Returns the UUID's existing slot, or appends it and returns the new slot.
/// Append-only: known devices are never renumbered, so per-pad mappings stay
/// bound to the same physical pad. `changed` tells the config owner whether it
/// should persist the new order. Device discovery is a cold path.
/// # Panics
///
/// Panics if an internal synchronization lock is poisoned.
pub fn pad_index_for_uuid(backend: PadOrderBackend, uuid: [u8; 16]) -> PadOrderAssignment {
    let mut order = PAD_DEVICE_ORDER.lock().expect("pad device order poisoned");
    let list = order.list_mut(backend);
    if let Some(i) = list.iter().position(|u| *u == uuid) {
        return PadOrderAssignment {
            index: i as u32,
            changed: false,
        };
    }
    if list.len() >= PAD_ORDER_CAP {
        return PadOrderAssignment {
            index: list.len() as u32,
            changed: false,
        };
    }
    list.push(uuid);
    PadOrderAssignment {
        index: (list.len() - 1) as u32,
        changed: true,
    }
}

/// Replaces a backend's order with resolved UUIDs, removing duplicates and capping slots.
///
/// Call during startup/configuration, before backend discovery. Parsing and
/// persistence belong to the caller; this operation never allocates.
pub fn set_pad_order(backend: PadOrderBackend, uuids: impl IntoIterator<Item = [u8; 16]>) {
    let list = sanitize(uuids);
    let mut order = PAD_DEVICE_ORDER.lock().expect("pad device order poisoned");
    *order.list_mut(backend) = list;
}

/// Copies a bounded device order for persistence without holding the discovery lock.
pub fn pad_order(backend: PadOrderBackend) -> PadOrderList {
    PAD_DEVICE_ORDER
        .lock()
        .expect("pad device order poisoned")
        .list(backend)
        .clone()
}

/// Clears slot assignments at a configuration reset, before devices are rediscovered.
pub fn reset_pad_order() {
    PAD_DEVICE_ORDER
        .lock()
        .expect("pad device order poisoned")
        .clear();
}

/// Drop duplicates, keeping first occurrence, and cap the list length.
fn sanitize(list: impl IntoIterator<Item = [u8; 16]>) -> PadOrderList {
    let mut out = PadOrderList::new();
    for u in list {
        if out.len() >= PAD_ORDER_CAP {
            break;
        }
        if !out.contains(&u) {
            out.push(u);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_dedups_and_caps() {
        let a = [1u8; 16];
        let b = [2u8; 16];
        assert_eq!(sanitize([a, b, a, b]).as_slice(), &[a, b]);

        let many: Vec<[u8; 16]> = (0..(PAD_ORDER_CAP as u16 + 10))
            .map(|i| {
                let mut u = [0u8; 16];
                u[0..2].copy_from_slice(&i.to_le_bytes());
                u
            })
            .collect();
        assert_eq!(sanitize(many).len(), PAD_ORDER_CAP);
    }

    #[test]
    fn device_slots_survive_reconnect_and_saturate_without_renumbering() {
        reset_pad_order();
        let a = [7; 16];
        let b = [8; 16];
        set_pad_order(PadOrderBackend::RawInput, [a, b, a]);
        assert_eq!(pad_order(PadOrderBackend::RawInput).as_slice(), &[a, b]);
        assert_eq!(
            pad_index_for_uuid(PadOrderBackend::RawInput, b),
            PadOrderAssignment {
                index: 1,
                changed: false
            }
        );
        assert_eq!(
            pad_index_for_uuid(PadOrderBackend::Wgi, b),
            PadOrderAssignment {
                index: 0,
                changed: true
            }
        );
        set_pad_order(
            PadOrderBackend::RawInput,
            (0..PAD_ORDER_CAP).map(|n| [n as u8; 16]),
        );
        assert_eq!(
            pad_index_for_uuid(PadOrderBackend::RawInput, [255; 16]),
            PadOrderAssignment {
                index: PAD_ORDER_CAP as u32,
                changed: false
            }
        );
        assert_eq!(pad_order(PadOrderBackend::RawInput).len(), PAD_ORDER_CAP);
        assert_eq!(
            pad_index_for_uuid(PadOrderBackend::RawInput, [3; 16]),
            PadOrderAssignment {
                index: 3,
                changed: false
            }
        );
        reset_pad_order();
        assert!(
            PAD_ORDER_BACKENDS
                .into_iter()
                .all(|backend| pad_order(backend).is_empty())
        );
    }
}
