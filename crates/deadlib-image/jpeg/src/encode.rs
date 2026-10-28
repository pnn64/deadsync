use crate::arithmetic::Statistics;
use crate::arithmetic_encode::Coder;
use crate::huffman_encode::Codebook;
use crate::metadata::{ICC_IDENTIFIER, is_icc};
use crate::output::Output;
use crate::scan_encode::{Plan, Scan};
use crate::{
    ArithmeticConditioning, CoefficientImage, Error, ICC_CHUNK_BYTES, MAX_ICC_BYTES,
    MetadataSegment, Process, Result, ZIGZAG,
};

#[derive(Clone, Copy, Debug, Default)]
pub enum IccWrite<'a> {
    #[default]
    Preserve,
    Remove,
    Replace(&'a [u8]),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MetadataOptions<'a> {
    /// Extra borrowed APPn/COM segments before the first scan. Set each
    /// segment's preceding_scans to zero. Existing payloads remain unchanged.
    pub additional_segments: &'a [MetadataSegment<'a>],
    pub icc: IccWrite<'a>,
}

/// Entropy coder used for newly encoded JPEG scans.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EntropyCoding {
    #[default]
    Huffman,
    Arithmetic,
}

/// Entropy coding controls shared by pixel encoding and coefficient rewriting.
#[derive(Clone, Copy, Debug)]
pub struct CodingOptions<'a> {
    /// Progressive output; false selects sequential DCT scans.
    pub progressive: bool,
    /// Initial Al of automatically generated progressive scans, 0..=13.
    /// Zero selects full-precision spectral scans; higher values refine to Al=0.
    pub successive_approximation: u8,
    /// Explicit scan order/bands/precision. Empty uses generated scans. A
    /// progressive script may intentionally transmit a partial progression.
    pub scan_script: &'a [crate::ScanSpec],
    pub entropy_coding: EntropyCoding,
    /// Used only with arithmetic coding. Both encoder tables use these values.
    pub arithmetic_conditioning: ArithmeticConditioning,
    /// Used only with Huffman coding.
    pub optimize_huffman: bool,
    /// MCU count between restart markers; zero disables restarts.
    pub restart_interval: u16,
    /// Declare SOF's final height directly, or use first-scan DNL.
    pub height_declaration: crate::HeightDeclaration,
    pub max_output_bytes: usize,
}

impl Default for CodingOptions<'_> {
    fn default() -> Self {
        Self {
            progressive: false,
            successive_approximation: 0,
            scan_script: &[],
            entropy_coding: EntropyCoding::Huffman,
            arithmetic_conditioning: ArithmeticConditioning::default(),
            optimize_huffman: true,
            restart_interval: 0,
            height_declaration: crate::HeightDeclaration::FrameHeader,
            max_output_bytes: 512 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy)]
enum Token {
    Symbol(usize, u8),
    Bits(u32, u8),
    Restart(u8),
}

enum BlockToken<'a> {
    Block(usize, &'a [i16; 64]),
    Restart(u8),
}

/// Encode original quantized coefficients without an IDCT/FDCT or color
/// conversion. Quantizer values and APPn/COM payloads are preserved. Metadata
/// keeps its relative order; positions past the new last scan move before EOI.
/// Accepts one to four components from Huffman/arithmetic DCT frames.
pub fn encode_coefficients(
    image: &CoefficientImage<'_>,
    options: CodingOptions,
) -> Result<Vec<u8>> {
    encode_coefficients_with_metadata(image, options, MetadataOptions::default())
}

/// Rewrite coefficients with metadata additions and ICC preserve/remove/replace.
/// Replacement profiles are split into APP2 chunks before the first SOS.
pub fn encode_coefficients_with_metadata(
    image: &CoefficientImage<'_>,
    options: CodingOptions,
    metadata: MetadataOptions<'_>,
) -> Result<Vec<u8>> {
    encode_with_table_writer(
        image,
        options,
        metadata,
        &mut crate::abbreviated::TableWriter::interchange(),
    )
}
pub(crate) fn encode_with_table_writer(
    image: &CoefficientImage<'_>,
    options: CodingOptions<'_>,
    metadata: MetadataOptions<'_>,
    tables: &mut crate::abbreviated::TableWriter<'_>,
) -> Result<Vec<u8>> {
    if image.frame.process.is_differential() {
        return Err(Error::Unsupported(
            "differential frame requires a hierarchy",
        ));
    }
    let source = DctFrame {
        frame: &image.frame,
        components: &image.components,
    };
    validate_frame(source, image.limits)?;
    validate_metadata(&image.metadata, image.limits, metadata)?;
    let mut previous = 0;
    for segment in &image.metadata {
        if segment.preceding_scans < previous {
            return Err(Error::Invalid("encoder metadata order"));
        }
        previous = segment.preceding_scans;
    }
    let mut out = Output::new(options.max_output_bytes);
    tables.prepare(&mut out)?;
    out.marker(0xd8)?;
    let mut metadata_index = 0;
    emit_metadata(
        &mut out,
        &image.metadata,
        &mut metadata_index,
        0,
        metadata.icc,
    )?;
    emit_additional(&mut out, metadata)?;
    encode_frame(
        &mut out,
        source,
        options,
        image.limits,
        false,
        &mut |out, scan| {
            emit_metadata(
                out,
                &image.metadata,
                &mut metadata_index,
                scan,
                metadata.icc,
            )
        },
        tables,
    )?;
    emit_metadata(
        &mut out,
        &image.metadata,
        &mut metadata_index,
        usize::MAX,
        metadata.icc,
    )?;
    out.marker(0xd9)?;
    Ok(out.bytes)
}

#[derive(Clone, Copy)]
pub(crate) struct DctFrame<'a> {
    pub frame: &'a crate::FrameHeader,
    pub components: &'a [crate::CoefficientComponent],
}

pub(crate) fn frame_process(image: DctFrame<'_>, options: CodingOptions<'_>) -> Process {
    frame_process_for(
        image.frame.precision,
        image.frame.process.is_differential(),
        image
            .components
            .iter()
            .any(|c| c.quantization.iter().any(|&q| q > 255)),
        options,
    )
}
pub(crate) fn frame_process_for(
    precision: u8,
    differential: bool,
    wide: bool,
    options: CodingOptions<'_>,
) -> Process {
    let ordinary = match (options.entropy_coding, options.progressive) {
        (EntropyCoding::Arithmetic, true) => Process::ArithmeticProgressive,
        (EntropyCoding::Arithmetic, false) => Process::ArithmeticSequential,
        (EntropyCoding::Huffman, true) => Process::Progressive,
        (EntropyCoding::Huffman, false) => {
            if precision == 8 && !wide {
                Process::Baseline
            } else {
                Process::ExtendedSequential
            }
        }
    };
    if !differential {
        return ordinary;
    }
    match ordinary {
        Process::Baseline | Process::ExtendedSequential => Process::DifferentialSequential,
        Process::Progressive => Process::DifferentialProgressive,
        Process::ArithmeticSequential => Process::ArithmeticDifferentialSequential,
        Process::ArithmeticProgressive => Process::ArithmeticDifferentialProgressive,
        _ => unreachable!(),
    }
}

/// Write one frame directly into a shared JPEG output, with no SOI/EOI or
/// metadata copy. A hierarchy resets the inherited DRI before its first scan.
pub(crate) fn encode_frame(
    out: &mut Output,
    image: DctFrame<'_>,
    options: CodingOptions<'_>,
    limits: crate::Limits,
    reset_restart: bool,
    before_scan: &mut impl FnMut(&mut Output, usize) -> Result<()>,
    tables: &mut crate::abbreviated::TableWriter<'_>,
) -> Result<usize> {
    validate_frame(image, limits)?;
    let n = image.components.len();
    let plan = Plan::new(
        &image.frame.components,
        options.progressive,
        options.successive_approximation,
        options.scan_script,
        limits.max_scans,
    )?;
    let scan_count = plan.len();
    let books = if options.entropy_coding == EntropyCoding::Huffman {
        let mut histograms = [[0u64; 256]; 4];
        // Validate categories and collect frequencies before allocating output.
        for s in 0..scan_count {
            walk(
                image,
                plan.get(s)?,
                options.restart_interval,
                &mut |token| {
                    if let Token::Symbol(table, symbol) = token {
                        histograms[table][symbol as usize] += 1;
                    }
                    Ok(())
                },
            )?;
        }
        let make = |i: usize| {
            if options.optimize_huffman && histograms[i].iter().any(|&v| v != 0) {
                Codebook::optimized(&histograms[i])
            } else {
                Codebook::fixed(
                    i >= 2,
                    image.frame.precision,
                    options.progressive,
                    image.frame.process.is_differential(),
                )
            }
        };
        Some([make(0)?, make(1)?, make(2)?, make(3)?])
    } else {
        options.arithmetic_conditioning.validate()?;
        None
    };
    let mut quantizers = [[0u16; 64]; 4];
    let mut quantizer_count = 0;
    let mut qslots = [0u8; 4];
    for (i, c) in image.components.iter().enumerate() {
        let slot = if let Some(j) = quantizers[..quantizer_count]
            .iter()
            .position(|q| q == &c.quantization)
        {
            j
        } else {
            quantizers[quantizer_count] = c.quantization;
            quantizer_count += 1;
            quantizer_count - 1
        };
        qslots[i] = slot as u8;
    }
    let process = frame_process(image, options);
    for (i, q) in quantizers[..quantizer_count].iter().enumerate() {
        let wide = q.iter().any(|&v| v > 255);
        tables.quantization(out, i as u8, q, wide)?;
    }
    let mut header = [0u8; 18];
    header[0] = image.frame.precision;
    header[1..3].copy_from_slice(
        &options
            .height_declaration
            .initial(image.frame.height)
            .to_be_bytes(),
    );
    header[3..5].copy_from_slice(&image.frame.width.to_be_bytes());
    header[5] = n as u8;
    for (i, c) in image.components.iter().enumerate() {
        header[6 + 3 * i] = c.spec.id;
        header[7 + 3 * i] = (c.spec.horizontal_sampling << 4) | c.spec.vertical_sampling;
        header[8 + 3 * i] = qslots[i];
    }
    out.segment(process as u8, &header[..6 + 3 * n])?;
    let groups = if n == 1 { 1 } else { 2 };
    if let Some(books) = &books {
        for i in 0..groups {
            tables.huffman(
                out,
                crate::HuffmanClass::Dc,
                i as u8,
                &books[i].counts,
                &books[i].symbols,
            )?;
            tables.huffman(
                out,
                crate::HuffmanClass::Ac,
                i as u8,
                &books[2 + i].counts,
                &books[2 + i].symbols,
            )?;
        }
    } else {
        let c = options.arithmetic_conditioning;
        let mut data = [0; 8];
        for i in 0..groups {
            data[4 * i..4 * i + 4].copy_from_slice(&[
                i as u8,
                (c.dc_upper << 4) | c.dc_lower,
                16 | i as u8,
                c.ac_split,
            ]);
        }
        out.segment(0xcc, &data[..4 * groups])?;
    }
    if reset_restart || options.restart_interval != 0 {
        out.segment(0xdd, &options.restart_interval.to_be_bytes())?;
    }
    for s in 0..scan_count {
        before_scan(out, s)?;
        tables.scan_started();
        let scan = plan.get(s)?;
        let mut sos = [0u8; 12];
        sos[0] = scan.count as u8;
        for (position, &i) in scan.indices[..scan.count].iter().enumerate() {
            sos[1 + 2 * position] = image.components[i].spec.id;
            sos[2 + 2 * position] = if i == 0 { 0 } else { 0x11 };
        }
        let parameters = scan
            .progression
            .map_or([0, 63, 0], |s| [s.ss as u8, s.se as u8, (s.ah << 4) | s.al]);
        sos[1 + 2 * scan.count..4 + 2 * scan.count].copy_from_slice(&parameters);
        let length = 4 + 2 * scan.count;
        out.segment(0xda, &sos[..length])?;
        if let Some(books) = &books {
            walk(
                image,
                scan,
                options.restart_interval,
                &mut |token| match token {
                    Token::Symbol(table, symbol) => books[table].write(out, symbol),
                    Token::Bits(value, n) => out.write_bits(value, n),
                    Token::Restart(i) => {
                        out.pad()?;
                        out.marker(0xd0 + i)
                    }
                },
            )?;
            out.pad()?;
        } else {
            let mut coder = Coder::new(out);
            let mut statistics = Statistics::default();
            let mut predictors = [0; 4];
            let mut classes = [0; 4];
            walk_blocks(image, scan, options.restart_interval, &mut |token| {
                match token {
                    BlockToken::Restart(index) => {
                        coder.restart(index)?;
                        statistics.reset();
                        predictors.fill(0);
                        classes.fill(0);
                    }
                    BlockToken::Block(i, block) => {
                        let group = usize::from(i != 0);
                        if image.frame.process.is_differential() {
                            predictors[i] = 0;
                        }
                        if scan.has_dc() {
                            let al = scan.progression.map_or(0, |s| s.al);
                            if scan.progression.is_some_and(|s| s.ah != 0) {
                                coder.fixed(((i32::from(block[0]) >> al) & 1) as u8)?;
                            } else {
                                statistics.encode_dc(
                                    &mut coder,
                                    group,
                                    i32::from(block[0]) >> al,
                                    &mut predictors[i],
                                    &mut classes[i],
                                    options.arithmetic_conditioning,
                                )?;
                            }
                        }
                        if scan.has_ac() {
                            if let Some(s) = scan.progression.filter(|s| s.ah != 0) {
                                statistics.encode_ac_refine(&mut coder, group, block, s)?;
                            } else {
                                statistics.encode_ac(
                                    &mut coder,
                                    group,
                                    block,
                                    options.arithmetic_conditioning,
                                    scan.progression.map_or(1, |s| s.ss),
                                    scan.progression.map_or(63, |s| s.se),
                                    scan.progression.map_or(0, |s| s.al),
                                )?;
                            }
                        }
                    }
                }
                Ok(())
            })?;
            coder.finish()?;
        }
        options
            .height_declaration
            .after_scan(out, image.frame.height, s)?;
    }
    Ok(scan_count)
}

pub(crate) fn emit_metadata(
    out: &mut Output,
    segments: &[MetadataSegment<'_>],
    index: &mut usize,
    scan: usize,
    icc: IccWrite<'_>,
) -> Result<()> {
    while let Some(s) = segments.get(*index) {
        if s.preceding_scans > scan {
            break;
        }
        if matches!(icc, IccWrite::Preserve) || !is_icc(s) {
            out.segment(s.marker, s.data)?;
        }
        *index += 1;
    }
    Ok(())
}

pub(crate) fn emit_additional(out: &mut Output, metadata: MetadataOptions<'_>) -> Result<()> {
    for segment in metadata.additional_segments {
        if matches!(metadata.icc, IccWrite::Preserve) || !is_icc(segment) {
            out.segment(segment.marker, segment.data)?;
        }
    }
    if let IccWrite::Replace(profile) = metadata.icc {
        let count = profile.len().div_ceil(ICC_CHUNK_BYTES) as u8;
        for (i, chunk) in profile.chunks(ICC_CHUNK_BYTES).enumerate() {
            out.segment_parts(0xe2, &[ICC_IDENTIFIER, &[i as u8 + 1, count], chunk])?;
        }
    }
    Ok(())
}

pub(crate) fn validate_frame(image: DctFrame<'_>, limits: crate::Limits) -> Result<()> {
    let f = &image.frame;
    let n = image.components.len();
    if f.process.is_lossless() {
        return Err(Error::Unsupported(
            "coefficient encoding of non-DCT process",
        ));
    }
    if n == 0 || n > 4 {
        return Err(Error::Unsupported(
            "coefficient encoder component count outside 1..4",
        ));
    }
    if f.width == 0 || f.height == 0 || !matches!(f.precision, 8 | 12) || f.components.len() != n {
        return Err(Error::Invalid("encoder frame dimensions or precision"));
    }
    if f.width as usize * f.height as usize > limits.max_pixels {
        return Err(Error::LimitExceeded("encoder pixels"));
    }
    let mut max_h = 0;
    let mut max_v = 0;
    for (i, c) in image.components.iter().enumerate() {
        if c.spec != f.components[i]
            || image.components[..i].iter().any(|p| p.spec.id == c.spec.id)
            || !(1..=4).contains(&c.spec.horizontal_sampling)
            || !(1..=4).contains(&c.spec.vertical_sampling)
            || c.quantization.contains(&0)
        {
            return Err(Error::Invalid(
                "encoder component specification or quantizer",
            ));
        }
        max_h = max_h.max(c.spec.horizontal_sampling as usize);
        max_v = max_v.max(c.spec.vertical_sampling as usize);
    }
    let cols = (f.width as usize).div_ceil(8 * max_h);
    let rows = (f.height as usize).div_ceil(8 * max_v);
    for c in image.components {
        if c.width != (f.width as usize * c.spec.horizontal_sampling as usize).div_ceil(max_h)
            || c.height != (f.height as usize * c.spec.vertical_sampling as usize).div_ceil(max_v)
            || c.blocks_wide < cols * c.spec.horizontal_sampling as usize
            || c.blocks_high < rows * c.spec.vertical_sampling as usize
            || c.blocks_wide.checked_mul(c.blocks_high) != Some(c.blocks.len())
        {
            return Err(Error::Invalid("encoder coefficient grid"));
        }
    }
    Ok(())
}

pub(crate) fn validate_metadata(
    segments: &[MetadataSegment<'_>],
    limits: crate::Limits,
    options: MetadataOptions<'_>,
) -> Result<usize> {
    let mut bytes = 0usize;
    for (additional, segments) in [(false, segments), (true, options.additional_segments)] {
        for s in segments {
            if !matches!(s.marker, 0xe0..=0xef | 0xfe)
                || s.data.len() > 65533
                || (additional && s.preceding_scans != 0)
            {
                return Err(Error::Invalid("encoder metadata addition"));
            }
            if !matches!(options.icc, IccWrite::Preserve) && is_icc(s) {
                continue;
            }
            bytes = bytes
                .checked_add(s.data.len().saturating_add(core::mem::size_of_val(s)))
                .ok_or(Error::LimitExceeded("encoder metadata"))?;
        }
    }
    if let IccWrite::Replace(profile) = options.icc {
        if profile.is_empty() {
            return Err(Error::Invalid("empty ICC profile"));
        }
        if profile.len() > MAX_ICC_BYTES {
            return Err(Error::LimitExceeded("ICC profile bytes"));
        }
        let count = profile.len().div_ceil(ICC_CHUNK_BYTES);
        bytes = bytes
            .checked_add(profile.len())
            .and_then(|n| n.checked_add(count * (14 + core::mem::size_of::<MetadataSegment<'_>>())))
            .ok_or(Error::LimitExceeded("encoder metadata"))?;
    }
    if bytes > limits.max_metadata_bytes {
        return Err(Error::LimitExceeded("encoder metadata"));
    }
    Ok(bytes)
}

fn walk(
    image: DctFrame<'_>,
    scan: Scan,
    interval: u16,
    emit: &mut impl FnMut(Token) -> Result<()>,
) -> Result<()> {
    let mut predictors = [0; 4];
    walk_blocks(image, scan, interval, &mut |token| {
        match token {
            BlockToken::Restart(index) => {
                emit(Token::Restart(index))?;
                predictors.fill(0);
            }
            BlockToken::Block(i, block) => {
                let group = usize::from(i != 0);
                if image.frame.process.is_differential() {
                    predictors[i] = 0;
                }
                if scan.has_dc() {
                    let al = scan.progression.map_or(0, |s| s.al);
                    if scan.progression.is_some_and(|s| s.ah != 0) {
                        emit(Token::Bits(((i32::from(block[0]) >> al) & 1) as u32, 1))?;
                    } else {
                        let value = i32::from(block[0]) >> al;
                        let limit = 1 << (image.frame.precision + 3);
                        if scan.progression.is_none() && (value < -limit || value >= limit) {
                            return Err(Error::Invalid("encoder DC coefficient range"));
                        }
                        let difference = value - predictors[i];
                        predictors[i] = value;
                        let size = category(difference);
                        if size > image.frame.precision + 3 {
                            return Err(Error::Invalid("encoder DC magnitude"));
                        }
                        emit(Token::Symbol(group, size))?;
                        emit(Token::Bits(amplitude(difference, size), size))?;
                    }
                }
                if scan.has_ac() {
                    if let Some(s) = scan.progression.filter(|s| s.ah != 0) {
                        emit_ac_refine(block, group + 2, s, emit)?;
                    } else {
                        emit_ac(
                            block,
                            group + 2,
                            image.frame.precision + u8::from(image.frame.process.is_differential()),
                            scan.progression.map_or(1, |s| s.ss),
                            scan.progression.map_or(63, |s| s.se),
                            scan.progression.map_or(0, |s| s.al),
                            emit,
                        )?;
                    }
                }
            }
        }
        Ok(())
    })
}

fn walk_blocks<'a>(
    image: DctFrame<'a>,
    scan: Scan,
    interval: u16,
    emit: &mut impl FnMut(BlockToken<'a>) -> Result<()>,
) -> Result<()> {
    let interleaved = scan.count > 1;
    let (cols, rows) = if interleaved {
        let max_h = image
            .components
            .iter()
            .map(|c| c.spec.horizontal_sampling)
            .max()
            .unwrap() as usize;
        let max_v = image
            .components
            .iter()
            .map(|c| c.spec.vertical_sampling)
            .max()
            .unwrap() as usize;
        (
            (image.frame.width as usize).div_ceil(max_h * 8),
            (image.frame.height as usize).div_ceil(max_v * 8),
        )
    } else {
        let c = &image.components[scan.indices[0]];
        (c.width.div_ceil(8), c.height.div_ceil(8))
    };
    let mut restart = 0;
    for mcu in 0..cols * rows {
        if interval != 0 && mcu != 0 && mcu % interval as usize == 0 {
            emit(BlockToken::Restart(restart))?;
            restart = (restart + 1) & 7;
        }
        for &i in &scan.indices[..scan.count] {
            let c = &image.components[i];
            let (h, v) = if interleaved {
                (
                    c.spec.horizontal_sampling as usize,
                    c.spec.vertical_sampling as usize,
                )
            } else {
                (1, 1)
            };
            for y in 0..v {
                for x in 0..h {
                    let block =
                        &c.blocks[(mcu / cols * v + y) * c.blocks_wide + mcu % cols * h + x];
                    emit(BlockToken::Block(i, block))?;
                }
            }
        }
    }
    Ok(())
}

#[inline]
fn category(value: i32) -> u8 {
    (32 - value.unsigned_abs().leading_zeros()) as u8
}
#[inline]
fn amplitude(value: i32, n: u8) -> u32 {
    if value < 0 {
        ((value - 1) as u32) & ((1u32 << n) - 1)
    } else {
        value as u32
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_ac(
    block: &[i16; 64],
    table: usize,
    precision: u8,
    ss: usize,
    se: usize,
    al: u8,
    emit: &mut impl FnMut(Token) -> Result<()>,
) -> Result<()> {
    let mut run = 0u8;
    for &index in &ZIGZAG[ss..=se] {
        let original = i32::from(block[index]);
        let magnitude = original.unsigned_abs() >> al;
        let value = if original < 0 {
            -(magnitude as i32)
        } else {
            magnitude as i32
        };
        if value == 0 {
            run += 1;
            continue;
        }
        while run >= 16 {
            emit(Token::Symbol(table, 0xf0))?;
            run -= 16;
        }
        let size = category(value);
        if size > precision + 2 {
            return Err(Error::Invalid("encoder AC magnitude"));
        }
        emit(Token::Symbol(table, (run << 4) | size))?;
        emit(Token::Bits(amplitude(value, size), size))?;
        run = 0;
    }
    if run > 0 {
        emit(Token::Symbol(table, 0))?;
    }
    Ok(())
}

fn correction_bits(
    bits: u64,
    mut count: u8,
    emit: &mut impl FnMut(Token) -> Result<()>,
) -> Result<()> {
    while count != 0 {
        let n = count.min(16);
        count -= n;
        emit(Token::Bits(((bits >> count) & ((1u64 << n) - 1)) as u32, n))?;
    }
    Ok(())
}

fn emit_ac_refine(
    block: &[i16; 64],
    table: usize,
    scan: crate::progressive::ProgressiveScan,
    emit: &mut impl FnMut(Token) -> Result<()>,
) -> Result<()> {
    let magnitude = |k: usize| i32::from(block[ZIGZAG[k]]).unsigned_abs();
    let last_new = (scan.ss..=scan.se).rfind(|&k| magnitude(k) >> scan.al == 1);
    let mut corrections = 0u64;
    let mut count = 0u8;
    let mut run = 0u8;
    if let Some(last) = last_new {
        for k in scan.ss..=last {
            let value = magnitude(k) >> scan.al;
            if value > 1 {
                corrections = (corrections << 1) | u64::from(value & 1);
                count += 1;
            } else if value == 0 {
                run += 1;
                if run == 16 {
                    emit(Token::Symbol(table, 0xf0))?;
                    correction_bits(corrections, count, emit)?;
                    run = 0;
                    corrections = 0;
                    count = 0;
                }
            } else {
                emit(Token::Symbol(table, (run << 4) | 1))?;
                emit(Token::Bits(u32::from(block[ZIGZAG[k]] > 0), 1))?;
                correction_bits(corrections, count, emit)?;
                run = 0;
                corrections = 0;
                count = 0;
            }
        }
    }
    let tail = last_new.map_or(scan.ss, |k| k + 1);
    if tail <= scan.se {
        for k in tail..=scan.se {
            let value = magnitude(k) >> scan.al;
            if value > 1 {
                corrections = (corrections << 1) | u64::from(value & 1);
                count += 1;
            }
        }
        // One EOB per block bounds correction storage to 63 bits on the stack.
        emit(Token::Symbol(table, 0))?;
        correction_bits(corrections, count, emit)?;
    }
    Ok(())
}
