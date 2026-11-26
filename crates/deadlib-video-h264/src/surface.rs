use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;

use crate::decoder::{ChromaFormat, Decoder, Frame};
use crate::dpb::{DecodedPicture, SurfacePool};
use crate::error::DecodeError;
use crate::nal::{NalUnit, NalUnitType};

pub(crate) enum SurfaceStorage {
    Owned(Frame),
    Reference {
        display: Frame,
        picture: Arc<DecodedPicture>,
        coded_width: usize,
        origin: (u32, u32),
    },
}

impl SurfaceStorage {
    fn poc(&self) -> i32 {
        match self {
            Self::Owned(frame) | Self::Reference { display: frame, .. } => frame.pic_order_cnt,
        }
    }
}

/// A display-ordered picture that can share its coded planes with the DPB.
pub struct SurfaceFrame {
    frame: Option<SurfaceStorage>,
    pool: Weak<SurfacePool>,
}

/// The visible rows of a decoded plane and the byte distance between them.
pub struct SurfacePlane<'a> {
    data: &'a [u8],
    stride: usize,
    row_bytes: usize,
    height: usize,
}

impl<'a> SurfacePlane<'a> {
    /// Distance in bytes between the starts of two consecutive rows.
    pub fn stride_bytes(&self) -> usize {
        self.stride
    }

    /// Iterate over the visible samples, one row at a time.
    pub fn rows(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        (0..self.height).map(|row| {
            let offset = row * self.stride;
            &self.data[offset..offset + self.row_bytes]
        })
    }
}

impl SurfaceFrame {
    /// Visible luma dimensions.
    pub fn size(&self) -> (u32, u32) {
        let frame = self.header();
        (frame.width, frame.height)
    }

    /// Display order count within the current output period.
    pub fn pic_order_cnt(&self) -> i32 {
        self.header().pic_order_cnt
    }

    /// Chroma sampling of the decoded planes.
    pub fn chroma_format(&self) -> ChromaFormat {
        self.header().chroma_format
    }

    /// Number of significant bits per sample.
    pub fn bit_depth(&self) -> u8 {
        self.header().bit_depth
    }

    fn header(&self) -> &Frame {
        match self.frame.as_ref().unwrap() {
            SurfaceStorage::Owned(frame) | SurfaceStorage::Reference { display: frame, .. } => {
                frame
            }
        }
    }

    /// Borrow one of the three planes in Y, Cb, Cr order.
    pub fn plane(&self, index: usize) -> Option<SurfacePlane<'_>> {
        let (frame, data, coded_width, origin) = match self.frame.as_ref().unwrap() {
            SurfaceStorage::Owned(frame) => {
                let plane = [&frame.y, &frame.u, &frame.v]
                    .get(index)
                    .copied()?
                    .as_slice();
                (frame, plane, frame.width as usize, (0, 0))
            }
            SurfaceStorage::Reference {
                display,
                picture,
                coded_width,
                origin,
            } => {
                if index >= 3 {
                    return None;
                }
                (display, picture.plane(index), *coded_width, *origin)
            }
        };
        let xs = u32::from(index != 0);
        let ys = if index == 0 {
            0
        } else {
            frame.chroma_format.shift_y()
        };
        let bps = frame.bytes_per_sample();
        let stride = (coded_width >> xs) * bps;
        let row_bytes = ((frame.width >> xs) as usize) * bps;
        let height = (frame.height >> ys) as usize;
        let offset = ((origin.1 >> ys) as usize) * stride + ((origin.0 >> xs) as usize) * bps;
        let len = if height > 0 {
            (height - 1) * stride + row_bytes
        } else {
            0
        };
        Some(SurfacePlane {
            data: data.get(offset..offset + len)?,
            stride,
            row_bytes,
            height,
        })
    }

    fn new(frame: SurfaceStorage, pool: &Arc<SurfacePool>) -> Self {
        Self {
            frame: Some(frame),
            pool: Arc::downgrade(pool),
        }
    }
}

impl Drop for SurfaceFrame {
    fn drop(&mut self) {
        if let Some(SurfaceStorage::Owned(frame)) = self.frame.take() {
            if let Some(pool) = self.pool.upgrade() {
                pool.put_picture([frame.y, frame.u, frame.v], None);
            }
        }
    }
}

/// Decodes H.264 into display-ordered surfaces that own their visible planes.
///
/// A surface may outlive its decoder. Its coded buffers become available for
/// reuse when the caller and the decoder release their shares.
///
/// # Example
///
/// ```no_run
/// use deadlib_video_h264::{nal::parse_annex_b, SurfaceDecoder, SurfaceFrame};
///
/// let bytes = std::fs::read("background.h264")?;
/// let mut decoder = SurfaceDecoder::new();
/// let consume = |surface: SurfaceFrame| {
///     let (width, height) = surface.size();
///     println!("{width}x{height}, POC {}", surface.pic_order_cnt());
/// };
/// for nal in parse_annex_b(&bytes) {
///     decoder.decode_nal(&nal, consume)?;
/// }
/// decoder.flush(consume);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct SurfaceDecoder {
    inner: Decoder,
    buffer: Vec<SurfaceStorage>,
    depth: usize,
}

impl Default for SurfaceDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceDecoder {
    /// Create a decoder with a reorder depth of 16 pictures.
    pub fn new() -> Self {
        Self::with_max_depth(16)
    }

    /// Create a decoder with the given reorder depth. When the stream
    /// signals an SPS reorder limit, it can be used as the depth.
    pub fn with_max_depth(depth: usize) -> Self {
        Self {
            inner: Decoder::with_surface_pool(),
            buffer: Vec::new(),
            depth,
        }
    }

    /// Feed one NAL and deliver any ready surfaces in display order.
    ///
    /// The decoder delivers surfaces from previous pictures before starting
    /// the new picture, so dropping a surface in the callback allows its
    /// storage to be reused immediately. The new picture can fail to decode
    /// after the callback has already received earlier pictures.
    pub fn decode_nal(
        &mut self,
        nal: &NalUnit,
        mut emit: impl FnMut(SurfaceFrame),
    ) -> Result<(), DecodeError> {
        let is_idr = nal.nal_unit_type == NalUnitType::SliceIdr;
        let buffer = &mut self.buffer;
        let depth = self.depth;
        let mut began_picture = false;
        self.inner.decode_nal_as(nal, |inner| {
            began_picture = true;
            let frame = inner.finalize_surface();
            let starts_period = frame.is_some() && inner.take_period_start();
            let pool = inner.surface_pool();
            if starts_period {
                Self::drain(buffer, pool, &mut emit);
            }
            if let Some(frame) = frame {
                buffer.push(frame);
            }
            if is_idr {
                Self::drain(buffer, pool, &mut emit);
            }
            while buffer.len() > depth {
                let index = buffer
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, frame)| frame.poc())
                    .unwrap()
                    .0;
                emit(SurfaceFrame::new(buffer.remove(index), pool));
            }
            None::<()>
        })?;
        if is_idr && !began_picture {
            let pool = self.inner.surface_pool();
            Self::drain(buffer, pool, &mut emit);
        }
        Ok(())
    }

    /// Deliver all remaining surfaces in display order.
    pub fn flush(&mut self, mut emit: impl FnMut(SurfaceFrame)) {
        if let Some(frame) = self.inner.flush_surface() {
            if self.inner.take_period_start() {
                let pool = self.inner.surface_pool();
                Self::drain(&mut self.buffer, pool, &mut emit);
            }
            self.buffer.push(frame);
        }
        let pool = self.inner.surface_pool();
        Self::drain(&mut self.buffer, pool, &mut emit);
    }

    fn drain(
        buffer: &mut Vec<SurfaceStorage>,
        pool: &Arc<SurfacePool>,
        emit: &mut impl FnMut(SurfaceFrame),
    ) {
        buffer.sort_by_key(SurfaceStorage::poc);
        for frame in buffer.drain(..) {
            emit(SurfaceFrame::new(frame, pool));
        }
    }
}
