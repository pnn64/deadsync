use crate::{Container, Endian, Error, Format, SampleFormat};
use crate::{
    Muxer,
    mux::{put16, put32},
};
use std::io::{Seek, Write};

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub sample_format: SampleFormat,
    pub channels: u16,
    pub sample_rate: u32,
    /// Must be nonzero and no greater than the container width.
    pub valid_bits: u16,
    pub channel_mask: u32,
}
impl Spec {
    pub fn new(sample_format: SampleFormat, channels: u16, sample_rate: u32) -> Self {
        Self {
            sample_format,
            channels,
            sample_rate,
            valid_bits: sample_format.bits(),
            channel_mask: 0,
        }
    }
    pub(crate) fn header(
        self,
        e: Endian,
        force_extensible: bool,
    ) -> Result<([u8; 40], usize), Error> {
        let bits = self.sample_format.bits();
        let align = self
            .channels
            .checked_mul(bits / 8)
            .ok_or(Error::Invalid("block alignment overflow"))?;
        let rate = self
            .sample_rate
            .checked_mul(u32::from(align))
            .ok_or(Error::Invalid("byte rate overflow"))?;
        let ext = force_extensible
            || self.channels > 2
            || self.valid_bits != bits
            || self.channel_mask != 0;
        let tag = if self.sample_format.is_float() { 3 } else { 1 };
        let mut b = [0u8; 40];
        put16(&mut b, if ext { 0xfffe } else { tag }, e);
        put16(&mut b[2..], self.channels, e);
        put32(&mut b[4..], self.sample_rate, e);
        put32(&mut b[8..], rate, e);
        put16(&mut b[12..], align, e);
        put16(&mut b[14..], bits, e);
        let n = if ext {
            put16(&mut b[16..], 22, e);
            put16(&mut b[18..], self.valid_bits, e);
            put32(&mut b[20..], self.channel_mask, e);
            put32(&mut b[24..], u32::from(tag), e);
            put16(&mut b[30..], 0x10, e);
            b[32..].copy_from_slice(&[0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71]);
            40
        } else if tag == 3 {
            18
        } else {
            16
        };
        Format::parse(&b[..n], e)?;
        Ok((b, n))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WriteOptions {
    pub container: Container,
    /// Reserve a JUNK chunk and promote RIFF to RF64 when 32-bit sizes overflow.
    /// Applies only to Container::Riff.
    pub auto_rf64: bool,
    pub force_extensible: bool,
}
impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            container: Container::Riff,
            auto_rf64: true,
            force_extensible: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RawChunk<'a> {
    pub id: [u8; 4],
    pub data: &'a [u8],
}

/// Streaming native PCM/float writer. Audio goes straight to the sink. `finish`
/// writes padding and patches sizes; dropping the writer does not finalize it.
pub struct Writer<W> {
    mux: Muxer<W>,
    spec: Spec,
    endian: Endian,
}
impl<W: Write + Seek> Writer<W> {
    pub fn new(
        inner: W,
        spec: Spec,
        options: WriteOptions,
        metadata: &[RawChunk<'_>],
    ) -> Result<Self, Error> {
        let endian = options.container.endian();
        let (format, n) = spec.header(endian, options.force_extensible)?;
        let mux = Muxer::new(inner, &format[..n], options, metadata)?;
        Ok(Self { mux, spec, endian })
    }
    pub fn finish(self) -> Result<W, Error> {
        self.mux.finish()
    }
}
impl<W: Write> Writer<W> {
    /// Emit final sizes before audio, allowing a Write-only sink. Call
    /// finish_stream after exactly `frames` complete native frames.
    pub fn with_frames(
        inner: W,
        spec: Spec,
        options: WriteOptions,
        metadata: &[RawChunk<'_>],
        frames: u64,
    ) -> Result<Self, Error> {
        let endian = options.container.endian();
        let (format, n) = spec.header(endian, options.force_extensible)?;
        let bytes = frames
            .checked_mul(u64::from(spec.channels) * u64::from(spec.sample_format.bits() / 8))
            .ok_or(Error::Limit("data size overflow"))?;
        let mux = Muxer::with_lengths(
            inner,
            &format[..n],
            options,
            metadata,
            crate::DataLength { bytes, frames },
        )?;
        Ok(Self { mux, spec, endian })
    }
    pub fn spec(&self) -> Spec {
        self.spec
    }
    pub fn frames_written(&self) -> u64 {
        self.mux.frames_written()
    }
    pub fn into_inner(self) -> W {
        self.mux.into_inner()
    }
    /// Complete interleaved frames already in the specified container byte order.
    /// Preserves sample bits, including low unused valid-bit padding.
    pub fn write_frames(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let align =
            usize::from(self.spec.channels) * usize::from(self.spec.sample_format.bits() / 8);
        if !bytes.len().is_multiple_of(align) {
            return Err(Error::Invalid("partial PCM frame"));
        }
        self.mux.write_data(bytes, (bytes.len() / align) as u64)
    }
    pub fn write_i16(&mut self, samples: &[i16]) -> Result<(), Error> {
        self.write_native(samples, SampleFormat::I16, |v, b, e| put16(b, *v as u16, e))
    }
    /// Native I32 only. Use `write_integers` for packed U8/I16/I24 containers.
    pub fn write_i32(&mut self, samples: &[i32]) -> Result<(), Error> {
        self.write_native(samples, SampleFormat::I32, |v, b, e| put32(b, *v as u32, e))
    }
    pub fn write_f32(&mut self, samples: &[f32]) -> Result<(), Error> {
        self.write_native(samples, SampleFormat::F32, |v, b, e| {
            put32(b, v.to_bits(), e)
        })
    }
    pub fn write_f64(&mut self, samples: &[f64]) -> Result<(), Error> {
        self.write_native(samples, SampleFormat::F64, |v, b, e| {
            b.copy_from_slice(&match e {
                Endian::Little => v.to_bits().to_le_bytes(),
                Endian::Big => v.to_bits().to_be_bytes(),
            });
        })
    }
    fn write_native<T>(
        &mut self,
        samples: &[T],
        expected: SampleFormat,
        pack: impl Fn(&T, &mut [u8], Endian),
    ) -> Result<(), Error> {
        if self.spec.sample_format != expected {
            return Err(Error::Unsupported("requested native sample type"));
        }
        if !samples
            .len()
            .is_multiple_of(usize::from(self.spec.channels))
        {
            return Err(Error::Invalid("partial PCM frame"));
        }
        let e = self.endian;
        if e.native() {
            // SAFETY: private callers supply only fully initialized numeric values
            // without padding; the immutable byte borrow covers their exact storage.
            let b = unsafe {
                core::slice::from_raw_parts(
                    samples.as_ptr().cast::<u8>(),
                    core::mem::size_of_val(samples),
                )
            };
            self.write_frames(b)
        } else {
            // A frame may be larger than scratch. Pack samples and write bytes
            // through a private path, while preserving frame boundaries externally.
            let mut scratch = [0u8; 8192];
            let width = usize::from(expected.bits() / 8);
            self.pack_samples(samples, &mut scratch, width, |v, b| pack(v, b, e))
        }
    }
    /// Explicit integer packing in native container scale; out-of-range values
    /// fail before writing any samples. U8 input is signed around zero.
    pub fn write_integers(&mut self, samples: &[i32]) -> Result<(), Error> {
        let bits = self.spec.sample_format.bits();
        if self.spec.sample_format.is_float() {
            return Err(Error::Unsupported("integer packing to float"));
        }
        if !samples
            .len()
            .is_multiple_of(usize::from(self.spec.channels))
        {
            return Err(Error::Invalid("partial PCM frame"));
        }
        if bits < 32 {
            let bound = 1i32 << (bits - 1);
            if samples.iter().any(|&n| n < -bound || n >= bound) {
                return Err(Error::Invalid("integer exceeds sample container"));
            }
        }
        if bits == 32 {
            return self.write_i32(samples);
        }
        let e = self.endian;
        self.pack_samples(samples, &mut [0; 8192], usize::from(bits / 8), |n, b| {
            if bits == 8 {
                b[0] = (*n + 128) as u8;
            } else if e == Endian::Little {
                b.copy_from_slice(&n.to_le_bytes()[..bits as usize / 8]);
            } else {
                b.copy_from_slice(&n.to_be_bytes()[4 - bits as usize / 8..]);
            }
        })
    }
    fn pack_samples<T>(
        &mut self,
        samples: &[T],
        scratch: &mut [u8],
        width: usize,
        pack: impl Fn(&T, &mut [u8]),
    ) -> Result<(), Error> {
        let align = usize::from(self.spec.channels) * width;
        let per_batch = if align <= scratch.len() {
            scratch.len() / align * usize::from(self.spec.channels)
        } else {
            scratch.len() / width
        };
        let total = (samples.len() as u64)
            .checked_mul(width as u64)
            .ok_or(Error::Limit("data size overflow"))?;
        self.mux.write_with(
            total,
            (samples.len() / usize::from(self.spec.channels)) as u64,
            |inner| {
                for batch in samples.chunks(per_batch) {
                    for (v, b) in batch.iter().zip(scratch.chunks_exact_mut(width)) {
                        pack(v, b);
                    }
                    inner.write_all(&scratch[..batch.len() * width])?;
                }
                Ok(())
            },
        )
    }
    pub fn finish_stream(self) -> Result<W, Error> {
        self.mux.finish_stream()
    }
}
