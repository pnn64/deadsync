//! Direct table/image emission without compressing or splitting a second JPEG.
use crate::output::{Output, Sink};
use crate::tables::{emit_huffman, emit_quantization};
use crate::{Error, HuffmanClass, QuantizationTable, Result, Tables};

/// Send tables first, then image data. An unchanged cache produces the valid
/// four-byte SOI/EOI table stream. Later in-image redefinitions stay in place.
#[derive(Debug)]
pub struct AbbreviatedImage {
    pub tables: Vec<u8>,
    pub image: Vec<u8>,
}

/// Encoder-side knowledge of the receiver's persistent table destinations.
/// Feed every successful pair to the same decoder cache, in order. Write
/// failures clear the cache so retries cannot omit untransmitted definitions.
#[derive(Debug)]
pub struct AbbreviatedEncoder {
    tables: Tables,
    pub max_table_bytes: usize,
}
impl Default for AbbreviatedEncoder {
    fn default() -> Self {
        Self {
            tables: Tables::new(),
            max_table_bytes: 65536,
        }
    }
}
impl AbbreviatedEncoder {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_tables(tables: Tables) -> Self {
        Self {
            tables,
            ..Self::default()
        }
    }
    pub fn tables(&self) -> &Tables {
        &self.tables
    }
    pub fn clear(&mut self) {
        self.tables.clear();
    }
    fn write(
        &mut self,
        emit: impl FnOnce(&mut TableWriter<'_>) -> Result<Vec<u8>>,
    ) -> Result<AbbreviatedImage> {
        let result = (|| {
            let mut writer = TableWriter::abbreviated(&mut self.tables, self.max_table_bytes)?;
            let image = emit(&mut writer)?;
            let tables = writer.finish()?;
            Ok(AbbreviatedImage { tables, image })
        })();
        if result.is_err() {
            self.tables.clear();
        }
        result
    }
    pub fn encode_coefficients(
        &mut self,
        image: &crate::CoefficientImage<'_>,
        options: crate::CodingOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> Result<AbbreviatedImage> {
        self.write(|writer| {
            crate::encode::encode_with_table_writer(image, options, metadata, writer)
        })
    }
    pub fn encode_samples(
        &mut self,
        input: &crate::EncodeInput<'_>,
        options: crate::EncodeOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> Result<AbbreviatedImage> {
        let image = crate::quantize(input, options)?;
        self.encode_coefficients(&image, options.coding, metadata)
    }
    pub fn encode_image(
        &mut self,
        image: &deadlib_image::Image,
        options: crate::EncodeOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> Result<AbbreviatedImage> {
        self.encode_samples(
            &crate::pixel_encode::shared_input(image)?,
            options,
            metadata,
        )
    }
    pub fn encode_lossless(
        &mut self,
        image: &crate::LosslessImage<'_>,
        options: crate::LosslessOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> Result<AbbreviatedImage> {
        self.write(|writer| {
            crate::lossless_encode::encode_planes_with_tables(image, options, metadata, writer)
        })
    }
    pub fn encode_lossless_samples(
        &mut self,
        input: &crate::EncodeInput<'_>,
        options: crate::LosslessOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> Result<AbbreviatedImage> {
        self.write(|writer| {
            crate::lossless_encode::encode_samples_with_tables(input, options, metadata, writer)
        })
    }
    pub fn encode_lossless_image(
        &mut self,
        image: &deadlib_image::Image,
        options: crate::LosslessOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> Result<AbbreviatedImage> {
        self.encode_lossless_samples(
            &crate::pixel_encode::shared_input(image)?,
            options,
            metadata,
        )
    }
    pub fn encode_hierarchical(
        &mut self,
        image: &crate::HierarchicalImage<'_>,
        options: crate::HierarchicalOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> Result<AbbreviatedImage> {
        self.write(|writer| {
            crate::hierarchical_encode::encode_with_table_writer(image, options, metadata, writer)
        })
    }
    pub fn encode_pyramid_samples(
        &mut self,
        input: &crate::EncodeInput<'_>,
        raster: crate::PyramidRasterOptions,
        options: crate::PyramidOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> Result<AbbreviatedImage> {
        let image = crate::build_hierarchical_samples(input, raster, options)?;
        self.write(|writer| {
            crate::hierarchical_encode::write_hierarchy_with_tables(
                &image,
                options.limits,
                options.max_output_bytes,
                options.omit_dhp_height,
                metadata,
                |i, _| Ok(options.levels[i].coding),
                writer,
            )
        })
    }
    pub fn encode_pyramid_planes(
        &mut self,
        header: crate::HierarchyHeader,
        components: &[crate::ComponentPlane],
        source_metadata: Vec<crate::MetadataSegment<'_>>,
        options: crate::PyramidOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> Result<AbbreviatedImage> {
        let image = crate::build_hierarchical_planes(header, components, source_metadata, options)?;
        self.write(|writer| {
            crate::hierarchical_encode::write_hierarchy_with_tables(
                &image,
                options.limits,
                options.max_output_bytes,
                options.omit_dhp_height,
                metadata,
                |i, _| Ok(options.levels[i].coding),
                writer,
            )
        })
    }
    pub fn encode_pyramid_image(
        &mut self,
        image: &deadlib_image::Image,
        raster: crate::PyramidRasterOptions,
        options: crate::PyramidOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> Result<AbbreviatedImage> {
        self.encode_pyramid_samples(
            &crate::pixel_encode::shared_input(image)?,
            raster,
            options,
            metadata,
        )
    }
}

pub(crate) struct TableWriter<'a, T: Sink = Vec<u8>> {
    cache: Option<&'a mut Tables>,
    packet: Option<Output<T>>,
    first_scan: bool,
}
impl<'a> TableWriter<'a> {
    pub fn interchange() -> Self {
        Self {
            cache: None,
            packet: None,
            first_scan: true,
        }
    }
    fn abbreviated(cache: &'a mut Tables, limit: usize) -> Result<Self> {
        if limit < 4 {
            return Err(Error::LimitExceeded("table stream bytes"));
        }
        let mut packet = Output::new(limit);
        packet.marker(0xd8)?;
        Ok(Self {
            cache: Some(cache),
            packet: Some(packet),
            first_scan: true,
        })
    }
    pub(crate) fn abbreviated_to<'s, S: Sink>(
        cache: &'a mut Tables,
        sink: &'s mut S,
        limit: usize,
        output_limit: usize,
    ) -> Result<TableWriter<'a, &'s mut S>> {
        if output_limit < 4 {
            return Err(Error::LimitExceeded("encoded bytes"));
        }
        if limit < 4 {
            return Err(Error::LimitExceeded("table stream bytes"));
        }
        // The paired streaming table destination is direct: no packet storage
        // competes with the source or image staging budget.
        let mut packet = Output::with_sink(sink, limit.min(output_limit), 0)?;
        packet.marker(0xd8)?;
        Ok(TableWriter {
            cache: Some(cache),
            packet: Some(packet),
            first_scan: true,
        })
    }
}
impl<T: Sink> TableWriter<'_, T> {
    pub fn prepare<S: Sink>(&self, out: &mut Output<S>) -> Result<()> {
        if self.packet.is_some() {
            out.reserve_external(4)?;
        }
        Ok(())
    }
    pub fn scan_started(&mut self) {
        self.first_scan = false;
    }
    fn initial_packet<S: Sink>(
        &mut self,
        image: &mut Output<S>,
        size: usize,
    ) -> Result<Option<&mut Output<T>>> {
        if self.first_scan
            && let Some(packet) = self.packet.as_mut()
        {
            image.reserve_external(size)?;
            return Ok(Some(packet));
        }
        Ok(None)
    }
    pub fn quantization<S: Sink>(
        &mut self,
        out: &mut Output<S>,
        id: u8,
        values: &[u16; 64],
        wide: bool,
    ) -> Result<()> {
        let table = QuantizationTable {
            values: *values,
            wide,
        };
        if self
            .cache
            .as_ref()
            .and_then(|c| c.quantization(id))
            .is_some_and(|old| old == &table)
        {
            return Ok(());
        }
        if let Some(packet) = self.initial_packet(out, 5 + 64 * if wide { 2 } else { 1 })? {
            emit_quantization(packet, id, &table)?;
        } else {
            emit_quantization(out, id, &table)?;
        }
        if let Some(cache) = self.cache.as_mut() {
            cache.set_quantization(id, table)?;
        }
        Ok(())
    }
    pub fn huffman<S: Sink>(
        &mut self,
        out: &mut Output<S>,
        class: HuffmanClass,
        id: u8,
        counts: &[u8; 16],
        symbols: &[u8],
    ) -> Result<()> {
        if self
            .cache
            .as_ref()
            .and_then(|c| c.huffman(class, id))
            .is_some_and(|old| old.counts == counts && old.symbols == symbols)
        {
            return Ok(());
        }
        if let Some(packet) = self.initial_packet(out, 21 + symbols.len())? {
            emit_huffman(packet, class, id, counts, symbols)?;
        } else {
            emit_huffman(out, class, id, counts, symbols)?;
        }
        if let Some(cache) = self.cache.as_mut() {
            cache.set_huffman(class, id, *counts, symbols)?;
        }
        Ok(())
    }
    pub(crate) fn finish(mut self) -> Result<T> {
        let packet = self
            .packet
            .as_mut()
            .ok_or(Error::Invalid("missing table stream output"))?;
        packet.marker(0xd9)?;
        Ok(self.packet.take().unwrap().bytes)
    }
}
