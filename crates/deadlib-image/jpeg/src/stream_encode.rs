//! Incremental encoded output to `std::io::Write`.
//! Raster conversion and source model preparation still take place in memory.
use crate::output::Sink;
use crate::{Error, Result};
use std::io::{self, Write};

/// An encoding failure or the original error returned by the destination.
#[derive(Debug)]
pub enum WriteError {
    /// Invalid source, allocation failure, or a configured resource ceiling.
    Codec(Error),
    /// The original destination error, including its custom payload.
    Io(io::Error),
}
impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Codec(e) => e.fmt(f),
            Self::Io(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Codec(e) => e,
            Self::Io(e) => e,
        })
    }
}
impl From<Error> for WriteError {
    fn from(value: Error) -> Self {
        Self::Codec(value)
    }
}
/// Result of writing a JPEG stream.
pub type WriteResult<T> = std::result::Result<T, WriteError>;

/// Encodes directly to a writer with a reusable bounded staging buffer.
/// Successful calls drain that buffer and return bytes written for this image;
/// they do not call the destination's `flush`. Errors can leave a prefix in the
/// destination. Pending bytes are discarded on error and never written by Drop.
/// A subsequent call starts a new JPEG at the writer's current position; the
/// caller must discard or otherwise handle any failed prefix.
///
/// All thirteen JPEG-1 processes use the same marker and entropy writers as
/// the `Vec` APIs. Calls produce full interchange images or table-only streams;
/// use [`AbbreviatedStreamEncoder`](crate::AbbreviatedStreamEncoder) for paired
/// abbreviated output to separate logical destinations.
/// Source models and raster preparation remain resident in memory. Only the
/// encoded destination is incremental. Staging capacity is charged against the
/// memory left after source and encoder storage; `max_output_bytes` still
/// bounds the whole encoded stream. The destination's own storage is external
/// to these limits.
///
/// ```
/// use deadlib_image_jpeg::{EncodeInput, EncodeOptions, ColorSpace,
///     MetadataOptions, SampleData, StreamEncoder};
/// let input = EncodeInput {
///     width: 2, height: 2, precision: 8,
///     color_space: ColorSpace::Grayscale,
///     samples: SampleData::U8(&[0, 64, 128, 255]),
/// };
/// let mut encoder = StreamEncoder::new(Vec::new()); // File or socket also works
/// let bytes = encoder.encode_samples(&input, EncodeOptions::default(),
///     MetadataOptions::default())?;
/// assert_eq!(encoder.get_ref().len(), bytes);
/// # Ok::<(), deadlib_image_jpeg::WriteError>(())
/// ```
pub struct StreamEncoder<W: Write> {
    writer: W,
    buffer: Vec<u8>,
    requested: usize,
}
impl<W: Write> StreamEncoder<W> {
    /// Use an 8 KiB staging window, reduced by the available resource budget.
    pub fn new(writer: W) -> Self {
        Self::with_buffer_size(writer, 8192)
    }
    /// Zero selects direct writes. Resource ceilings can reduce the requested
    /// buffer size; encoded byte limits remain independent of staging storage.
    pub fn with_buffer_size(writer: W, bytes: usize) -> Self {
        Self {
            writer,
            buffer: Vec::new(),
            requested: bytes,
        }
    }
    /// Borrow the destination. No staged bytes remain after a successful call.
    pub fn get_ref(&self) -> &W {
        &self.writer
    }
    /// Mutably borrow the destination between encoding calls.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.writer
    }
    /// Return the destination without flushing it or writing additional bytes.
    pub fn into_inner(self) -> W {
        self.writer
    }
    /// Retained staging capacity. A later call with a smaller memory ceiling
    /// releases an oversized allocation before preparing its output.
    pub fn buffer_capacity(&self) -> usize {
        self.buffer.capacity()
    }

    /// Write quantized coefficients with the controls of `encode_coefficients`.
    pub fn encode_coefficients(
        &mut self,
        image: &crate::CoefficientImage<'_>,
        options: crate::CodingOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> WriteResult<usize> {
        self.emit(|sink| {
            crate::encode::write_with_sink(
                image,
                options,
                metadata,
                &mut crate::abbreviated::TableWriter::interchange(),
                sink,
            )
        })
    }
    /// Quantize a complete raster, then write its DCT image incrementally.
    pub fn encode_samples(
        &mut self,
        input: &crate::EncodeInput<'_>,
        options: crate::EncodeOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> WriteResult<usize> {
        let image = crate::quantize(input, options)?;
        self.encode_coefficients(&image, options.coding, metadata)
    }
    /// Convert a shared image through the raster DCT encoding path.
    pub fn encode_image(
        &mut self,
        image: &deadlib_image::Image,
        options: crate::EncodeOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> WriteResult<usize> {
        self.encode_samples(
            &crate::pixel_encode::shared_input(image)?,
            options,
            metadata,
        )
    }
    /// Write existing predictive sample planes without a raster conversion.
    pub fn encode_lossless(
        &mut self,
        image: &crate::LosslessImage<'_>,
        options: crate::LosslessOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> WriteResult<usize> {
        self.emit(|sink| {
            crate::lossless_encode::write_planes_with_sink(
                image,
                options,
                metadata,
                &mut crate::abbreviated::TableWriter::interchange(),
                sink,
            )
        })
    }
    /// Write a complete raster with predictive lossless coding.
    pub fn encode_lossless_samples(
        &mut self,
        input: &crate::EncodeInput<'_>,
        options: crate::LosslessOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> WriteResult<usize> {
        self.emit(|sink| {
            crate::lossless_encode::write_samples_with_sink(
                input,
                options,
                metadata,
                &mut crate::abbreviated::TableWriter::interchange(),
                sink,
            )
        })
    }
    /// Write a shared image with predictive lossless coding.
    pub fn encode_lossless_image(
        &mut self,
        image: &deadlib_image::Image,
        options: crate::LosslessOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> WriteResult<usize> {
        self.encode_lossless_samples(
            &crate::pixel_encode::shared_input(image)?,
            options,
            metadata,
        )
    }
    /// Write prepared hierarchy frames directly to one destination.
    pub fn encode_hierarchical(
        &mut self,
        image: &crate::HierarchicalImage<'_>,
        options: crate::HierarchicalOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> WriteResult<usize> {
        self.emit(|sink| {
            crate::hierarchical_encode::write_with_sink(
                image,
                options,
                metadata,
                &mut crate::abbreviated::TableWriter::interchange(),
                sink,
            )
        })
    }
    /// Build a hierarchy in memory from component planes, then write it.
    pub fn encode_pyramid_planes(
        &mut self,
        header: crate::HierarchyHeader,
        components: &[crate::ComponentPlane],
        source_metadata: Vec<crate::MetadataSegment<'_>>,
        options: crate::PyramidOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> WriteResult<usize> {
        let image = crate::build_hierarchical_planes(header, components, source_metadata, options)?;
        self.emit(|sink| {
            crate::hierarchical_encode::write_hierarchy_to_sink(
                &image,
                options.limits,
                options.max_output_bytes,
                options.omit_dhp_height,
                metadata,
                |i, _| Ok(options.levels[i].coding),
                &mut crate::abbreviated::TableWriter::interchange(),
                sink,
            )
        })
    }
    /// Build a hierarchy in memory from a raster, then write it.
    pub fn encode_pyramid_samples(
        &mut self,
        input: &crate::EncodeInput<'_>,
        raster: crate::PyramidRasterOptions,
        options: crate::PyramidOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> WriteResult<usize> {
        let image = crate::build_hierarchical_samples(input, raster, options)?;
        self.emit(|sink| {
            crate::hierarchical_encode::write_hierarchy_to_sink(
                &image,
                options.limits,
                options.max_output_bytes,
                options.omit_dhp_height,
                metadata,
                |i, _| Ok(options.levels[i].coding),
                &mut crate::abbreviated::TableWriter::interchange(),
                sink,
            )
        })
    }
    /// Build a hierarchy in memory from a shared image, then write it.
    pub fn encode_pyramid_image(
        &mut self,
        image: &deadlib_image::Image,
        raster: crate::PyramidRasterOptions,
        options: crate::PyramidOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> WriteResult<usize> {
        self.encode_pyramid_samples(
            &crate::pixel_encode::shared_input(image)?,
            raster,
            options,
            metadata,
        )
    }
    /// Write a table-only SOI/DQT/DHT/EOI stream with optional APPn/COM metadata.
    pub fn encode_tables(
        &mut self,
        tables: &crate::Tables,
        metadata: &[crate::MetadataSegment<'_>],
        limits: crate::Limits,
        max_output_bytes: usize,
    ) -> WriteResult<usize> {
        self.emit(|sink| tables.write_with_sink(metadata, limits, max_output_bytes, sink))
    }

    pub(crate) fn emit(
        &mut self,
        encode: impl FnOnce(&mut WriterSink<'_>) -> Result<()>,
    ) -> WriteResult<usize> {
        let mut sink = WriterSink::new(&mut self.writer, &mut self.buffer, self.requested);
        let result = encode(&mut sink).and_then(|()| sink.drain());
        match result {
            Ok(()) => Ok(sink.written),
            Err(codec) => Err(match sink.error.take() {
                Some(error) => WriteError::Io(error),
                None => WriteError::Codec(codec),
            }),
        }
    }
}

pub(crate) struct WriterSink<'a> {
    writer: &'a mut dyn Write,
    buffer: &'a mut Vec<u8>,
    requested: usize,
    used: usize,
    length: usize,
    written: usize,
    error: Option<io::Error>,
}

fn write_all(writer: &mut dyn Write, mut data: &[u8], written: &mut usize) -> io::Result<()> {
    while !data.is_empty() {
        match writer.write(data) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "JPEG destination accepted no bytes",
                ));
            }
            Ok(n) if n <= data.len() => {
                *written += n;
                data = &data[n..];
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "JPEG destination returned an invalid write length",
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

impl WriterSink<'_> {
    pub(crate) fn new<'a>(
        writer: &'a mut dyn Write,
        buffer: &'a mut Vec<u8>,
        requested: usize,
    ) -> WriterSink<'a> {
        WriterSink {
            writer,
            buffer,
            requested,
            used: 0,
            length: 0,
            written: 0,
            error: None,
        }
    }
    pub(crate) fn written(&self) -> usize {
        self.written
    }
    pub(crate) fn take_error(&mut self) -> Option<io::Error> {
        self.error.take()
    }
    fn failure(&mut self, error: io::Error) -> Error {
        let kind = error.kind();
        self.error = Some(error);
        Error::Io(kind)
    }
    fn drain(&mut self) -> Result<()> {
        let result = write_all(self.writer, &self.buffer[..self.used], &mut self.written);
        self.used = 0;
        result.map_err(|e| self.failure(e))
    }
}
impl Sink for WriterSink<'_> {
    const RETAINS_OUTPUT: bool = false;
    fn len(&self) -> usize {
        self.length
    }
    fn prepare(&mut self, memory: usize, limit: usize) -> Result<()> {
        let size = self.requested.min(memory).min(limit);
        if self.buffer.capacity() > memory {
            *self.buffer = Vec::new();
        }
        if self.buffer.capacity() < size {
            self.buffer
                .try_reserve_exact(size - self.buffer.len())
                .map_err(|_| Error::Allocation)?;
        }
        if self.buffer.capacity() > memory {
            *self.buffer = Vec::new();
            return Err(Error::LimitExceeded("encoder stream buffer"));
        }
        self.buffer.resize(size, 0);
        Ok(())
    }
    fn reserve(&mut self, _n: usize, _limit: usize) -> Result<()> {
        Ok(())
    }
    fn push(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() <= self.buffer.len() - self.used {
            self.buffer[self.used..self.used + bytes.len()].copy_from_slice(bytes);
            self.used += bytes.len();
        } else {
            self.drain()?;
            if bytes.len() >= self.buffer.len() {
                write_all(self.writer, bytes, &mut self.written).map_err(|e| self.failure(e))?;
            } else {
                self.buffer[..bytes.len()].copy_from_slice(bytes);
                self.used = bytes.len();
            }
        }
        self.length += bytes.len();
        Ok(())
    }
}
