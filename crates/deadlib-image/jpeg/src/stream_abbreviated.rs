//! Paired abbreviated output to separate logical destinations.
use crate::abbreviated::TableWriter;
use crate::stream_encode::WriterSink;
use crate::{Error, Result, StreamEncoder, Tables, WriteError};
use std::io::{self, Write};

/// Encoded bytes accepted by each destination for one successful pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AbbreviatedBytes {
    /// Complete table packet, including SOI/EOI.
    pub tables: usize,
    /// Complete image stream, including SOI/EOI.
    pub image: usize,
}

/// A codec failure or the original error from one of the two destinations.
#[derive(Debug)]
pub enum AbbreviatedWriteError {
    /// Invalid source, allocation failure, or a configured resource ceiling.
    Codec(Error),
    /// Original error from the table destination.
    Tables(io::Error),
    /// Original error from the image destination.
    Image(io::Error),
}
impl std::fmt::Display for AbbreviatedWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Codec(error) => error.fmt(f),
            Self::Tables(error) => write!(f, "JPEG table destination: {error}"),
            Self::Image(error) => write!(f, "JPEG image destination: {error}"),
        }
    }
}
impl std::error::Error for AbbreviatedWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Codec(error) => error,
            Self::Tables(error) | Self::Image(error) => error,
        })
    }
}
/// Result of writing a paired abbreviated image.
pub type AbbreviatedWriteResult<T> = std::result::Result<T, AbbreviatedWriteError>;

/// Writes paired B.4/B.5 streams without retaining either encoded output.
///
/// Use separate logical destinations for tables and image data. Writes to them
/// can interleave. After a successful call, deliver its table stream and then
/// its image to the same receiver cache. Repeated calls append another stream
/// to each destination; the returned byte counts delimit those pairs. Sending
/// both destinations directly into a single channel does not preserve this
/// required order. Single-channel table-first transmission is not provided.
///
/// The table destination uses direct writes without an encoded-packet buffer.
/// The image uses `StreamEncoder`'s reusable bounded staging. External writer
/// buffers, source preparation and the persistent table cache remain separate
/// from encoded-output staging. No frame is encoded twice. All thirteen frame
/// processes and the raster/model conveniences of `AbbreviatedEncoder` share
/// the same marker, table and entropy writers.
///
/// `max_table_bytes` bounds the table packet, including SOI/EOI. Image options'
/// `max_output_bytes` bounds the sum of table and image bytes. Neither writer
/// is flushed by this codec. On any failure the cache is cleared, pending image
/// staging is discarded, and either destination may contain a prefix. Handle
/// those prefixes before retrying. The codec never drains staging on Drop;
/// each destination retains its own destructor behavior. Clear the cache if
/// the receiver loses its state or a successful pair is discarded.
///
/// ```
/// use deadlib_image_jpeg::{AbbreviatedStreamEncoder, EncodeInput, EncodeOptions,
///     ColorSpace, MetadataOptions, SampleData};
/// let input = EncodeInput {
///     width: 2, height: 2, precision: 8,
///     color_space: ColorSpace::Grayscale,
///     samples: SampleData::U8(&[0, 64, 128, 255]),
/// };
/// let mut encoder = AbbreviatedStreamEncoder::new(Vec::new(), Vec::new());
/// let first = encoder.encode_samples(&input, EncodeOptions::default(),
///     MetadataOptions::default())?;
/// let second = encoder.encode_samples(&input, EncodeOptions::default(),
///     MetadataOptions::default())?;
/// assert!(first.tables > 4);
/// assert_eq!(second.tables, 4); // SOI/EOI: all definitions already known
/// # Ok::<(), deadlib_image_jpeg::AbbreviatedWriteError>(())
/// ```
pub struct AbbreviatedStreamEncoder<T: Write, I: Write> {
    tables: Tables,
    table_writer: T,
    image: StreamEncoder<I>,
    /// Maximum table-packet length, including SOI/EOI. Defaults to 65,536 bytes.
    pub max_table_bytes: usize,
}
impl<T: Write, I: Write> AbbreviatedStreamEncoder<T, I> {
    /// Empty receiver cache and an 8 KiB image staging window.
    pub fn new(tables: T, image: I) -> Self {
        Self::with_buffer_size(tables, image, 8192)
    }
    /// Select the image staging window; zero uses direct writes for both outputs.
    pub fn with_buffer_size(tables: T, image: I, bytes: usize) -> Self {
        Self::from_parts(
            Tables::new(),
            tables,
            StreamEncoder::with_buffer_size(image, bytes),
        )
    }
    /// Start with known receiver destinations and an existing image writer.
    pub fn from_parts(tables: Tables, table_writer: T, image: StreamEncoder<I>) -> Self {
        Self {
            tables,
            table_writer,
            image,
            max_table_bytes: 65536,
        }
    }
    /// Current knowledge of the receiver's persistent destinations.
    pub fn tables(&self) -> &Tables {
        &self.tables
    }
    /// Reset cache knowledge before the next pair.
    pub fn clear(&mut self) {
        self.tables.clear();
    }
    /// Borrow table and image destinations, in that order.
    pub fn get_ref(&self) -> (&T, &I) {
        (&self.table_writer, self.image.get_ref())
    }
    /// Mutably borrow both destinations between calls.
    pub fn get_mut(&mut self) -> (&mut T, &mut I) {
        (&mut self.table_writer, self.image.get_mut())
    }
    /// Retained image staging capacity. Table output uses no codec staging.
    pub fn buffer_capacity(&self) -> usize {
        self.image.buffer_capacity()
    }
    /// Return both destinations without extra writes or flushes.
    pub fn into_inner(self) -> (T, I) {
        (self.table_writer, self.image.into_inner())
    }
    /// Return receiver-cache knowledge, the table writer and image encoder.
    pub fn into_parts(self) -> (Tables, T, StreamEncoder<I>) {
        (self.tables, self.table_writer, self.image)
    }
    fn prepare<V>(&mut self, result: Result<V>) -> AbbreviatedWriteResult<V> {
        result.map_err(|error| {
            self.clear();
            AbbreviatedWriteError::Codec(error)
        })
    }
    fn emit(
        &mut self,
        output_limit: usize,
        encode: impl FnOnce(
            &mut WriterSink<'_>,
            &mut TableWriter<'_, &mut WriterSink<'_>>,
        ) -> Result<()>,
    ) -> AbbreviatedWriteResult<AbbreviatedBytes> {
        // The empty Vec supplies WriterSink's zero-length staging view. It
        // allocates nothing, and packet preparation enforces direct writes.
        let mut unused = Vec::new();
        let mut table_sink = WriterSink::new(&mut self.table_writer, &mut unused, 0);
        let result = self.image.emit(|image_sink| {
            let mut writer = TableWriter::abbreviated_to(
                &mut self.tables,
                &mut table_sink,
                self.max_table_bytes,
                output_limit,
            )?;
            encode(image_sink, &mut writer)?;
            writer.finish()?;
            Ok(())
        });
        let result = if let Some(error) = table_sink.take_error() {
            Err(AbbreviatedWriteError::Tables(error))
        } else {
            match result {
                Ok(image) => Ok(AbbreviatedBytes {
                    tables: table_sink.written(),
                    image,
                }),
                Err(WriteError::Codec(error)) => Err(AbbreviatedWriteError::Codec(error)),
                Err(WriteError::Io(error)) => Err(AbbreviatedWriteError::Image(error)),
            }
        };
        if result.is_err() {
            self.tables.clear();
        }
        result
    }
    /// Preserve prepared DCT coefficients and quantizers in paired output.
    pub fn encode_coefficients(
        &mut self,
        image: &crate::CoefficientImage<'_>,
        options: crate::CodingOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> AbbreviatedWriteResult<AbbreviatedBytes> {
        self.emit(options.max_output_bytes, |sink, writer| {
            crate::encode::write_with_sink(image, options, metadata, writer, sink)
        })
    }
    /// Quantize a complete raster, then emit a paired DCT image.
    pub fn encode_samples(
        &mut self,
        input: &crate::EncodeInput<'_>,
        options: crate::EncodeOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> AbbreviatedWriteResult<AbbreviatedBytes> {
        let image = self.prepare(crate::quantize(input, options))?;
        self.encode_coefficients(&image, options.coding, metadata)
    }
    /// Convert a shared image through the paired raster DCT path.
    pub fn encode_image(
        &mut self,
        image: &deadlib_image::Image,
        options: crate::EncodeOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> AbbreviatedWriteResult<AbbreviatedBytes> {
        let input = self.prepare(crate::pixel_encode::shared_input(image))?;
        self.encode_samples(&input, options, metadata)
    }
    /// Emit paired predictive coding from existing native sample planes.
    pub fn encode_lossless(
        &mut self,
        image: &crate::LosslessImage<'_>,
        options: crate::LosslessOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> AbbreviatedWriteResult<AbbreviatedBytes> {
        self.emit(options.max_output_bytes, |sink, writer| {
            crate::lossless_encode::write_planes_with_sink(image, options, metadata, writer, sink)
        })
    }
    /// Emit paired predictive coding from a complete raster.
    pub fn encode_lossless_samples(
        &mut self,
        input: &crate::EncodeInput<'_>,
        options: crate::LosslessOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> AbbreviatedWriteResult<AbbreviatedBytes> {
        self.emit(options.max_output_bytes, |sink, writer| {
            crate::lossless_encode::write_samples_with_sink(input, options, metadata, writer, sink)
        })
    }
    /// Emit paired predictive coding from a shared image.
    pub fn encode_lossless_image(
        &mut self,
        image: &deadlib_image::Image,
        options: crate::LosslessOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> AbbreviatedWriteResult<AbbreviatedBytes> {
        let input = self.prepare(crate::pixel_encode::shared_input(image))?;
        self.encode_lossless_samples(&input, options, metadata)
    }
    /// Emit prepared hierarchy frames directly to paired destinations.
    pub fn encode_hierarchical(
        &mut self,
        image: &crate::HierarchicalImage<'_>,
        options: crate::HierarchicalOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> AbbreviatedWriteResult<AbbreviatedBytes> {
        self.emit(options.max_output_bytes, |sink, writer| {
            crate::hierarchical_encode::write_with_sink(image, options, metadata, writer, sink)
        })
    }
    /// Build a hierarchy in memory from native planes, then emit a pair.
    pub fn encode_pyramid_planes(
        &mut self,
        header: crate::HierarchyHeader,
        components: &[crate::ComponentPlane],
        source_metadata: Vec<crate::MetadataSegment<'_>>,
        options: crate::PyramidOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> AbbreviatedWriteResult<AbbreviatedBytes> {
        let image = self.prepare(crate::build_hierarchical_planes(
            header,
            components,
            source_metadata,
            options,
        ))?;
        self.emit(options.max_output_bytes, |sink, writer| {
            crate::hierarchical_encode::write_hierarchy_to_sink(
                &image,
                options.limits,
                options.max_output_bytes,
                options.omit_dhp_height,
                metadata,
                |i, _| Ok(options.levels[i].coding),
                writer,
                sink,
            )
        })
    }
    /// Build a hierarchy in memory from a raster, then emit a pair.
    pub fn encode_pyramid_samples(
        &mut self,
        input: &crate::EncodeInput<'_>,
        raster: crate::PyramidRasterOptions,
        options: crate::PyramidOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> AbbreviatedWriteResult<AbbreviatedBytes> {
        let image = self.prepare(crate::build_hierarchical_samples(input, raster, options))?;
        self.emit(options.max_output_bytes, |sink, writer| {
            crate::hierarchical_encode::write_hierarchy_to_sink(
                &image,
                options.limits,
                options.max_output_bytes,
                options.omit_dhp_height,
                metadata,
                |i, _| Ok(options.levels[i].coding),
                writer,
                sink,
            )
        })
    }
    /// Build a hierarchy in memory from a shared image, then emit a pair.
    pub fn encode_pyramid_image(
        &mut self,
        image: &deadlib_image::Image,
        raster: crate::PyramidRasterOptions,
        options: crate::PyramidOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> AbbreviatedWriteResult<AbbreviatedBytes> {
        let input = self.prepare(crate::pixel_encode::shared_input(image))?;
        self.encode_pyramid_samples(&input, raster, options, metadata)
    }
}
