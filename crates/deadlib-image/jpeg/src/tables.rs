//! Persistent DQT/DHT destinations and abbreviated streams, T.81 B.4/B.5.
use crate::decode::{parse_huffman, parse_quant};
use crate::entropy::Huffman;
use crate::output::{Output, Sink};
use crate::{Error, Event, Limits, MarkerReader, MetadataSegment, Result, ZIGZAG};

/// Natural-order quantizers, retaining the DQT representation's width.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuantizationTable {
    pub values: [u16; 64],
    pub wide: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum HuffmanClass {
    Dc = 0,
    Ac = 1,
}

/// A borrowed canonical Huffman definition; no decoder lookup-table copy.
#[derive(Clone, Copy, Debug)]
pub struct HuffmanTable<'a> {
    pub counts: &'a [u8; 16],
    pub symbols: &'a [u8],
}

/// Tables persist across abbreviated images. SOI resets arithmetic conditioning
/// and restart intervals, so those are deliberately absent from this cache.
/// Definitions update when processed; errors retain updates already made.
/// Clone explicitly before a call if rollback is required.
#[derive(Clone)]
pub struct Tables {
    pub(crate) quant: [Option<QuantizationTable>; 4],
    pub(crate) huffman: [Option<Huffman>; 8],
}
impl Default for Tables {
    fn default() -> Self {
        Self {
            quant: [None; 4],
            huffman: core::array::from_fn(|_| None),
        }
    }
}
impl core::fmt::Debug for Tables {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Tables")
            .field("quantization", &self.quant.each_ref().map(Option::is_some))
            .field("huffman", &self.huffman.each_ref().map(Option::is_some))
            .finish()
    }
}

/// Metadata belongs to this table stream, not automatically to later images.
#[derive(Debug)]
pub struct TableStream<'a> {
    pub metadata: Vec<MetadataSegment<'a>>,
    pub consumed: usize,
}

impl Tables {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn is_empty(&self) -> bool {
        self.quant.iter().all(Option::is_none) && self.huffman.iter().all(Option::is_none)
    }
    pub fn quantization(&self, id: u8) -> Option<&QuantizationTable> {
        self.quant.get(id as usize)?.as_ref()
    }
    pub fn huffman(&self, class: HuffmanClass, id: u8) -> Option<HuffmanTable<'_>> {
        if id > 3 {
            return None;
        }
        let (counts, symbols) = self.huffman[class as usize * 4 + id as usize]
            .as_ref()?
            .definition();
        Some(HuffmanTable { counts, symbols })
    }
    pub fn set_quantization(&mut self, id: u8, table: QuantizationTable) -> Result<()> {
        if id > 3
            || table.values.contains(&0)
            || (!table.wide && table.values.iter().any(|&v| v > 255))
        {
            return Err(Error::Invalid("quantization table definition"));
        }
        self.quant[id as usize] = Some(table);
        Ok(())
    }
    pub fn set_huffman(
        &mut self,
        class: HuffmanClass,
        id: u8,
        counts: [u8; 16],
        symbols: &[u8],
    ) -> Result<()> {
        if id > 3 {
            return Err(Error::Invalid("Huffman table destination"));
        }
        self.huffman[class as usize * 4 + id as usize] = Some(Huffman::new(&counts, symbols)?);
        Ok(())
    }

    /// Read one B.5 table-specification stream. Bytes after EOI are unconsumed.
    /// B.5 allows DQT/DHT/APPn/COM, not image scans, DAC, DRI, DNL or DHP.
    pub fn load<'a>(&mut self, input: &'a [u8], limits: Limits) -> Result<TableStream<'a>> {
        let mut reader = MarkerReader::new(input)?;
        let mut metadata = Vec::new();
        let mut bytes = 0usize;
        while let Some(event) = reader.next() {
            match event? {
                Event::Segment {
                    marker: 0xdb, data, ..
                } => parse_quant(data, &mut self.quant)?,
                Event::Segment {
                    marker: 0xc4, data, ..
                } => parse_huffman(data, &mut self.huffman)?,
                Event::Segment {
                    marker: marker @ (0xe0..=0xef | 0xfe),
                    data,
                    offset,
                } => {
                    bytes = bytes
                        .checked_add(data.len() + core::mem::size_of::<MetadataSegment<'_>>())
                        .ok_or(Error::LimitExceeded("table-stream metadata"))?;
                    if bytes > limits.max_metadata_bytes || bytes > limits.max_memory_bytes {
                        return Err(Error::LimitExceeded("table-stream metadata"));
                    }
                    metadata
                        .try_reserve_exact(1)
                        .map_err(|_| Error::Allocation)?;
                    metadata.push(MetadataSegment {
                        marker,
                        data,
                        offset,
                        preceding_scans: 0,
                    });
                }
                Event::End { .. } => {
                    return Ok(TableStream {
                        metadata,
                        consumed: reader.consumed(),
                    });
                }
                _ => return Err(Error::Invalid("marker in table-specification stream")),
            }
        }
        Err(Error::Truncated)
    }

    pub fn encode(&self, max_output_bytes: usize) -> Result<Vec<u8>> {
        self.encode_with_metadata(&[], Limits::default(), max_output_bytes)
    }
    pub fn encode_with_metadata(
        &self,
        metadata: &[MetadataSegment<'_>],
        limits: Limits,
        max_output_bytes: usize,
    ) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.write_with_sink(metadata, limits, max_output_bytes, &mut bytes)?;
        Ok(bytes)
    }
    pub(crate) fn write_with_sink<S: Sink>(
        &self,
        metadata: &[MetadataSegment<'_>],
        limits: Limits,
        max_output_bytes: usize,
        sink: &mut S,
    ) -> Result<()> {
        crate::encode::validate_metadata(metadata, limits, crate::MetadataOptions::default())?;
        if metadata.iter().any(|s| s.preceding_scans != 0) {
            return Err(Error::Invalid("table-stream metadata scan position"));
        }
        let mut out = Output::with_sink(sink, max_output_bytes, limits.max_memory_bytes)?;
        out.marker(0xd8)?;
        for segment in metadata {
            out.segment(segment.marker, segment.data)?;
        }
        for (id, table) in self.quant.iter().enumerate() {
            if let Some(table) = table {
                emit_quantization(&mut out, id as u8, table)?;
            }
        }
        for class in [HuffmanClass::Dc, HuffmanClass::Ac] {
            for id in 0..4 {
                if let Some(table) = self.huffman(class, id) {
                    emit_huffman(&mut out, class, id, table.counts, table.symbols)?;
                }
            }
        }
        out.marker(0xd9)?;
        Ok(())
    }

    pub fn decode_coefficients<'a>(
        &mut self,
        input: &'a [u8],
        limits: Limits,
    ) -> Result<crate::CoefficientImage<'a>> {
        crate::decode::decode_with_tables(input, limits, self)
    }
    pub fn decode_lossless<'a>(
        &mut self,
        input: &'a [u8],
        limits: Limits,
    ) -> Result<crate::LosslessImage<'a>> {
        crate::lossless::decode_with_tables(input, limits, self)
    }
    pub fn decode_hierarchical<'a>(
        &mut self,
        input: &'a [u8],
        limits: Limits,
    ) -> Result<crate::HierarchicalImage<'a>> {
        crate::hierarchical::decode_with_tables(input, limits, self)
    }
}

pub(crate) fn emit_quantization<S: Sink>(
    out: &mut Output<S>,
    id: u8,
    table: &QuantizationTable,
) -> Result<()> {
    let mut data = [0u8; 129];
    data[0] = id | if table.wide { 16 } else { 0 };
    let mut n = 1;
    for &k in &ZIGZAG {
        if table.wide {
            data[n..n + 2].copy_from_slice(&table.values[k].to_be_bytes());
            n += 2;
        } else {
            data[n] = table.values[k] as u8;
            n += 1;
        }
    }
    out.segment(0xdb, &data[..n])
}
pub(crate) fn emit_huffman<S: Sink>(
    out: &mut Output<S>,
    class: HuffmanClass,
    id: u8,
    counts: &[u8; 16],
    symbols: &[u8],
) -> Result<()> {
    out.segment_parts(0xc4, &[&[((class as u8) << 4) | id], counts, symbols])
}
