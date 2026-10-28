use crate::huffman_encode::Codebook;
use crate::output::Output;
use crate::{CoefficientImage, Error, Process, Result, ZIGZAG};

/// Entropy coding controls shared by pixel encoding and coefficient rewriting.
#[derive(Clone, Copy, Debug)]
pub struct CodingOptions {
    /// Spectral progressive output: an interleaved DC scan and one AC scan per
    /// component. Coefficients are transmitted at full precision in each band.
    pub progressive: bool,
    pub optimize_huffman: bool,
    /// MCU count between restart markers; zero disables restarts.
    pub restart_interval: u16,
    pub max_output_bytes: usize,
}

impl Default for CodingOptions {
    fn default() -> Self {
        Self {
            progressive: false,
            optimize_huffman: true,
            restart_interval: 0,
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

#[derive(Clone, Copy)]
enum Scan {
    Sequential,
    Dc,
    Ac(usize),
}

/// Encode original quantized coefficients without an IDCT/FDCT or color
/// conversion. Quantizer values and APPn/COM payloads are preserved. Metadata
/// keeps its relative order; positions past the new last scan move before EOI.
/// This initial writer accepts one to four components and Huffman DCT frames.
pub fn encode_coefficients(
    image: &CoefficientImage<'_>,
    options: CodingOptions,
) -> Result<Vec<u8>> {
    validate(image)?;
    let n = image.components.len();
    let scan_count = if options.progressive { n + 1 } else { 1 };
    if scan_count > image.limits.max_scans {
        return Err(Error::LimitExceeded("encoder scans"));
    }
    let scan_at = |s| {
        if !options.progressive {
            Scan::Sequential
        } else if s == 0 {
            Scan::Dc
        } else {
            Scan::Ac(s - 1)
        }
    };
    let mut histograms = [[0u64; 256]; 4];
    // Validate categories and collect frequencies before allocating output.
    for s in 0..scan_count {
        walk(image, scan_at(s), options.restart_interval, &mut |token| {
            if let Token::Symbol(table, symbol) = token {
                histograms[table][symbol as usize] += 1;
            }
            Ok(())
        })?;
    }
    let make = |i: usize| {
        if options.optimize_huffman && histograms[i].iter().any(|&v| v != 0) {
            Codebook::optimized(&histograms[i])
        } else {
            Codebook::fixed(i >= 2, image.frame.precision, options.progressive)
        }
    };
    let books = [make(0)?, make(1)?, make(2)?, make(3)?];
    let mut quantizers = Vec::<[u16; 64]>::new();
    quantizers
        .try_reserve_exact(n)
        .map_err(|_| Error::Allocation)?;
    let mut qslots = [0u8; 4];
    for (i, c) in image.components.iter().enumerate() {
        let slot = if let Some(j) = quantizers.iter().position(|q| q == &c.quantization) {
            j
        } else {
            quantizers.push(c.quantization);
            quantizers.len() - 1
        };
        qslots[i] = slot as u8;
    }
    let wide = quantizers.iter().any(|q| q.iter().any(|&v| v > 255));
    let process = if options.progressive {
        Process::Progressive
    } else if image.frame.precision == 8 && !wide {
        Process::Baseline
    } else {
        Process::ExtendedSequential
    };
    let mut out = Output::new(options.max_output_bytes);
    out.marker(0xd8)?;
    let mut metadata_index = 0;
    emit_metadata(&mut out, image, &mut metadata_index, 0)?;
    for (i, q) in quantizers.iter().enumerate() {
        let wide = q.iter().any(|&v| v > 255);
        let mut data = [0u8; 129];
        data[0] = (if wide { 16 } else { 0 }) | i as u8;
        let mut p = 1;
        for &index in &ZIGZAG {
            if wide {
                data[p..p + 2].copy_from_slice(&q[index].to_be_bytes());
                p += 2;
            } else {
                data[p] = q[index] as u8;
                p += 1;
            }
        }
        out.segment(0xdb, &data[..p])?;
    }
    let mut header = [0u8; 18];
    header[0] = image.frame.precision;
    header[1..3].copy_from_slice(&image.frame.height.to_be_bytes());
    header[3..5].copy_from_slice(&image.frame.width.to_be_bytes());
    header[5] = n as u8;
    for (i, c) in image.components.iter().enumerate() {
        header[6 + 3 * i] = c.spec.id;
        header[7 + 3 * i] = (c.spec.horizontal_sampling << 4) | c.spec.vertical_sampling;
        header[8 + 3 * i] = qslots[i];
    }
    out.segment(process as u8, &header[..6 + 3 * n])?;
    let groups = if n == 1 { 1 } else { 2 };
    for i in 0..groups {
        books[i].emit(&mut out, 0, i as u8)?;
        books[2 + i].emit(&mut out, 1, i as u8)?;
    }
    if options.restart_interval != 0 {
        out.segment(0xdd, &options.restart_interval.to_be_bytes())?;
    }
    for s in 0..scan_count {
        if s > 0 {
            emit_metadata(&mut out, image, &mut metadata_index, s)?;
        }
        let scan = scan_at(s);
        let mut sos = [0u8; 12];
        let length = match scan {
            Scan::Sequential | Scan::Dc => {
                sos[0] = n as u8;
                for (i, c) in image.components.iter().enumerate() {
                    sos[1 + 2 * i] = c.spec.id;
                    sos[2 + 2 * i] = if i == 0 { 0 } else { 0x11 };
                }
                sos[2 + 2 * n] = if matches!(scan, Scan::Sequential) {
                    63
                } else {
                    0
                };
                4 + 2 * n
            }
            Scan::Ac(i) => {
                sos[..6].copy_from_slice(&[
                    1,
                    image.components[i].spec.id,
                    if i == 0 { 0 } else { 0x11 },
                    1,
                    63,
                    0,
                ]);
                6
            }
        };
        out.segment(0xda, &sos[..length])?;
        walk(
            image,
            scan,
            options.restart_interval,
            &mut |token| match token {
                Token::Symbol(table, symbol) => books[table].write(&mut out, symbol),
                Token::Bits(value, n) => out.write_bits(value, n),
                Token::Restart(i) => {
                    out.pad()?;
                    out.marker(0xd0 + i)
                }
            },
        )?;
        out.pad()?;
    }
    emit_metadata(&mut out, image, &mut metadata_index, usize::MAX)?;
    out.marker(0xd9)?;
    Ok(out.bytes)
}

fn emit_metadata(
    out: &mut Output,
    image: &CoefficientImage<'_>,
    index: &mut usize,
    scan: usize,
) -> Result<()> {
    while let Some(s) = image.metadata.get(*index) {
        if s.preceding_scans > scan {
            break;
        }
        out.segment(s.marker, s.data)?;
        *index += 1;
    }
    Ok(())
}

fn validate(image: &CoefficientImage<'_>) -> Result<()> {
    let f = &image.frame;
    let n = image.components.len();
    if !matches!(
        f.process,
        Process::Baseline | Process::ExtendedSequential | Process::Progressive
    ) {
        return Err(Error::Unsupported(
            "coefficient encoding of non-Huffman DCT process",
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
    if f.width as usize * f.height as usize > image.limits.max_pixels {
        return Err(Error::LimitExceeded("encoder pixels"));
    }
    let mut max_h = 0;
    let mut max_v = 0;
    let mut blocks_per_mcu = 0;
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
        blocks_per_mcu += c.spec.horizontal_sampling as usize * c.spec.vertical_sampling as usize;
    }
    if n > 1 && blocks_per_mcu > 10 {
        return Err(Error::Invalid("encoder MCU exceeds ten blocks"));
    }
    let cols = (f.width as usize).div_ceil(8 * max_h);
    let rows = (f.height as usize).div_ceil(8 * max_v);
    for c in &image.components {
        if c.width != (f.width as usize * c.spec.horizontal_sampling as usize).div_ceil(max_h)
            || c.height != (f.height as usize * c.spec.vertical_sampling as usize).div_ceil(max_v)
            || c.blocks_wide < cols * c.spec.horizontal_sampling as usize
            || c.blocks_high < rows * c.spec.vertical_sampling as usize
            || c.blocks_wide.checked_mul(c.blocks_high) != Some(c.blocks.len())
        {
            return Err(Error::Invalid("encoder coefficient grid"));
        }
    }
    let mut bytes = 0usize;
    let mut scan = 0;
    for s in &image.metadata {
        if !matches!(s.marker, 0xe0..=0xef | 0xfe)
            || s.data.len() > 65533
            || s.preceding_scans < scan
        {
            return Err(Error::Invalid("encoder metadata"));
        }
        scan = s.preceding_scans;
        bytes = bytes
            .checked_add(s.data.len().saturating_add(core::mem::size_of_val(s)))
            .ok_or(Error::LimitExceeded("encoder metadata"))?;
        if bytes > image.limits.max_metadata_bytes {
            return Err(Error::LimitExceeded("encoder metadata"));
        }
    }
    Ok(())
}

fn walk(
    image: &CoefficientImage<'_>,
    scan: Scan,
    interval: u16,
    emit: &mut impl FnMut(Token) -> Result<()>,
) -> Result<()> {
    let n = image.components.len();
    let interleaved = !matches!(scan, Scan::Ac(_)) && n > 1;
    let first = if let Scan::Ac(i) = scan { i } else { 0 };
    let last = if matches!(scan, Scan::Ac(_)) {
        first + 1
    } else {
        n
    };
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
        let c = &image.components[first];
        (c.width.div_ceil(8), c.height.div_ceil(8))
    };
    let mut predictors = [0i32; 4];
    let mut restart = 0;
    for mcu in 0..cols * rows {
        if interval != 0 && mcu != 0 && mcu % interval as usize == 0 {
            emit(Token::Restart(restart))?;
            restart = (restart + 1) & 7;
            predictors.fill(0);
        }
        for (i, c) in image.components.iter().enumerate().take(last).skip(first) {
            let (h, v) = if interleaved {
                (
                    c.spec.horizontal_sampling as usize,
                    c.spec.vertical_sampling as usize,
                )
            } else {
                (1, 1)
            };
            let group = usize::from(i != 0);
            for y in 0..v {
                for x in 0..h {
                    let block =
                        &c.blocks[(mcu / cols * v + y) * c.blocks_wide + mcu % cols * h + x];
                    if !matches!(scan, Scan::Ac(_)) {
                        let value = i32::from(block[0]);
                        let difference = value - predictors[i];
                        predictors[i] = value;
                        let size = category(difference);
                        if size > image.frame.precision + 3 {
                            return Err(Error::Invalid("encoder DC magnitude"));
                        }
                        emit(Token::Symbol(group, size))?;
                        emit(Token::Bits(amplitude(difference, size), size))?;
                    }
                    if !matches!(scan, Scan::Dc) {
                        emit_ac(block, group + 2, image.frame.precision, emit)?;
                    }
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

fn emit_ac(
    block: &[i16; 64],
    table: usize,
    precision: u8,
    emit: &mut impl FnMut(Token) -> Result<()>,
) -> Result<()> {
    let mut run = 0u8;
    for &index in &ZIGZAG[1..] {
        let value = i32::from(block[index]);
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
