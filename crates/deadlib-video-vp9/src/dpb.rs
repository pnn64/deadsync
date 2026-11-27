//! Decoded picture buffer (DPB) management (spec §8.10
//! "Reference frame update process" + the `show_existing_frame` branch of §8.9 "Output process").
//!
//! Holds 8 slots' worth of the spec's `FrameStore[ i ][ plane ]`/`RefFrameWidth[ i ]`/
//! `RefFrameHeight[ i ]`/`RefSubsamplingX[ i ]`/`RefSubsamplingY[ i ]`/`RefBitDepth[ i ]`.
//! `FrameStore` holds pixel data already cropped to the display size (`FrameWidth`/
//! `FrameHeight`, chroma after subsampling); this matches the spec §8.10 copy range
//! being `x = 0..FrameWidth-1` and so on. This makes the clamping used in motion
//! compensation (spec §8.5.2.4's `lastX`/`lastY`) line up directly with the plane's
//! `width - 1`/`height - 1`.

use alloc::sync::Arc;

use crate::framebuffer::RefPlane;
use crate::prob_tables::NUM_REF_FRAMES;

/// One slot's worth of reference frame data (the spec's `FrameStore[ i ]` + accompanying size info).
/// Held behind `Arc` everywhere it's stored/passed (see [`Dpb`]) so that a frame's pixel data
/// is built once and shared, rather than deep-cloned per DPB slot / per consuming reference.
#[derive(Debug)]
pub struct RefFrameData {
    /// `RefFrameWidth[ i ]` (frame width, in luma terms).
    pub width: u32,
    /// `RefFrameHeight[ i ]`.
    pub height: u32,
    pub subsampling_x: u32,
    pub subsampling_y: u32,
    pub bit_depth: u8,
    /// Cropped (`width x height`) luma plane (`u8` for 8-bit, `u16` for 10/12-bit).
    pub y: RefPlane,
    /// Cropped (`((width+subX)>>subX) x ((height+subY)>>subY)`) chroma plane.
    pub u: RefPlane,
    pub v: RefPlane,
}

/// The DPB, with 8 slots.
#[derive(Debug, Clone)]
pub struct Dpb {
    slots: [Option<Arc<RefFrameData>>; NUM_REF_FRAMES],
}

impl Default for Dpb {
    fn default() -> Self {
        Self::new()
    }
}

impl Dpb {
    pub fn new() -> Self {
        Self {
            slots: core::array::from_fn(|_| None),
        }
    }

    /// Shares the picture in one reference slot.
    pub fn get_arc(&self, idx: u8) -> Option<Arc<RefFrameData>> {
        self.slots.get(idx as usize)?.clone()
    }

    /// Maximum superblock-padded `(y_elems, uv_elems)` across all live DPB slots (used to bound
    /// working-plane capacity reuse across spatial SVC layers without retaining stale resizes).
    pub(crate) fn max_sb_plane_elems(&self) -> (usize, usize) {
        self.slots.iter().flatten().fold((0, 0), |(my, muv), slot| {
            let y_w = (slot.width as usize).next_multiple_of(64);
            let y_h = (slot.height as usize).next_multiple_of(64);
            let uv_w = y_w >> slot.subsampling_x;
            let uv_h = y_h >> slot.subsampling_y;
            (my.max(y_w * y_h), muv.max(uv_w * uv_h))
        })
    }

    /// Returns `true` if any live 10/12-bit DPB slot has matching luma/chroma backing geometry.
    pub(crate) fn has_u16_geometry(&self, key: ((usize, usize), (usize, usize))) -> bool {
        self.slots.iter().flatten().any(|slot| {
            matches!(
                (slot.y.u16_backing_dims(), slot.u.u16_backing_dims()),
                (Some(y), Some(u)) if (y, u) == key
            )
        })
    }

    /// Returns `true` if any live 8-bit DPB slot has matching cropped `(y_len, uv_len)`.
    pub(crate) fn has_u8_geometry(&self, key: (usize, usize)) -> bool {
        self.slots.iter().flatten().any(|slot| {
            matches!(
                (slot.y.u8_len(), slot.u.u8_len()),
                (Some(y), Some(u)) if (y, u) == key
            )
        })
    }

    /// Takes the replaced slots before constructing the next reference picture.
    pub(crate) fn retire(
        &mut self,
        refresh_frame_flags: u8,
    ) -> [Option<RefFrameData>; NUM_REF_FRAMES] {
        core::array::from_fn(|slot| {
            if refresh_frame_flags & (1 << slot) != 0 {
                self.slots[slot].take().and_then(Arc::into_inner)
            } else {
                None
            }
        })
    }

    pub(crate) fn store(&mut self, refresh_frame_flags: u8, data: &Arc<RefFrameData>) {
        for (slot, entry) in self.slots.iter_mut().enumerate() {
            if refresh_frame_flags & (1 << slot) != 0 {
                debug_assert!(entry.is_none());
                *entry = Some(Arc::clone(data));
            }
        }
    }
}
