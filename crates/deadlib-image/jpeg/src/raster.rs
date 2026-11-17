//! Pixel output without retaining full-image coefficients for sequential scans.
use crate::decode::{parse_huffman, parse_quant};
use crate::fast::{self, Reader};
use crate::simd::Kernel;
use crate::transform::{FloatQuant, idct8_auto, idct8_dc};
use crate::reconstruct::{
    ColorHints, CHUNK, cmyk_to_rgb, emit_chunk, infer_components_with_hints, ycbcr_chunk, ycbcr_row, ycbcr420_row,
    ycck_to_cmyk,
};
use crate::{
    CmykPolarity, CoefficientImage, ColorSpace, Error, Event, FrameHeader, Limits, MarkerReader, MetadataSegment,
    Process, Result, Tables, Upsampling,
};
use deadlib_image::{ColorType, Image, Samples};

/// Decode gray/RGB pixels with centered linear chroma interpolation. Native
/// coefficient, planar, and metadata-preserving APIs remain available separately.
/// Sequential 8-bit single-scan images need only a rolling MCU-row window;
/// other processes use their precision-preserving native reconstruction path.
pub fn decode(input: &[u8], limits: Limits) -> Result<Image> {
    decode_with_upsampling(input, limits, Upsampling::Linear)
}

pub fn decode_with_upsampling(input: &[u8], limits: Limits, method: Upsampling) -> Result<Image> {
    let mut tables = Tables::new();
    if let Some(plan) = prepare(input, limits, &mut tables)? {
        return plan.decode(limits, method);
    }
    // Choose the existing process decoder without changing its validation or
    // numerical model. This also handles DCT/lossless hierarchical mixtures.
    for event in MarkerReader::new(input)? {
        if let Event::Segment { marker, .. } = event? {
            if marker == 0xde {
                return crate::decode_hierarchical(input, limits)?
                    .reconstruct()?
                    .to_image(method);
            }
            if let Ok(process) = Process::try_from(marker) {
                if process.is_lossless() {
                    return crate::decode_lossless(input, limits)?
                        .into_planar()?
                        .to_image(method);
                }
                break;
            }
        }
    }
    // Pixel output needs only color hints, not retained metadata segments.
    let mut hints = ColorHints::default();
    let image =
        crate::decode::decode_with_tables(input, limits, &mut Tables::new(), Some(&mut hints))?;
    let color = hints.color(&image.frame.components);
    if image.frame.precision == 8 && color != ColorSpace::Unknown {
        return from_coefficients(&image, color, hints.polarity(), limits, method);
    }
    let mut planes = image.reconstruct()?;
    planes.color_space = color;
    planes.cmyk_polarity = hints.polarity();
    planes.to_image(method)
}

struct Plan<'a> {
    frame: FrameHeader,
    // Borrowed from the caller: moving ~20 KB of tables cost a memcpy each.
    tables: &'a Tables,
    selected: [(usize, usize); 4],
    entropy: &'a [u8],
    interval: usize,
    color: ColorSpace,
    polarity: CmykPolarity,
    metadata_bytes: usize,
}

fn prepare<'a>(input: &'a [u8], limits: Limits, tables: &'a mut Tables) -> Result<Option<Plan<'a>>> {
    let mut reader = MarkerReader::new(input)?;
    let mut frame = None;
    let mut jfif = false;
    let mut adobe = None;
    let mut inverted = false;
    let mut metadata_bytes = 0usize;
    let mut scan = None;
    let mut interval = 0;
    let mut ended = false;
    while let Some(event) = reader.next() {
        match event? {
            Event::End { .. } => {
                ended = true;
                break;
            }
            Event::Segment {
                marker: marker @ (0xdb | 0xc4 | 0xdd),
                data,
                ..
            } => {
                // A later definition must never leak backwards into a scan.
                if scan.is_some() {
                    return Ok(None);
                }
                match marker {
                    0xdb => parse_quant(data, &mut tables.quant)?,
                    0xc4 => parse_huffman(data, &mut tables.huffman)?,
                    _ => {
                        if data.len() != 2 {
                            return Err(Error::Invalid("DRI length"));
                        }
                        interval = u16::from_be_bytes(data.try_into().unwrap()) as usize;
                    }
                }
            }
            Event::Segment {
                marker: marker @ (0xe0..=0xef | 0xfe),
                data,
                offset,
            } => {
                metadata_bytes = metadata_bytes
                    .checked_add(data.len() + core::mem::size_of::<MetadataSegment<'_>>())
                    .ok_or(Error::LimitExceeded("metadata"))?;
                if metadata_bytes > limits.max_metadata_bytes
                    || metadata_bytes > limits.max_memory_bytes
                {
                    return Err(Error::LimitExceeded("metadata"));
                }
                if scan.is_none() {
                    jfif |= marker == 0xe0 && data.len() >= 14 && data.starts_with(b"JFIF\0");
                    if let Some(value) = (MetadataSegment {
                        marker,
                        data,
                        offset,
                        preceding_scans: 0,
                    })
                    .adobe()
                    .ok()
                    .flatten()
                    {
                        adobe = Some(value.transform);
                        inverted |= value.transform == 0 || value.transform == 2;
                    }
                }
            }
            Event::Segment { marker, data, .. } if Process::try_from(marker).is_ok() => {
                if frame.is_some() {
                    return Ok(None);
                }
                if data.first() != Some(&8)
                    || !matches!(
                        Process::try_from(marker)?,
                        Process::Baseline | Process::ExtendedSequential
                    )
                {
                    return Ok(None);
                }
                let header = FrameHeader::parse(marker, data)?;
                frame = Some(header);
            }
            Event::Scan {
                header, entropy, ..
            } => {
                if scan.is_some() {
                    return Ok(None);
                }
                if limits.max_scans == 0 {
                    return Err(Error::LimitExceeded("scans"));
                }
                let frame = frame.as_mut().ok_or(Error::Invalid("scan before frame"))?;
                reader.resolve_height(frame)?;
                if header.is_empty() || usize::from(header[0]) != frame.components.len() {
                    return Ok(None);
                }
                scan = Some((header, entropy));
            }
            // The general decoders retain validation of unused DAC, reserved
            // markers, multiple scans and every other coding process.
            _ => return Ok(None),
        }
    }
    if !ended {
        return Err(Error::Truncated);
    }
    let frame = frame.ok_or(Error::Invalid("missing frame"))?;
    let (header, entropy) = scan.ok_or(Error::Invalid("missing component scan"))?;
    let n = frame.components.len();
    if n > 4 || header.len() != 4 + 2 * n || header[1 + 2 * n..] != [0, 63, 0] {
        return Err(Error::Invalid("sequential scan parameters"));
    }
    let mut selected = [(0, 0); 4];
    let mut blocks = 0usize;
    for (i, fields) in header[1..1 + 2 * n].as_chunks::<2>().0.iter().enumerate() {
        if fields[0] != frame.components[i].id {
            return Err(Error::Invalid("SOS component order"));
        }
        let dc = usize::from(fields[1] >> 4);
        let ac = usize::from(fields[1] & 15);
        let max = if frame.process == Process::Baseline {
            1
        } else {
            3
        };
        if dc > max || ac > max || tables.huffman[dc].is_none() || tables.huffman[4 + ac].is_none()
        {
            return Err(Error::Invalid("missing or invalid entropy table"));
        }
        let spec = frame.components[i];
        let q = tables.quant[usize::from(spec.quantization_table)]
            .as_ref()
            .ok_or(Error::Invalid("missing quantization table"))?;
        if frame.process == Process::Baseline && q.wide {
            return Err(Error::Invalid("16-bit quantizer in baseline"));
        }
        selected[i] = (dc, 4 + ac);
        blocks += usize::from(spec.horizontal_sampling) * usize::from(spec.vertical_sampling);
    }
    if n > 1 && blocks > 10 {
        return Err(Error::Invalid("more than ten blocks per MCU"));
    }
    let color = infer_components_with_hints(&frame.components, jfif, adobe);
    let polarity = if inverted {
        CmykPolarity::AdobeInverted
    } else {
        CmykPolarity::Conventional
    };
    if color == ColorSpace::Unknown {
        return Ok(None);
    }
    Ok(Some(Plan {
        frame,
        tables,
        selected,
        entropy,
        interval,
        color,
        polarity,
        metadata_bytes,
    }))
}

#[derive(Clone, Copy, Default)]
struct Layout {
    width: usize,
    height: usize,
    stride: usize,
    band_rows: usize,
    offset: usize,
    h: usize,
    v: usize,
    slots: usize,
}

/// Circular MCU rows per component. Centered vertical interpolation can touch
/// the previous and next MCU row, including fractional sampling grids, so it
/// keeps three slots and emission trails reconstruction by one row. Without
/// vertical subsampling, or with nearest sampling, every source row lies in
/// the current MCU row: one slot suffices and rows emit immediately (a zero
/// vertical weight may still read an in-bounds neighbor). Storage is
/// independent of the image height. Grayscale blocks go straight to the output.
struct Window {
    width: usize,
    height: usize,
    components: usize,
    color: ColorSpace,
    polarity: CmykPolarity,
    gray: bool,
    count: usize,
    cols: usize,
    rows: usize,
    max_h: usize,
    max_v: usize,
    layouts: [Layout; 4],
    cache_bytes: usize,
    delay: usize,
}

impl Window {
    fn new(
        frame: &FrameHeader,
        color: ColorSpace,
        polarity: CmykPolarity,
        limits: Limits,
        method: Upsampling,
    ) -> Result<Self> {
        let width = usize::from(frame.width);
        let height = usize::from(frame.height);
        let pixels = width * height;
        if pixels > limits.max_pixels {
            return Err(Error::LimitExceeded("pixels"));
        }
        let gray = color == ColorSpace::Grayscale;
        let count = pixels
            .checked_mul(if gray { 1 } else { 3 })
            .ok_or(Error::LimitExceeded("output storage"))?;
        let factor = |f: fn(&crate::ComponentSpec) -> u8| {
            frame.components.iter().map(|c| usize::from(f(c))).max().unwrap()
        };
        let (max_h, max_v) = if gray {
            (1, 1)
        } else {
            (factor(|c| c.horizontal_sampling), factor(|c| c.vertical_sampling))
        };
        let cols = width.div_ceil(8 * max_h);
        let rows = height.div_ceil(8 * max_v);
        let neighbors = method == Upsampling::Linear
            && frame
                .components
                .iter()
                .any(|c| usize::from(c.vertical_sampling) != max_v);
        let slots = if neighbors { 3 } else { 1 };
        let mut cache_bytes = 0;
        let layouts = core::array::from_fn(|i| {
            let Some(spec) = frame.components.get(i) else {
                return Layout::default();
            };
            let (h, v) = if gray {
                (1, 1)
            } else {
                (
                    usize::from(spec.horizontal_sampling),
                    usize::from(spec.vertical_sampling),
                )
            };
            let layout = Layout {
                width: (width * h).div_ceil(max_h),
                height: (height * v).div_ceil(max_v),
                stride: cols * h * 8,
                band_rows: v * 8,
                offset: cache_bytes,
                h,
                v,
                slots,
            };
            if !gray {
                cache_bytes += layout.stride * layout.band_rows * slots;
            }
            layout
        });
        Ok(Self {
            width,
            height,
            components: frame.components.len(),
            color,
            polarity,
            gray,
            count,
            cols,
            rows,
            max_h,
            max_v,
            layouts,
            cache_bytes,
            delay: usize::from(neighbors),
        })
    }

    /// Reserve output and window storage once `retained` caller-held bytes
    /// are counted against the memory limit.
    fn allocate(&self, retained: usize, limits: Limits) -> Result<(Vec<u8>, Vec<u8>)> {
        let storage = self
            .count
            .checked_add(self.cache_bytes)
            .and_then(|n| n.checked_add(retained))
            .ok_or(Error::LimitExceeded("pixel decoding storage"))?;
        if storage > limits.max_memory_bytes {
            return Err(Error::LimitExceeded("pixel decoding storage"));
        }
        let mut output = Vec::new();
        output
            .try_reserve_exact(self.count)
            .map_err(|_| Error::Allocation)?;
        if self.gray {
            output.resize(self.count, 0);
        }
        let mut cache = Vec::new();
        cache
            .try_reserve_exact(self.cache_bytes)
            .map_err(|_| Error::Allocation)?;
        cache.resize(self.cache_bytes, 0);
        Ok((output, cache))
    }

    /// Reconstruct `block` as block column `bx`, block row `by` within MCU row
    /// `my`, of component `i`. `slot` is `my % slots`, computed once per MCU
    /// row: a per-block division showed in profiles. No closure is involved,
    /// so the transform inlines into AVX2-dispatched callers.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    fn put(
        &self,
        i: usize,
        my: usize,
        slot: usize,
        by: usize,
        bx: usize,
        cache: &mut [u8],
        output: &mut [u8],
        block: &[i16; 64],
        quant: &FloatQuant,
        dc_only: bool,
    ) {
        if !self.gray {
            let l = self.layouts[i];
            let start = l.offset + slot * l.band_rows * l.stride + by * 8 * l.stride + bx * 8;
            return reconstruct(block, quant, dc_only, &mut cache[start..], l.stride);
        }
        let (x, y) = (bx * 8, my * 8);
        if x + 8 <= self.width && y + 8 <= self.height {
            let target = &mut output[y * self.width + x..];
            return reconstruct(block, quant, dc_only, target, self.width);
        }
        let mut edge = [0u8; 64];
        reconstruct(block, quant, dc_only, &mut edge, 8);
        let w = (self.width - x).min(8);
        for row in 0..(self.height - y).min(8) {
            output[(y + row) * self.width + x..][..w].copy_from_slice(&edge[row * 8..row * 8 + w]);
        }
    }

    /// After MCU row `my` is reconstructed, emit every row now complete.
    fn advance(&self, cache: &[u8], my: usize, method: Upsampling, out: &mut Vec<u8>) {
        if !self.gray && my >= self.delay {
            let row = my - self.delay;
            self.emit(cache, row * self.max_v * 8, self.max_v * 8, method, out);
        }
    }

    /// Emit the delayed last MCU row, if any.
    fn flush(&self, cache: &[u8], method: Upsampling, out: &mut Vec<u8>) {
        if !self.gray && self.delay == 1 {
            let row = self.rows - 1;
            self.emit(cache, row * self.max_v * 8, self.max_v * 8, method, out);
        }
    }

    fn finish(&self, output: Vec<u8>) -> Image {
        debug_assert_eq!(output.len(), self.count);
        Image {
            width: self.width as u32,
            height: self.height as u32,
            color_type: if self.gray {
                ColorType::Grayscale
            } else {
                ColorType::Rgb
            },
            bit_depth: 8,
            samples: Samples::U8(output),
        }
    }

    fn emit(&self, cache: &[u8], start: usize, rows: usize, method: Upsampling, out: &mut Vec<u8>) {
        crate::simd::dispatch(EmitKernel {
            window: self,
            cache,
            start,
            rows,
            method,
            out,
        });
    }

    #[inline(always)]
    fn emit_body(
        &self,
        cache: &[u8],
        start: usize,
        rows: usize,
        method: Upsampling,
        out: &mut Vec<u8>,
    ) {
        let (width, height, max_h, max_v) = (self.width, self.height, self.max_h, self.max_v);
        let full = self.layouts[..3].iter().all(|c| c.h == max_h && c.v == max_v);
        // 4:2:0 and 4:2:2: half-width chroma, half or full height.
        let half = self.layouts[0].h == max_h
            && self.layouts[0].v == max_v
            && self.layouts[1].h * 2 == max_h
            && (self.layouts[1].v * 2 == max_v || self.layouts[1].v == max_v)
            && self.layouts[2].h == self.layouts[1].h
            && self.layouts[2].v == self.layouts[1].v;
        let half_height = self.layouts[1].v * 2 == max_v;
        for y in start..(start + rows).min(height) {
            // A plain loop: the from_fn closure stayed out of line per row.
            let mut samplers = [Row::empty(); 4];
            for (i, sampler) in samplers.iter_mut().enumerate().take(self.components) {
                *sampler = Row::new(cache, self.layouts[i], y, self.max_h, self.max_v, method);
            }
            if self.color == ColorSpace::YCbCr && half && method == Upsampling::Linear {
                ycbcr420_row(
                    samplers[0].top,
                    [samplers[1].top, samplers[1].bottom],
                    [samplers[2].top, samplers[2].bottom],
                    if !half_height {
                        [4, 0]
                    } else if y & 1 == 0 {
                        [1, 3]
                    } else {
                        [3, 1]
                    },
                    out,
                );
            } else if self.color == ColorSpace::YCbCr && full {
                ycbcr_row(
                    &samplers[0].top[..width],
                    &samplers[1].top[..width],
                    &samplers[2].top[..width],
                    out,
                );
            } else if self.color == ColorSpace::Rgb && full {
                for x in 0..width {
                    out.extend_from_slice(&[
                        samplers[0].top[x],
                        samplers[1].top[x],
                        samplers[2].top[x],
                    ]);
                }
            } else {
                // Other grids: interpolate a chunk of each component, then convert.
                let mut rgb = [0u8; CHUNK * 3];
                for start in (0..width).step_by(CHUNK) {
                    let n = (start + CHUNK).min(width) - start;
                    let mut values = [[0i16; CHUNK]; 4];
                    for (sampler, row) in samplers.iter().zip(&mut values).take(self.components) {
                        sampler.fill(start, &mut row[..n]);
                    }
                    if self.color == ColorSpace::YCbCr {
                        ycbcr_chunk(&values[0], &values[1], &values[2], &mut rgb);
                    } else {
                        for x in 0..n {
                            let v = values.map(|row| i32::from(row[x]));
                            let pixel = match self.color {
                                ColorSpace::Rgb => [v[0], v[1], v[2]],
                                ColorSpace::Cmyk => cmyk_to_rgb(v, self.polarity, 8),
                                ColorSpace::Ycck => {
                                    cmyk_to_rgb(ycck_to_cmyk(v, 8), self.polarity, 8)
                                }
                                _ => unreachable!(),
                            };
                            rgb[3 * x..3 * x + 3].copy_from_slice(&pixel.map(|v| v as u8));
                        }
                    }
                    emit_chunk(out, &rgb, n);
                }
            }
        }
    }
}

impl Plan<'_> {
    fn decode(&self, limits: Limits, method: Upsampling) -> Result<Image> {
        crate::simd::dispatch(SequentialKernel {
            plan: self,
            limits,
            method,
        })
    }

    #[inline(always)]
    fn decode_body(&self, limits: Limits, method: Upsampling) -> Result<Image> {
        let window = Window::new(&self.frame, self.color, self.polarity, limits, method)?;
        let retained = self.metadata_bytes
            + self.frame.components.capacity() * core::mem::size_of::<crate::ComponentSpec>();
        let (mut output, mut cache) = window.allocate(retained, limits)?;
        let n = self.frame.components.len();
        let quants: [FloatQuant; 4] = core::array::from_fn(|i| {
            FloatQuant::new(&self.frame.components.get(i).map_or([0; 64], |spec| {
                self.tables.quant[usize::from(spec.quantization_table)]
                    .as_ref()
                    .unwrap()
                    .values
            }))
        });
        let mut reader = Reader::new(self.entropy);
        let mut predictors = [0; 4];
        let mut restart = 0;
        let mut block = [0i16; 64];
        for my in 0..window.rows {
            let slot = my % window.layouts[0].slots;
            for mx in 0..window.cols {
                let mcu = my * window.cols + mx;
                if self.interval != 0 && mcu != 0 && mcu % self.interval == 0 {
                    reader.restart(restart)?;
                    restart = (restart + 1) & 7;
                    predictors.fill(0);
                }
                for i in 0..n {
                    let l = window.layouts[i];
                    let dc = self.tables.huffman[self.selected[i].0].as_ref().unwrap();
                    let ac = self.tables.huffman[self.selected[i].1].as_ref().unwrap();
                    let quant = &quants[i];
                    for by in 0..l.v {
                        for bx in 0..l.h {
                            let last =
                                fast::block(&mut block, &mut reader, dc, ac, &mut predictors[i], 8, false)?;
                            let column = mx * l.h + bx;
                            window.put(
                                i,
                                my,
                                slot,
                                by,
                                column,
                                &mut cache,
                                &mut output,
                                &block,
                                quant,
                                last == 0,
                            );
                            if last == 0 {
                                block[0] = 0;
                            } else {
                                block = [0; 64];
                            }
                        }
                    }
                }
            }
            reader.check()?;
            window.advance(&cache, my, method, &mut output);
        }
        window.flush(&cache, method, &mut output);
        reader.finish()?;
        Ok(window.finish(output))
    }
}

/// Pixels from stored coefficients (progressive, multi-scan and arithmetic
/// 8-bit images): blocks go through the row window instead of full planes.
fn from_coefficients(
    image: &CoefficientImage<'_>,
    color: ColorSpace,
    polarity: CmykPolarity,
    limits: Limits,
    method: Upsampling,
) -> Result<Image> {
    crate::simd::dispatch(CoefficientKernel {
        image,
        color,
        polarity,
        limits,
        method,
    })
}

struct SequentialKernel<'p, 'a> {
    plan: &'p Plan<'a>,
    limits: Limits,
    method: Upsampling,
}

impl Kernel for SequentialKernel<'_, '_> {
    type Output = Result<Image>;
    #[inline(always)]
    fn run(self) -> Result<Image> {
        self.plan.decode_body(self.limits, self.method)
    }
}

struct CoefficientKernel<'i, 'a> {
    image: &'i CoefficientImage<'a>,
    color: ColorSpace,
    polarity: CmykPolarity,
    limits: Limits,
    method: Upsampling,
}

impl Kernel for CoefficientKernel<'_, '_> {
    type Output = Result<Image>;
    #[inline(always)]
    fn run(self) -> Result<Image> {
        from_coefficients_body(self.image, self.color, self.polarity, self.limits, self.method)
    }
}

struct EmitKernel<'w, 'c, 'o> {
    window: &'w Window,
    cache: &'c [u8],
    start: usize,
    rows: usize,
    method: Upsampling,
    out: &'o mut Vec<u8>,
}

impl Kernel for EmitKernel<'_, '_, '_> {
    type Output = ();
    #[inline(always)]
    fn run(self) {
        let EmitKernel {
            window,
            cache,
            start,
            rows,
            method,
            out,
        } = self;
        window.emit_body(cache, start, rows, method, out);
    }
}

#[inline(always)]
fn from_coefficients_body(
    image: &CoefficientImage<'_>,
    color: ColorSpace,
    polarity: CmykPolarity,
    limits: Limits,
    method: Upsampling,
) -> Result<Image> {
    image.validate_grid()?;
    let window = Window::new(&image.frame, color, polarity, limits, method)?;
    let (mut output, mut cache) = window.allocate(image.memory_bytes, limits)?;
    let quants: [FloatQuant; 4] = core::array::from_fn(|i| {
        FloatQuant::new(image.components.get(i).map_or(&[0; 64], |c| &c.quantization))
    });
    for my in 0..window.rows {
        let slot = my % window.layouts[0].slots;
        for (i, c) in image.components.iter().enumerate() {
            let quant = &quants[i];
            let v = window.layouts[i].v;
            for by in 0..v {
                let row = my * v + by;
                if row >= c.height.div_ceil(8) {
                    break;
                }
                for bx in 0..c.width.div_ceil(8) {
                    let block = &c.blocks[row * c.blocks_wide + bx];
                    window.put(i, my, slot, by, bx, &mut cache, &mut output, block, quant, false);
                }
            }
        }
        window.advance(&cache, my, method, &mut output);
    }
    window.flush(&cache, method, &mut output);
    Ok(window.finish(output))
}

#[inline(always)]
fn reconstruct(block: &[i16; 64], quant: &FloatQuant, dc_only: bool, out: &mut [u8], stride: usize) {
    if dc_only {
        idct8_dc(block[0], quant, out, stride);
    } else {
        idct8_auto(block, quant, out, stride);
    }
}

#[derive(Clone, Copy)]
struct Row<'a> {
    top: &'a [u8],
    bottom: &'a [u8],
    h: usize,
    max_h: usize,
    fy: i32,
    dy: i32,
    method: Upsampling,
}
impl<'a> Row<'a> {
    fn empty() -> Self {
        Self {
            top: &[],
            bottom: &[],
            h: 1,
            max_h: 1,
            fy: 0,
            dy: 1,
            method: Upsampling::Nearest,
        }
    }
    #[inline(always)]
    fn new(
        cache: &'a [u8],
        c: Layout,
        y: usize,
        max_h: usize,
        max_v: usize,
        method: Upsampling,
    ) -> Self {
        let (top, bottom, fy, dy) = if method == Upsampling::Nearest {
            let y = ((2 * y + 1) * c.v / (2 * max_v)).min(c.height - 1);
            (y, y, 0, 1)
        } else {
            let sy = ((2 * y + 1) * c.v) as i32 - max_v as i32;
            let dy = 2 * max_v as i32;
            // Euclidean division; shifts and masks for power-of-two factors.
            let (iy, fy) = if (dy as u32).is_power_of_two() {
                (sy >> dy.trailing_zeros(), sy & (dy - 1))
            } else {
                (sy.div_euclid(dy), sy.rem_euclid(dy))
            };
            (
                iy.clamp(0, c.height as i32 - 1) as usize,
                (iy + 1).clamp(0, c.height as i32 - 1) as usize,
                fy,
                dy,
            )
        };
        let get = |y: usize| {
            let (band, row) = if c.band_rows.is_power_of_two() {
                let shift = c.band_rows.trailing_zeros();
                (y >> shift, y & (c.band_rows - 1))
            } else {
                (y / c.band_rows, y % c.band_rows)
            };
            let slot = if c.slots == 1 { 0 } else { band % 3 };
            let start = c.offset + (slot * c.band_rows + row) * c.stride;
            &cache[start..start + c.width]
        };
        Self {
            top: get(top),
            bottom: get(bottom),
            h: c.h,
            max_h,
            fy,
            dy,
            method,
        }
    }
    /// Interpolated samples for `x` in `start..start + out.len()`. The source
    /// position advances by `2h` per pixel instead of dividing per sample;
    /// power-of-two denominators shift. Every sum is non-negative.
    // Out of line on purpose: compiled for AVX2, LLVM turned the indexed
    // loads into gathers, which were several times slower here.
    #[inline(never)]
    fn fill(&self, start: usize, out: &mut [i16]) {
        let last = self.top.len() as i32 - 1;
        if self.method == Upsampling::Nearest {
            for (k, value) in out.iter_mut().enumerate() {
                let x = start + k;
                let source = ((2 * x + 1) * self.h / (2 * self.max_h)).min(last as usize);
                *value = i16::from(self.top[source]);
            }
            return;
        }
        if self.h == self.max_h && self.fy == 0 {
            for (value, &sample) in out.iter_mut().zip(&self.top[start..]) {
                *value = i16::from(sample);
            }
            return;
        }
        let dx = 2 * self.max_h as i32;
        let step = 2 * self.h as i32;
        let denominator = dx * self.dy;
        let half = denominator / 2;
        let shift = (denominator as u32)
            .is_power_of_two()
            .then(|| denominator.trailing_zeros());
        let source = ((2 * start + 1) * self.h) as i32 - self.max_h as i32;
        let (mut ix, mut fx) = (source.div_euclid(dx), source.rem_euclid(dx));
        // The bilinear sum factors exactly through one vertical blend per
        // source sample: V[j] = top[j] * (dy - fy) + bottom[j] * fy, then
        // V[left] * (dx - fx) + V[right] * fx. The last pixel reads source
        // `base + span - 1`; h <= max_h keeps span <= out.len() + 1.
        let base = ix;
        let span = ((out.len() as i32 - 1) * step + fx) / dx + 2;
        let mut blend = [0i32; CHUNK + 2];
        for (j, value) in blend.iter_mut().enumerate().take(span as usize) {
            let at = (base + j as i32).clamp(0, last) as usize;
            *value = i32::from(self.top[at]) * (self.dy - self.fy)
                + i32::from(self.bottom[at]) * self.fy;
        }
        for value in out {
            let j = (ix - base) as usize;
            let sum = blend[j] * (dx - fx) + blend[j + 1] * fx + half;
            *value = shift.map_or(sum / denominator, |s| sum >> s) as i16;
            fx += step;
            if fx >= dx {
                fx -= dx;
                ix += 1;
            }
        }
    }
}
