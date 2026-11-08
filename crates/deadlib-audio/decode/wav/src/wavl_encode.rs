//! Native PCM/float packing over the codec-independent wavl muxer.
use crate::{
    DataLength, Endian, Error, RawChunk, SampleFormat, Spec, WavlMuxer, WriteOptions,
    mux::{put16, put32},
};
use std::io::{Seek, Write};

/// Native PCM/float wavl writer. Every write emits a complete child chunk;
/// `write_silence` holds the preceding frame according to the WAVE specification.
/// Dropping or extracting the sink does not finalize unknown-length headers.
pub struct WavlWriter<W> {
    mux: WavlMuxer<W>,
    spec: Spec,
}
impl<W: Write + Seek> WavlWriter<W> {
    pub fn new(
        inner: W,
        spec: Spec,
        options: WriteOptions,
        metadata: &[RawChunk<'_>],
    ) -> Result<Self, Error> {
        let (format, n) = spec.header(options.container.endian(), options.force_extensible)?;
        // Spec::header has already validated the complete native descriptor.
        let align = options.container.endian().u16(&format[12..]);
        let mux = WavlMuxer::new_validated(inner, &format[..n], Some(align), options, metadata)?;
        Ok(Self { mux, spec })
    }
    pub fn finish(self) -> Result<W, Error> {
        self.mux.finish()
    }
}
impl<W: Write> WavlWriter<W> {
    /// `length.bytes` is the complete LIST payload: four bytes for `wavl`, plus
    /// every child header, payload and odd pad. Frames include virtual silence.
    pub fn with_lengths(
        inner: W,
        spec: Spec,
        options: WriteOptions,
        metadata: &[RawChunk<'_>],
        length: DataLength,
    ) -> Result<Self, Error> {
        let (format, n) = spec.header(options.container.endian(), options.force_extensible)?;
        let align = options.container.endian().u16(&format[12..]);
        let mux = WavlMuxer::with_lengths_validated(
            inner,
            &format[..n],
            Some(align),
            options,
            metadata,
            length,
        )?;
        Ok(Self { mux, spec })
    }
    pub fn finish_stream(self) -> Result<W, Error> {
        self.mux.finish_stream()
    }
    pub fn spec(&self) -> Spec {
        self.spec
    }
    pub fn expected_length(&self) -> Option<DataLength> {
        self.mux.expected_length()
    }
    pub fn frames_written(&self) -> u64 {
        self.mux.frames_written()
    }
    pub fn list_bytes_written(&self) -> u64 {
        self.mux.list_bytes_written()
    }
    pub fn into_inner(self) -> W {
        self.mux.into_inner()
    }
    pub fn write_frames(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let align =
            usize::from(self.spec.channels) * usize::from(self.spec.sample_format.bits() / 8);
        if !bytes.len().is_multiple_of(align) {
            return Err(Error::Invalid("partial PCM frame"));
        }
        self.mux.write_data(bytes, (bytes.len() / align) as u64)
    }
    /// Split long counts into u32 slnt records without materializing samples.
    pub fn write_silence(&mut self, frames: u64) -> Result<(), Error> {
        self.mux.write_silence(frames)
    }
    /// Preserve one ancillary child payload. Audio methods account for data/slnt.
    pub fn write_chunk(&mut self, record: RawChunk<'_>) -> Result<(), Error> {
        self.mux.write_chunk(record)
    }
    pub fn write_i16(&mut self, samples: &[i16]) -> Result<(), Error> {
        self.native(samples, SampleFormat::I16, |v, b, e| put16(b, *v as u16, e))
    }
    pub fn write_i32(&mut self, samples: &[i32]) -> Result<(), Error> {
        self.native(samples, SampleFormat::I32, |v, b, e| put32(b, *v as u32, e))
    }
    pub fn write_f32(&mut self, samples: &[f32]) -> Result<(), Error> {
        self.native(samples, SampleFormat::F32, |v, b, e| {
            put32(b, v.to_bits(), e)
        })
    }
    pub fn write_f64(&mut self, samples: &[f64]) -> Result<(), Error> {
        self.native(samples, SampleFormat::F64, |v, b, e| {
            b.copy_from_slice(&if e == Endian::Little {
                v.to_bits().to_le_bytes()
            } else {
                v.to_bits().to_be_bytes()
            })
        })
    }
    fn native<T>(
        &mut self,
        samples: &[T],
        expected: SampleFormat,
        pack: impl Fn(&T, &mut [u8], Endian),
    ) -> Result<(), Error> {
        if self.spec.sample_format != expected {
            return Err(Error::Unsupported("requested native sample type"));
        }
        let channels = usize::from(self.spec.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(Error::Invalid("partial PCM frame"));
        }
        let e = self.mux.endian();
        if e.native() {
            // SAFETY: only private numeric callers with no padding and fully
            // initialized bits reach this immutable byte view.
            let b = unsafe {
                std::slice::from_raw_parts(
                    samples.as_ptr().cast::<u8>(),
                    std::mem::size_of_val(samples),
                )
            };
            return self.write_frames(b);
        }
        let width = usize::from(expected.bits() / 8);
        self.mux.data_with(
            std::mem::size_of_val(samples) as u64,
            (samples.len() / channels) as u64,
            |w| {
                let mut scratch = [0; 8192];
                for values in samples.chunks(scratch.len() / width) {
                    for (i, v) in values.iter().enumerate() {
                        pack(v, &mut scratch[i * width..(i + 1) * width], e);
                    }
                    w.write_all(&scratch[..values.len() * width])?;
                }
                Ok(())
            },
        )
    }
    pub fn write_integers(&mut self, samples: &[i32]) -> Result<(), Error> {
        let sample = self.spec.sample_format;
        if sample.is_float() {
            return Err(Error::Unsupported("integer packing to float"));
        }
        if sample == SampleFormat::I32 {
            return self.write_i32(samples);
        }
        let channels = usize::from(self.spec.channels);
        if !samples.len().is_multiple_of(channels) {
            return Err(Error::Invalid("partial PCM frame"));
        }
        let bits = sample.bits();
        let bound = 1i32 << (bits - 1);
        if samples.iter().any(|&v| v < -bound || v >= bound) {
            return Err(Error::Invalid("integer exceeds sample container"));
        }
        let width = usize::from(bits / 8);
        let e = self.mux.endian();
        self.mux.data_with(
            samples.len() as u64 * width as u64,
            (samples.len() / channels) as u64,
            |w| {
                let mut scratch = [0; 8192];
                for values in samples.chunks(scratch.len() / width) {
                    for (i, v) in values.iter().enumerate() {
                        let b = &mut scratch[i * width..(i + 1) * width];
                        if bits == 8 {
                            b[0] = (*v + 128) as u8;
                        } else if e == Endian::Little {
                            b.copy_from_slice(&v.to_le_bytes()[..width]);
                        } else {
                            b.copy_from_slice(&v.to_be_bytes()[4 - width..]);
                        }
                    }
                    w.write_all(&scratch[..values.len() * width])?;
                }
                Ok(())
            },
        )
    }
}
