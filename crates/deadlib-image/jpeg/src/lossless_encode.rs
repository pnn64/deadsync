//! Native sample encoder, T.81 Annex H (no DCT or source raster copy).
use crate::arithmetic_encode::Coder;
use crate::encode::{emit_additional, emit_metadata, validate_metadata};
use crate::huffman_encode::Codebook;
use crate::lossless::{predictor, sampling};
use crate::lossless_arithmetic::{History, Statistics};
use crate::output::{Output, Sink};
use crate::{
    ArithmeticConditioning, CmykPolarity, ColorSpace, ComponentPlane, ComponentSpec, EncodeInput,
    EntropyCoding, Error, FrameHeader, Limits, LosslessImage, MetadataOptions, MetadataSegment,
    Process, Result, SampleData,
};
use deadlib_image::{ColorType, Image, Samples};

/// One lossless output scan. IDs within the scan follow frame order. Separate
/// scans may visit components in any order; each component is transmitted once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LosslessScan {
    pub components: [u8; 4],
    pub component_count: u8,
    pub predictor: u8,
    pub point_transform: u8,
    /// Whole MCU rows between restart markers. Zero disables restarts.
    pub restart_rows: u16,
}
impl LosslessScan {
    pub fn new(
        components: &[u8],
        predictor: u8,
        point_transform: u8,
        restart_rows: u16,
    ) -> Result<Self> {
        if !(1..=4).contains(&components.len()) {
            return Err(Error::Invalid("lossless scan component count"));
        }
        let mut ids = [0; 4];
        ids[..components.len()].copy_from_slice(components);
        Ok(Self {
            components: ids,
            component_count: components.len() as u8,
            predictor,
            point_transform,
            restart_rows,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LosslessOptions<'a> {
    /// Selection value 1..7 for generated scans.
    pub predictor: u8,
    /// 0 preserves all source bits. Higher Pt values discard low bits.
    pub point_transform: u8,
    pub restart_rows: u16,
    pub optimize_huffman: bool,
    /// Declare SOF's final height directly, or use first-scan DNL.
    pub height_declaration: crate::HeightDeclaration,
    pub entropy_coding: EntropyCoding,
    /// Arithmetic L/U conditioning. The AC split is unused by lossless scans.
    pub arithmetic_conditioning: ArithmeticConditioning,
    /// Generate one scan per component rather than packing legal MCUs.
    pub separate_components: bool,
    /// Borrowed explicit scans override generated predictor/Pt/restart controls.
    pub scan_script: &'a [LosslessScan],
    /// Raw CMYK sample convention; values are encoded without inversion.
    pub cmyk_polarity: CmykPolarity,
    pub limits: Limits,
    pub max_output_bytes: usize,
}
impl Default for LosslessOptions<'_> {
    fn default() -> Self {
        Self {
            predictor: 1,
            point_transform: 0,
            restart_rows: 0,
            optimize_huffman: true,
            height_declaration: crate::HeightDeclaration::FrameHeader,
            entropy_coding: EntropyCoding::Huffman,
            arithmetic_conditioning: ArithmeticConditioning::default(),
            separate_components: false,
            scan_script: &[],
            cmyk_polarity: CmykPolarity::Conventional,
            limits: Limits::default(),
            max_output_bytes: 512 * 1024 * 1024,
        }
    }
}

/// Encode shared grayscale/RGB at its original 2..16-bit precision. There is
/// no implicit color conversion, scaling or downsampling. Alpha/indexed images
/// require explicit caller conversion. One-bit source precision is not JPEG-1.
pub fn encode_lossless(image: &Image, options: LosslessOptions) -> Result<Vec<u8>> {
    encode_lossless_with_metadata(image, options, MetadataOptions::default())
}
pub fn encode_lossless_with_metadata(
    image: &Image,
    options: LosslessOptions,
    metadata: MetadataOptions<'_>,
) -> Result<Vec<u8>> {
    image
        .validate()
        .map_err(|_| Error::Invalid("lossless shared image"))?;
    let color_space = match image.color_type {
        ColorType::Grayscale => ColorSpace::Grayscale,
        ColorType::Rgb => ColorSpace::Rgb,
        _ => return Err(Error::Unsupported("lossless shared color type")),
    };
    let input = EncodeInput {
        width: u16::try_from(image.width)
            .map_err(|_| Error::Invalid("lossless width exceeds 65535"))?,
        height: u16::try_from(image.height)
            .map_err(|_| Error::Invalid("lossless height exceeds 65535"))?,
        precision: image.bit_depth,
        color_space,
        samples: match &image.samples {
            Samples::U8(v) => SampleData::U8(v),
            Samples::U16(v) => SampleData::U16(v),
        },
    };
    encode_lossless_samples_with_metadata(&input, options, metadata)
}

/// Encode interleaved native integers in gray/RGB/YCbCr/CMYK/YCCK form. No
/// source samples are transformed (except explicit Pt), scaled or copied.
pub fn encode_lossless_samples(
    input: &EncodeInput<'_>,
    options: LosslessOptions,
) -> Result<Vec<u8>> {
    encode_lossless_samples_with_metadata(input, options, MetadataOptions::default())
}
pub fn encode_lossless_samples_with_metadata(
    input: &EncodeInput<'_>,
    options: LosslessOptions,
    metadata: MetadataOptions<'_>,
) -> Result<Vec<u8>> {
    encode_samples_with_tables(
        input,
        options,
        metadata,
        &mut crate::abbreviated::TableWriter::interchange(),
    )
}
pub(crate) fn encode_samples_with_tables(
    input: &EncodeInput<'_>,
    options: LosslessOptions<'_>,
    metadata: MetadataOptions<'_>,
    tables: &mut crate::abbreviated::TableWriter<'_>,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    write_samples_with_sink(input, options, metadata, tables, &mut bytes)?;
    Ok(bytes)
}
pub(crate) fn write_samples_with_sink<S: Sink>(
    input: &EncodeInput<'_>,
    options: LosslessOptions<'_>,
    metadata: MetadataOptions<'_>,
    tables: &mut crate::abbreviated::TableWriter<'_>,
    sink: &mut S,
) -> Result<()> {
    let count = match input.color_space {
        ColorSpace::Grayscale => 1,
        ColorSpace::Rgb | ColorSpace::YCbCr => 3,
        ColorSpace::Cmyk | ColorSpace::Ycck => 4,
        _ => return Err(Error::Unsupported("lossless input color space")),
    };
    if input.width == 0 || input.height == 0 || !(2..=16).contains(&input.precision) {
        return Err(Error::Invalid("lossless input dimensions or precision"));
    }
    let pixels = input.width as usize * input.height as usize;
    if pixels > options.limits.max_pixels {
        return Err(Error::LimitExceeded("lossless pixels"));
    }
    let maximum = (1u32 << input.precision) - 1;
    let sample_count = pixels
        .checked_mul(count)
        .ok_or(Error::LimitExceeded("lossless input samples"))?;
    let valid = match input.samples {
        SampleData::U8(v) => {
            input.precision <= 8
                && v.len() == sample_count
                && v.iter().all(|&v| u32::from(v) <= maximum)
        }
        SampleData::U16(v) => v.len() == sample_count && v.iter().all(|&v| u32::from(v) <= maximum),
    };
    if !valid {
        return Err(Error::Invalid("lossless input sample count or range"));
    }
    let ids = match input.color_space {
        ColorSpace::Rgb => *b"RGB\0",
        ColorSpace::Cmyk => *b"CMYK",
        _ => [1, 2, 3, 4],
    };
    let mut specs = Vec::new();
    specs
        .try_reserve_exact(count)
        .map_err(|_| Error::Allocation)?;
    for &id in &ids[..count] {
        specs.push(ComponentSpec {
            id,
            horizontal_sampling: 1,
            vertical_sampling: 1,
            quantization_table: 0,
        });
    }
    let frame = FrameHeader {
        process: Process::Lossless,
        precision: input.precision,
        width: input.width,
        height: input.height,
        components: specs,
    };
    // Adobe hints only label the original integers. Lossless output is not JFIF
    // and must not silently convert RGB or invert conventional CMYK.
    let transform = match input.color_space {
        ColorSpace::Rgb => Some(0),
        ColorSpace::YCbCr => Some(1),
        ColorSpace::Ycck => Some(2),
        ColorSpace::Cmyk if options.cmyk_polarity == CmykPolarity::AdobeInverted => Some(0),
        _ => None,
    };
    let adobe = crate::Adobe {
        version: 100,
        flags0: 0,
        flags1: 0,
        transform: transform.unwrap_or(0),
    }
    .to_bytes();
    let segments = [MetadataSegment {
        marker: 0xee,
        data: &adobe,
        offset: 0,
        preceding_scans: 0,
    }];
    let segments = if transform.is_some() {
        &segments[..]
    } else {
        &[]
    };
    let stored = count * core::mem::size_of::<ComponentSpec>();
    encode_native(
        &frame,
        Source::Interleaved {
            samples: input.samples,
            width: input.width as usize,
            height: input.height as usize,
            count,
        },
        segments,
        stored,
        options,
        metadata,
        tables,
        sink,
    )
}

pub(crate) fn encode_planes(
    image: &LosslessImage<'_>,
    options: LosslessOptions,
    metadata: MetadataOptions<'_>,
) -> Result<Vec<u8>> {
    encode_planes_with_tables(
        image,
        options,
        metadata,
        &mut crate::abbreviated::TableWriter::interchange(),
    )
}
pub(crate) fn encode_planes_with_tables(
    image: &LosslessImage<'_>,
    options: LosslessOptions<'_>,
    metadata: MetadataOptions<'_>,
    tables: &mut crate::abbreviated::TableWriter<'_>,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    write_planes_with_sink(image, options, metadata, tables, &mut bytes)?;
    Ok(bytes)
}
pub(crate) fn write_planes_with_sink<S: Sink>(
    image: &LosslessImage<'_>,
    mut options: LosslessOptions<'_>,
    metadata: MetadataOptions<'_>,
    tables: &mut crate::abbreviated::TableWriter<'_>,
    sink: &mut S,
) -> Result<()> {
    let stored = image.validate()?.max(image.memory_bytes);
    options.limits = intersect(options.limits, image.limits);
    if image.frame.width as usize * image.frame.height as usize > options.limits.max_pixels {
        return Err(Error::LimitExceeded("lossless pixels"));
    }
    encode_native(
        &image.frame,
        Source::Planes(&image.components),
        &image.metadata,
        stored,
        options,
        metadata,
        tables,
        sink,
    )
}
pub(crate) fn intersect(a: Limits, b: Limits) -> Limits {
    Limits {
        max_pixels: a.max_pixels.min(b.max_pixels),
        max_memory_bytes: a.max_memory_bytes.min(b.max_memory_bytes),
        max_metadata_bytes: a.max_metadata_bytes.min(b.max_metadata_bytes),
        max_scans: a.max_scans.min(b.max_scans),
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Source<'a> {
    Planes(&'a [ComponentPlane]),
    Interleaved {
        samples: SampleData<'a>,
        width: usize,
        height: usize,
        count: usize,
    },
}
impl Source<'_> {
    fn at(self, i: usize, x: usize, y: usize) -> i32 {
        match self {
            Self::Planes(p) => {
                let c = &p[i];
                i32::from(c.samples[y.min(c.height - 1) * c.width + x.min(c.width - 1)])
            }
            Self::Interleaved {
                samples,
                width,
                height,
                count,
            } => {
                let offset = (y.min(height - 1) * width + x.min(width - 1)) * count + i;
                match samples {
                    SampleData::U8(v) => i32::from(v[offset]),
                    SampleData::U16(v) => i32::from(v[offset]),
                }
            }
        }
    }
}
#[derive(Clone, Copy)]
struct Scan {
    indices: [usize; 4],
    count: usize,
    predictor: u8,
    point: u8,
    restart_rows: u16,
}
struct Plan<'a> {
    frame: &'a FrameHeader,
    options: LosslessOptions<'a>,
    count: usize,
}
impl<'a> Plan<'a> {
    fn new(frame: &'a FrameHeader, options: LosslessOptions<'a>) -> Result<Self> {
        let mut plan = Self {
            frame,
            options,
            count: options.scan_script.len(),
        };
        if options.scan_script.is_empty() {
            let mut start = 0;
            while start < frame.components.len() {
                start += plan.generated(start).count;
                plan.count += 1;
            }
        }
        if plan.count > options.limits.max_scans {
            return Err(Error::LimitExceeded("lossless encoder scans"));
        }
        let mut seen = [false; 255];
        for s in 0..plan.count {
            let scan = plan.get(s)?;
            if (if frame.process.is_differential() {
                scan.predictor != 0
            } else {
                !(1..=7).contains(&scan.predictor)
            }) || scan.point >= frame.precision
            {
                return Err(Error::Invalid(
                    "lossless encoder predictor or point transform",
                ));
            }
            let mut samples = 0;
            for (j, &i) in scan.indices[..scan.count].iter().enumerate() {
                if seen[i] {
                    return Err(Error::Invalid("duplicate lossless encoder component"));
                }
                if j != 0 && i < scan.indices[j - 1] {
                    return Err(Error::Invalid("lossless encoder component order"));
                }
                seen[i] = true;
                let c = frame.components[i];
                samples += c.horizontal_sampling as usize * c.vertical_sampling as usize;
            }
            if scan.count > 1 && samples > 10 {
                return Err(Error::Invalid("lossless encoder MCU exceeds ten samples"));
            }
            geometry(frame, scan)?;
        }
        if seen[..frame.components.len()].iter().any(|&v| !v) {
            return Err(Error::Invalid("lossless encoder script misses a component"));
        }
        Ok(plan)
    }
    fn generated(&self, start: usize) -> Scan {
        let mut count = 0;
        let mut samples = 0;
        let mut indices = [0; 4];
        for (i, c) in self.frame.components.iter().enumerate().skip(start).take(4) {
            let units = c.horizontal_sampling as usize * c.vertical_sampling as usize;
            if count != 0 && (self.options.separate_components || samples + units > 10) {
                break;
            }
            indices[count] = i;
            count += 1;
            samples += units;
        }
        Scan {
            indices,
            count,
            predictor: if self.frame.process.is_differential() {
                0
            } else {
                self.options.predictor
            },
            point: self.options.point_transform,
            restart_rows: self.options.restart_rows,
        }
    }
    fn get(&self, index: usize) -> Result<Scan> {
        if self.options.scan_script.is_empty() {
            let mut start = 0;
            for _ in 0..index {
                start += self.generated(start).count;
            }
            return Ok(self.generated(start));
        }
        let spec = self.options.scan_script[index];
        let count = spec.component_count as usize;
        if !(1..=4).contains(&count) {
            return Err(Error::Invalid("lossless encoder scan component count"));
        }
        let mut indices = [0; 4];
        for (j, &id) in spec.components[..count].iter().enumerate() {
            indices[j] = self
                .frame
                .components
                .iter()
                .position(|c| c.id == id)
                .ok_or(Error::Invalid("lossless encoder scan component ID"))?;
        }
        Ok(Scan {
            indices,
            count,
            predictor: spec.predictor,
            point: spec.point_transform,
            restart_rows: spec.restart_rows,
        })
    }
}

fn geometry(frame: &FrameHeader, scan: Scan) -> Result<(usize, usize, u16)> {
    let (max_h, max_v) = sampling(frame);
    let (cols, rows) = if scan.count == 1 {
        let spec = frame.components[scan.indices[0]];
        (
            (frame.width as usize * spec.horizontal_sampling as usize).div_ceil(max_h),
            (frame.height as usize * spec.vertical_sampling as usize).div_ceil(max_v),
        )
    } else {
        (
            (frame.width as usize).div_ceil(max_h),
            (frame.height as usize).div_ceil(max_v),
        )
    };
    let interval = cols
        .checked_mul(scan.restart_rows as usize)
        .and_then(|n| u16::try_from(n).ok())
        .ok_or(Error::Invalid("lossless restart interval exceeds 65535"))?;
    Ok((cols, rows, interval))
}

enum Token {
    Difference {
        component: usize,
        local: usize,
        value: i32,
        size: u8,
        x: usize,
        y: usize,
        first_row: bool,
    },
    Restart(u8),
}
fn walk(
    frame: &FrameHeader,
    source: Source<'_>,
    scan: Scan,
    emit: &mut impl FnMut(Token) -> Result<()>,
) -> Result<()> {
    let (cols, rows, interval) = geometry(frame, scan)?;
    let initial = 1i32 << (frame.precision - scan.point - 1);
    let mut first_row = 0;
    let mut restart = 0;
    for mcu in 0..cols * rows {
        if interval != 0 && mcu != 0 && mcu % interval as usize == 0 {
            emit(Token::Restart(restart))?;
            restart = (restart + 1) & 7;
            first_row = mcu / cols;
        }
        for (local, &i) in scan.indices[..scan.count].iter().enumerate() {
            let spec = frame.components[i];
            let (h, v) = if scan.count == 1 {
                (1, 1)
            } else {
                (
                    spec.horizontal_sampling as usize,
                    spec.vertical_sampling as usize,
                )
            };
            for yy in 0..v {
                for xx in 0..h {
                    let x = mcu % cols * h + xx;
                    let y = mcu / cols * v + yy;
                    let get = |x, y| {
                        let value = source.at(i, x, y);
                        if frame.process.is_differential() {
                            i32::from(value as i16) / (1i32 << scan.point)
                        } else {
                            value >> scan.point
                        }
                    };
                    let a = if x == 0 { 0 } else { get(x - 1, y) };
                    let b = if y == 0 { 0 } else { get(x, y - 1) };
                    let c = if x == 0 || y == 0 {
                        0
                    } else {
                        get(x - 1, y - 1)
                    };
                    let prediction = if frame.process.is_differential() {
                        0
                    } else if y == first_row * v {
                        if x == 0 { initial } else { a }
                    } else if x == 0 {
                        b
                    } else {
                        predictor(scan.predictor, a, b, c)
                    };
                    let difference = i32::from((get(x, y) - prediction) as i16);
                    let size = (32 - difference.unsigned_abs().leading_zeros()) as u8;
                    emit(Token::Difference {
                        component: i,
                        local,
                        value: difference,
                        size,
                        x,
                        y,
                        first_row: y == first_row * v,
                    })?;
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn encode_native<S: Sink>(
    frame: &FrameHeader,
    source: Source<'_>,
    segments: &[MetadataSegment<'_>],
    stored: usize,
    options: LosslessOptions<'_>,
    metadata: MetadataOptions<'_>,
    tables: &mut crate::abbreviated::TableWriter<'_>,
    sink: &mut S,
) -> Result<()> {
    if stored > options.limits.max_memory_bytes {
        return Err(Error::LimitExceeded("lossless encoder storage"));
    }
    validate_metadata(segments, options.limits, metadata)?;
    let (_, history_bytes) = frame_requirements(frame, options)?;
    let available = options.limits.max_memory_bytes - stored;
    if history_bytes > available {
        return Err(Error::LimitExceeded("lossless arithmetic history"));
    }
    let additional = validate_metadata(&[], options.limits, metadata)?;
    let output_memory = available
        .checked_sub(history_bytes)
        .and_then(|n| n.checked_sub(additional))
        .and_then(|n| {
            n.checked_sub(if options.entropy_coding == EntropyCoding::Huffman {
                512
            } else {
                0
            })
        })
        .ok_or(Error::LimitExceeded("lossless encoder storage"))?;
    let mut out = Output::with_sink(sink, options.max_output_bytes, output_memory)?;
    tables.prepare(&mut out)?;
    out.marker(0xd8)?;
    let mut metadata_index = 0;
    emit_metadata(&mut out, segments, &mut metadata_index, 0, metadata.icc)?;
    emit_additional(&mut out, metadata)?;
    encode_frame(
        &mut out,
        frame,
        source,
        options,
        false,
        &mut |out, scan| emit_metadata(out, segments, &mut metadata_index, scan, metadata.icc),
        tables,
    )?;
    emit_metadata(
        &mut out,
        segments,
        &mut metadata_index,
        usize::MAX,
        metadata.icc,
    )?;
    out.marker(0xd9)?;
    Ok(())
}

pub(crate) fn frame_requirements(
    frame: &FrameHeader,
    options: LosslessOptions<'_>,
) -> Result<(usize, usize)> {
    let plan = Plan::new(frame, options)?;
    let mut history_bytes = 0;
    if options.entropy_coding == EntropyCoding::Arithmetic {
        let conditioning = options.arithmetic_conditioning;
        if conditioning.dc_lower > conditioning.dc_upper || conditioning.dc_upper > 15 {
            return Err(Error::Invalid("lossless arithmetic conditioning"));
        }
        for s in 0..plan.count {
            let scan = plan.get(s)?;
            history_bytes =
                history_bytes.max(History::bytes(&history_shapes(frame, scan)?[..scan.count])?);
        }
    }
    Ok((plan.count, history_bytes))
}

/// Resolved per-component predictors and point transforms, before allocating
/// pyramid samples. Geometry depends only on the validated frame header.
pub(crate) fn frame_coding(
    frame: &FrameHeader,
    options: LosslessOptions<'_>,
) -> Result<([crate::LosslessCoding; 255], usize)> {
    let plan = Plan::new(frame, options)?;
    let mut coding = [crate::LosslessCoding::default(); 255];
    for s in 0..plan.count {
        let scan = plan.get(s)?;
        for &i in &scan.indices[..scan.count] {
            coding[i] = crate::LosslessCoding {
                predictor: scan.predictor,
                point_transform: scan.point,
            };
        }
    }
    Ok((coding, plan.count))
}

/// Emit one predictive frame directly into the caller's bounded JPEG output.
pub(crate) fn encode_frame<S: Sink>(
    out: &mut Output<S>,
    frame: &FrameHeader,
    source: Source<'_>,
    options: LosslessOptions<'_>,
    reset_restart: bool,
    before_scan: &mut impl FnMut(&mut Output<S>, usize) -> Result<()>,
    tables: &mut crate::abbreviated::TableWriter<'_>,
) -> Result<usize> {
    let plan = Plan::new(frame, options)?;
    let (_, history_bytes) = frame_requirements(frame, options)?;
    let arithmetic = options.entropy_coding == EntropyCoding::Arithmetic;
    let conditioning = options.arithmetic_conditioning;
    let mut histograms = [[0u64; 256]; 2];
    if !arithmetic && options.optimize_huffman {
        for s in 0..plan.count {
            walk(frame, source, plan.get(s)?, &mut |token| {
                if let Token::Difference {
                    component, size, ..
                } = token
                {
                    histograms[usize::from(component != 0)][size as usize] += 1;
                }
                Ok(())
            })?;
        }
    }
    let make = |i: usize| {
        if options.optimize_huffman && histograms[i].iter().any(|&v| v != 0) {
            Codebook::optimized(&histograms[i])
        } else {
            Codebook::lossless_fixed()
        }
    };
    let books = if arithmetic {
        None
    } else {
        Some([make(0)?, make(1)?])
    };
    let mut header = [0; 771];
    header[0] = frame.precision;
    header[1..3].copy_from_slice(
        &options
            .height_declaration
            .initial(frame.height)
            .to_be_bytes(),
    );
    header[3..5].copy_from_slice(&frame.width.to_be_bytes());
    header[5] = frame.components.len() as u8;
    for (i, c) in frame.components.iter().enumerate() {
        header[6 + 3 * i] = c.id;
        header[7 + 3 * i] = (c.horizontal_sampling << 4) | c.vertical_sampling;
    }
    out.segment(
        if frame.process.is_differential() {
            if arithmetic { 0xcf } else { 0xc7 }
        } else if arithmetic {
            0xcb
        } else {
            0xc3
        },
        &header[..6 + 3 * frame.components.len()],
    )?;
    if let Some(books) = &books {
        tables.huffman(
            out,
            crate::HuffmanClass::Dc,
            0,
            &books[0].counts,
            &books[0].symbols,
        )?;
        if frame.components.len() > 1 {
            tables.huffman(
                out,
                crate::HuffmanClass::Dc,
                1,
                &books[1].counts,
                &books[1].symbols,
            )?;
        }
    } else {
        let mut dac = [0; 4];
        let groups = if frame.components.len() == 1 { 1 } else { 2 };
        for i in 0..groups {
            dac[2 * i] = i as u8;
            dac[2 * i + 1] = conditioning.dc_lower | (conditioning.dc_upper << 4);
        }
        out.segment(0xcc, &dac[..2 * groups])?;
    }
    let mut previous_interval = if reset_restart { None } else { Some(0) };
    for s in 0..plan.count {
        before_scan(out, s)?;
        tables.scan_started();
        let scan = plan.get(s)?;
        let (_, _, interval) = geometry(frame, scan)?;
        if Some(interval) != previous_interval {
            out.segment(0xdd, &interval.to_be_bytes())?;
            previous_interval = Some(interval);
        }
        let mut sos = [0; 12];
        sos[0] = scan.count as u8;
        for (j, &i) in scan.indices[..scan.count].iter().enumerate() {
            sos[1 + 2 * j] = frame.components[i].id;
            sos[2 + 2 * j] = if i == 0 { 0 } else { 0x10 };
        }
        sos[1 + 2 * scan.count] = scan.predictor;
        sos[3 + 2 * scan.count] = scan.point;
        out.segment(0xda, &sos[..4 + 2 * scan.count])?;
        if let Some(books) = &books {
            walk(frame, source, scan, &mut |token| match token {
                Token::Restart(i) => {
                    out.pad()?;
                    out.marker(0xd0 + i)
                }
                Token::Difference {
                    component,
                    value: difference,
                    size,
                    ..
                } => {
                    books[usize::from(component != 0)].write(out, size)?;
                    if size != 0 && size != 16 {
                        let amplitude = (if difference < 0 {
                            difference - 1
                        } else {
                            difference
                        }) as u32
                            & ((1u32 << size) - 1);
                        out.write_bits(amplitude, size)?;
                    }
                    Ok(())
                }
            })?;
            out.pad()?;
        } else {
            let mut history =
                History::new(&history_shapes(frame, scan)?[..scan.count], history_bytes)?;
            let mut statistics = Statistics::default();
            let mut coder = Coder::new(out);
            walk(frame, source, scan, &mut |token| match token {
                Token::Restart(index) => {
                    coder.restart(index)?;
                    statistics.reset();
                    Ok(())
                }
                Token::Difference {
                    component,
                    local,
                    value,
                    x,
                    y,
                    first_row,
                    ..
                } => {
                    let (left, above) = history.context(local, x, y, first_row);
                    let class = statistics.encode(
                        &mut coder,
                        usize::from(component != 0),
                        left,
                        above,
                        value,
                        conditioning.dc_lower,
                        conditioning.dc_upper,
                    )?;
                    history.set(local, x, y, class);
                    Ok(())
                }
            })?;
            coder.finish()?;
        }
        options
            .height_declaration
            .after_scan(out, frame.height, s)?;
    }
    Ok(plan.count)
}

fn history_shapes(frame: &FrameHeader, scan: Scan) -> Result<[(usize, usize); 4]> {
    let (cols, _, _) = geometry(frame, scan)?;
    let mut shapes = [(0, 0); 4];
    for (j, &i) in scan.indices[..scan.count].iter().enumerate() {
        let c = frame.components[i];
        shapes[j] = if scan.count == 1 {
            let (max_h, _) = sampling(frame);
            (
                (frame.width as usize * c.horizontal_sampling as usize).div_ceil(max_h),
                1,
            )
        } else {
            (
                cols * c.horizontal_sampling as usize,
                c.vertical_sampling as usize,
            )
        };
    }
    Ok(shapes)
}
