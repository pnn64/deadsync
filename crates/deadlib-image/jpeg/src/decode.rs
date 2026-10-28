use crate::arithmetic::{Arithmetic, Conditioning, Statistics};
use crate::entropy::{Bits, Huffman};
use crate::progressive::ProgressiveScan;
use crate::{
    ComponentSpec, Error, Event, FrameHeader, Limits, MarkerReader, MetadataSegment, Process,
    Result, ZIGZAG,
};

#[derive(Clone, Copy)]
struct Quantization {
    values: [u16; 64],
    wide: bool,
}

/// A component retains the padded MCU block grid and its original quantizer.
#[derive(Clone, Debug)]
pub struct CoefficientComponent {
    pub spec: ComponentSpec,
    pub width: usize,
    pub height: usize,
    pub blocks_wide: usize,
    pub blocks_high: usize,
    pub quantization: [u16; 64],
    /// Zig-zag band precision: -1 for unseen, otherwise the last scan's Al.
    pub successive_approximation: [i8; 64],
    pub blocks: Vec<[i16; 64]>,
}

#[derive(Clone, Debug)]
pub struct CoefficientImage<'a> {
    pub frame: FrameHeader,
    pub components: Vec<CoefficientComponent>,
    pub metadata: Vec<MetadataSegment<'a>>,
    pub scans: usize,
    pub consumed: usize,
    pub(crate) memory_bytes: usize,
    pub(crate) limits: Limits,
}

pub(crate) fn allocate_components(
    frame: &FrameHeader,
    limits: Limits,
) -> Result<(Vec<CoefficientComponent>, usize)> {
    let pixels = frame.width as usize * frame.height as usize;
    if pixels > limits.max_pixels {
        return Err(Error::LimitExceeded("pixels"));
    }
    let max_h = frame
        .components
        .iter()
        .map(|c| c.horizontal_sampling)
        .max()
        .unwrap() as usize;
    let max_v = frame
        .components
        .iter()
        .map(|c| c.vertical_sampling)
        .max()
        .unwrap() as usize;
    let cols = (frame.width as usize).div_ceil(8 * max_h);
    let rows = (frame.height as usize).div_ceil(8 * max_v);
    let mut bytes = 0usize;
    for c in &frame.components {
        let count = cols
            .checked_mul(rows)
            .and_then(|n| n.checked_mul(c.horizontal_sampling as usize))
            .and_then(|n| n.checked_mul(c.vertical_sampling as usize))
            .and_then(|n| n.checked_mul(128))
            .ok_or(Error::LimitExceeded("coefficient storage"))?;
        bytes = bytes
            .checked_add(count)
            .ok_or(Error::LimitExceeded("coefficient storage"))?;
    }
    if bytes > limits.max_memory_bytes {
        return Err(Error::LimitExceeded("coefficient storage"));
    }
    let mut components = Vec::new();
    components
        .try_reserve_exact(frame.components.len())
        .map_err(|_| Error::Allocation)?;
    for &spec in &frame.components {
        let blocks_wide = cols * spec.horizontal_sampling as usize;
        let blocks_high = rows * spec.vertical_sampling as usize;
        let mut blocks = Vec::new();
        blocks
            .try_reserve_exact(blocks_wide * blocks_high)
            .map_err(|_| Error::Allocation)?;
        blocks.resize(blocks_wide * blocks_high, [0; 64]);
        components.push(CoefficientComponent {
            spec,
            width: (frame.width as usize * spec.horizontal_sampling as usize).div_ceil(max_h),
            height: (frame.height as usize * spec.vertical_sampling as usize).div_ceil(max_v),
            blocks_wide,
            blocks_high,
            blocks,
            quantization: [0; 64],
            successive_approximation: [-1; 64],
        });
    }
    Ok((components, bytes))
}

/// Decode without IDCT or color conversion. APPn/COM payloads borrow the input.
/// Accepts sequential Huffman/arithmetic and progressive Huffman DCT frames; other processes are explicit
/// errors while their native implementations are in progress.
pub fn decode_coefficients(input: &[u8], limits: Limits) -> Result<CoefficientImage<'_>> {
    let mut reader = MarkerReader::new(input)?;
    let mut quant = [None; 4];
    let mut huffman: [Option<Huffman>; 8] = core::array::from_fn(|_| None);
    let mut conditioning = Conditioning::default();
    let mut frame = None;
    let mut components = Vec::new();
    let mut seen = Vec::new();
    let mut memory_bytes = 0usize;
    let mut metadata = Vec::new();
    let mut metadata_bytes = 0usize;
    let mut interval = 0;
    let mut scans = 0;
    let mut ended = false;
    for event in reader.by_ref() {
        match event? {
            Event::End { .. } => {
                ended = true;
                break;
            }
            Event::Segment {
                marker: 0xdb, data, ..
            } => parse_quant(data, &mut quant)?,
            Event::Segment {
                marker: 0xc4, data, ..
            } => parse_huffman(data, &mut huffman)?,
            Event::Segment {
                marker: 0xcc, data, ..
            } => conditioning.parse(data)?,
            Event::Segment {
                marker: 0xdd, data, ..
            } => {
                if data.len() != 2 {
                    return Err(Error::Invalid("DRI length"));
                }
                interval = u16::from_be_bytes([data[0], data[1]]) as usize;
            }
            Event::Segment {
                marker: marker @ (0xe0..=0xef | 0xfe),
                data,
                offset,
            } => {
                metadata_bytes = metadata_bytes
                    .checked_add(
                        data.len()
                            .saturating_add(core::mem::size_of::<MetadataSegment<'_>>()),
                    )
                    .ok_or(Error::LimitExceeded("metadata"))?;
                if metadata_bytes > limits.max_metadata_bytes {
                    return Err(Error::LimitExceeded("metadata"));
                }
                if memory_bytes
                    .checked_add(metadata_bytes)
                    .is_none_or(|n| n > limits.max_memory_bytes)
                {
                    return Err(Error::LimitExceeded("metadata and coefficients"));
                }
                metadata.try_reserve(1).map_err(|_| Error::Allocation)?;
                metadata.push(MetadataSegment {
                    marker,
                    data,
                    offset,
                    preceding_scans: scans,
                });
            }
            Event::Segment { marker, data, .. } if Process::try_from(marker).is_ok() => {
                if frame.is_some() {
                    return Err(Error::Invalid("multiple frames without hierarchy"));
                }
                let header = FrameHeader::parse(marker, data)?;
                if header.process != Process::Baseline
                    && header.process != Process::ExtendedSequential
                    && header.process != Process::Progressive
                    && header.process != Process::ArithmeticSequential
                {
                    return Err(Error::Unsupported("this JPEG process"));
                }
                if header.height == 0 {
                    return Err(Error::Unsupported("DNL height"));
                }
                let remaining = limits
                    .max_memory_bytes
                    .checked_sub(metadata_bytes)
                    .ok_or(Error::LimitExceeded("metadata and coefficients"))?;
                (components, memory_bytes) = allocate_components(
                    &header,
                    Limits {
                        max_memory_bytes: remaining,
                        ..limits
                    },
                )?;
                seen.resize(header.components.len(), false);
                frame = Some(header);
            }
            Event::Scan {
                header, entropy, ..
            } => {
                scans += 1;
                if scans > limits.max_scans {
                    return Err(Error::LimitExceeded("scans"));
                }
                let frame = frame.as_ref().ok_or(Error::Invalid("scan before frame"))?;
                decode_scan(
                    header,
                    entropy,
                    frame,
                    &mut components,
                    &mut seen,
                    &quant,
                    &huffman,
                    interval,
                    conditioning,
                )?;
            }
            Event::Segment { marker: 0xdc, .. } => return Err(Error::Unsupported("DNL height")),
            Event::Segment {
                marker: 0xde | 0xdf,
                ..
            } => return Err(Error::Unsupported("hierarchical process")),
            Event::Segment { .. } => return Err(Error::Invalid("reserved or unexpected marker")),
        }
        if memory_bytes
            .checked_add(metadata_bytes)
            .is_none_or(|n| n > limits.max_memory_bytes)
        {
            return Err(Error::LimitExceeded("metadata and coefficients"));
        }
    }
    if !ended {
        return Err(Error::Truncated);
    }
    let frame = frame.ok_or(Error::Invalid("missing frame"))?;
    if seen.iter().any(|&v| !v) {
        return Err(Error::Invalid("missing component scan"));
    }
    Ok(CoefficientImage {
        frame,
        components,
        metadata,
        scans,
        consumed: reader.consumed(),
        memory_bytes: memory_bytes + metadata_bytes,
        limits,
    })
}

fn parse_quant(mut data: &[u8], tables: &mut [Option<Quantization>; 4]) -> Result<()> {
    if data.is_empty() {
        return Err(Error::Invalid("empty DQT"));
    }
    while !data.is_empty() {
        let info = data[0];
        let precision = info >> 4;
        let id = (info & 15) as usize;
        if precision > 1 || id > 3 {
            return Err(Error::Invalid("DQT selector"));
        }
        let n = 64 * (precision as usize + 1);
        let values = data.get(1..1 + n).ok_or(Error::Invalid("DQT length"))?;
        let mut table = [0; 64];
        for (i, &z) in ZIGZAG.iter().enumerate() {
            let value = if precision == 0 {
                values[i] as u16
            } else {
                u16::from_be_bytes([values[2 * i], values[2 * i + 1]])
            };
            if value == 0 {
                return Err(Error::Invalid("zero quantizer"));
            }
            table[z] = value;
        }
        tables[id] = Some(Quantization {
            values: table,
            wide: precision == 1,
        });
        data = &data[1 + n..];
    }
    Ok(())
}

fn parse_huffman(mut data: &[u8], tables: &mut [Option<Huffman>; 8]) -> Result<()> {
    if data.is_empty() {
        return Err(Error::Invalid("empty DHT"));
    }
    while !data.is_empty() {
        if data.len() < 17 {
            return Err(Error::Invalid("DHT length"));
        }
        let class = data[0] >> 4;
        let id = data[0] & 15;
        if class > 1 || id > 3 {
            return Err(Error::Invalid("DHT selector"));
        }
        let counts: &[u8; 16] = data[1..17].try_into().unwrap();
        let n: usize = counts.iter().map(|&v| v as usize).sum();
        let values = data
            .get(17..17 + n)
            .ok_or(Error::Invalid("DHT symbol length"))?;
        tables[(class * 4 + id) as usize] = Some(Huffman::new(counts, values)?);
        data = &data[17 + n..];
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn decode_scan(
    header: &[u8],
    entropy: &[u8],
    frame: &FrameHeader,
    components: &mut [CoefficientComponent],
    seen: &mut [bool],
    quant: &[Option<Quantization>; 4],
    tables: &[Option<Huffman>; 8],
    interval: usize,
    conditioning: Conditioning,
) -> Result<()> {
    if header.len() < 4 {
        return Err(Error::Invalid("SOS length"));
    }
    let n = header[0] as usize;
    if !(1..=4).contains(&n) || header.len() != 4 + 2 * n {
        return Err(Error::Invalid("SOS components"));
    }
    let progression = if frame.process == Process::Progressive {
        Some(ProgressiveScan::parse(&header[1 + 2 * n..], n)?)
    } else {
        None
    };
    if progression.is_none() && header[1 + 2 * n..] != [0, 63, 0] {
        return Err(Error::Invalid("sequential scan parameters"));
    }
    let mut selected = [(0usize, 0usize, 0usize); 4];
    let mut block_count = 0;
    for (j, c) in header[1..1 + 2 * n].as_chunks::<2>().0.iter().enumerate() {
        let i = frame
            .components
            .iter()
            .position(|v| v.id == c[0])
            .ok_or(Error::Invalid("SOS component id"))?;
        let dc = (c[1] >> 4) as usize;
        let ac = (c[1] & 15) as usize;
        let max_table = if frame.process == Process::Baseline {
            1
        } else {
            3
        };
        let need_dc = progression.is_none_or(|s| s.ss == 0 && s.ah == 0);
        let need_ac = progression.is_none_or(|s| s.ss != 0);
        if dc > max_table
            || ac > max_table
            || (!frame.process.is_arithmetic() && need_dc && tables[dc].is_none())
            || (!frame.process.is_arithmetic() && need_ac && tables[4 + ac].is_none())
        {
            return Err(Error::Invalid("missing or invalid entropy table"));
        }
        if (progression.is_none() && seen[i]) || selected[..j].iter().any(|v| v.0 == i) {
            return Err(Error::Invalid("duplicate sequential component"));
        }
        selected[j] = (i, dc, 4 + ac);
        let spec = components[i].spec;
        let table = quant[spec.quantization_table as usize]
            .ok_or(Error::Invalid("missing quantization table"))?;
        if frame.process == Process::Baseline && table.wide {
            return Err(Error::Invalid("16-bit quantizer in baseline"));
        }
        if let Some(scan) = progression {
            if seen[i] && components[i].quantization != table.values {
                return Err(Error::Invalid(
                    "quantizer changed between progressive scans",
                ));
            }
            scan.advance(&mut components[i].successive_approximation)?;
        }
        components[i].quantization = table.values;
        block_count += spec.horizontal_sampling as usize * spec.vertical_sampling as usize;
    }
    if n > 1 && block_count > 10 {
        return Err(Error::Invalid("more than ten blocks per MCU"));
    }
    let max_h = frame
        .components
        .iter()
        .map(|c| c.horizontal_sampling)
        .max()
        .unwrap() as usize;
    let max_v = frame
        .components
        .iter()
        .map(|c| c.vertical_sampling)
        .max()
        .unwrap() as usize;
    let (cols, rows) = if n == 1 {
        let c = &components[selected[0].0];
        (c.width.div_ceil(8), c.height.div_ceil(8))
    } else {
        (
            (frame.width as usize).div_ceil(8 * max_h),
            (frame.height as usize).div_ceil(8 * max_v),
        )
    };
    let mut bits = Bits::new(entropy);
    let mut arithmetic = if frame.process.is_arithmetic() {
        Some((
            Arithmetic::new(entropy)?,
            Statistics::default(),
            [0usize; 4],
        ))
    } else {
        None
    };
    let mut predictors = [0i32; 4];
    let mut restart = 0;
    let mut eob_run = 0;
    for mcu in 0..cols * rows {
        if interval > 0 && mcu > 0 && mcu % interval == 0 {
            if eob_run != 0 {
                return Err(Error::Invalid("EOB run crosses restart"));
            }
            if let Some((coder, statistics, classes)) = &mut arithmetic {
                coder.restart(restart)?;
                statistics.reset();
                classes.fill(0);
            } else {
                bits.restart(restart)?;
            }
            restart = (restart + 1) & 7;
            predictors.fill(0);
        }
        for (j, &(i, dc, ac)) in selected[..n].iter().enumerate() {
            let c = &mut components[i];
            let (h, v) = if n == 1 {
                (1, 1)
            } else {
                (
                    c.spec.horizontal_sampling as usize,
                    c.spec.vertical_sampling as usize,
                )
            };
            for y in 0..v {
                for x in 0..h {
                    let bx = mcu % cols * h + x;
                    let by = mcu / cols * v + y;
                    let block = &mut c.blocks[by * c.blocks_wide + bx];
                    if let Some((coder, statistics, classes)) = &mut arithmetic {
                        statistics.sequential(
                            block,
                            coder,
                            dc,
                            ac - 4,
                            &mut classes[j],
                            &mut predictors[j],
                            conditioning,
                            frame.precision,
                        )?;
                    } else if let Some(scan) = progression {
                        scan.decode(
                            block,
                            &mut bits,
                            tables[dc].as_ref(),
                            tables[ac].as_ref(),
                            &mut predictors[j],
                            &mut eob_run,
                            frame.precision,
                        )?;
                    } else {
                        sequential_block(
                            block,
                            &mut bits,
                            tables[dc].as_ref().unwrap(),
                            tables[ac].as_ref().unwrap(),
                            &mut predictors[j],
                            frame.precision,
                        )?;
                    }
                }
            }
        }
    }
    if eob_run != 0 {
        return Err(Error::Invalid("EOB run exceeds scan"));
    }
    if let Some((coder, _, _)) = arithmetic {
        coder.finish()?;
    } else {
        bits.finish()?;
    }
    for &(i, _, _) in &selected[..n] {
        if progression.is_none_or(|s| s.ss == 0) {
            seen[i] = true;
        }
        if progression.is_none() {
            components[i].successive_approximation.fill(0);
        }
    }
    Ok(())
}

fn sequential_block(
    block: &mut [i16; 64],
    bits: &mut Bits<'_>,
    dc: &Huffman,
    ac: &Huffman,
    predictor: &mut i32,
    precision: u8,
) -> Result<()> {
    let size = dc.decode(bits)?;
    if size > precision + 3 {
        return Err(Error::Invalid("DC magnitude category"));
    }
    *predictor += bits.receive(size)?;
    if *predictor < -(1 << (precision + 3)) || *predictor >= 1 << (precision + 3) {
        return Err(Error::Invalid("DC coefficient range"));
    }
    block[0] = *predictor as i16;
    let mut k = 1;
    while k < 64 {
        let rs = ac.decode(bits)?;
        let run = (rs >> 4) as usize;
        let size = rs & 15;
        if size == 0 {
            if run == 0 {
                break;
            }
            if run != 15 {
                return Err(Error::Invalid("sequential AC run symbol"));
            }
            k += 16;
            if k > 64 {
                return Err(Error::Invalid("AC zero run exceeds block"));
            }
        } else {
            k += run;
            if k >= 64 || size > precision + 2 {
                return Err(Error::Invalid("AC run or magnitude"));
            }
            block[ZIGZAG[k]] = bits.receive(size)? as i16;
            k += 1;
        }
    }
    Ok(())
}
