//! Pixel output without retaining full-image coefficients for sequential scans.
use crate::decode::{parse_huffman, parse_quant};
use crate::fast::{self, Reader, Table};
use crate::transform::{FloatQuant, idct8, idct8_dc};
use crate::reconstruct::{
    cmyk_to_rgb, infer_components_with_hints, ycbcr_row, ycbcr_to_rgb, ycbcr420_row, ycck_to_cmyk,
};
use crate::{
    CmykPolarity, ColorSpace, Error, Event, FrameHeader, Limits, MarkerReader, MetadataSegment,
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
    if let Some(plan) = prepare(input, limits)? {
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
    crate::decode_coefficients(input, limits)?
        .reconstruct()?
        .to_image(method)
}

struct Plan<'a> {
    frame: FrameHeader,
    tables: Tables,
    selected: [(usize, usize); 4],
    entropy: &'a [u8],
    interval: usize,
    color: ColorSpace,
    polarity: CmykPolarity,
    metadata_bytes: usize,
}

fn prepare(input: &[u8], limits: Limits) -> Result<Option<Plan<'_>>> {
    let mut reader = MarkerReader::new(input)?;
    let mut tables = Tables::new();
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
}

impl Plan<'_> {
    fn decode(&self, limits: Limits, method: Upsampling) -> Result<Image> {
        let width = usize::from(self.frame.width);
        let height = usize::from(self.frame.height);
        let pixels = width * height;
        if pixels > limits.max_pixels {
            return Err(Error::LimitExceeded("pixels"));
        }
        let gray = self.color == ColorSpace::Grayscale;
        let channels = if gray { 1 } else { 3 };
        let count = pixels
            .checked_mul(channels)
            .ok_or(Error::LimitExceeded("output storage"))?;
        let max_h = self
            .frame
            .components
            .iter()
            .map(|c| usize::from(c.horizontal_sampling))
            .max()
            .unwrap();
        let max_v = self
            .frame
            .components
            .iter()
            .map(|c| usize::from(c.vertical_sampling))
            .max()
            .unwrap();
        let cols = width.div_ceil(if gray { 8 } else { 8 * max_h });
        let rows = height.div_ceil(if gray { 8 } else { 8 * max_v });
        let mut cache_bytes = 0;
        // Centered interpolation can touch the previous and next MCU row,
        // including fractional sampling grids. Three circular slots let us
        // delay emission by one row while keeping storage independent of height.
        let layouts: [Layout; 4] = core::array::from_fn(|i| {
            let Some(spec) = self.frame.components.get(i) else {
                return Layout::default();
            };
            let h = if gray {
                1
            } else {
                usize::from(spec.horizontal_sampling)
            };
            let v = if gray {
                1
            } else {
                usize::from(spec.vertical_sampling)
            };
            let layout = Layout {
                width: (width * h).div_ceil(if gray { 1 } else { max_h }),
                height: (height * v).div_ceil(if gray { 1 } else { max_v }),
                stride: cols * h * 8,
                band_rows: v * 8,
                offset: cache_bytes,
                h,
                v,
            };
            if !gray {
                cache_bytes += layout.stride * layout.band_rows * 3;
            }
            layout
        });
        let storage = count
            .checked_add(cache_bytes)
            .and_then(|n| n.checked_add(self.metadata_bytes))
            .and_then(|n| {
                n.checked_add(
                    self.frame.components.capacity() * core::mem::size_of::<crate::ComponentSpec>(),
                )
            })
            .ok_or(Error::LimitExceeded("pixel decoding storage"))?;
        if storage > limits.max_memory_bytes {
            return Err(Error::LimitExceeded("pixel decoding storage"));
        }
        let mut output = Vec::new();
        output
            .try_reserve_exact(count)
            .map_err(|_| Error::Allocation)?;
        if gray {
            output.resize(count, 0);
        }
        let mut cache = Vec::new();
        cache
            .try_reserve_exact(cache_bytes)
            .map_err(|_| Error::Allocation)?;
        cache.resize(cache_bytes, 0);
        let n = self.frame.components.len();
        let tables: [Option<Table>; 8] = core::array::from_fn(|t| {
            self.selected[..n]
                .iter()
                .any(|&(dc, ac)| dc == t || ac == t)
                .then(|| Table::new(self.tables.huffman[t].as_ref().unwrap()))
        });
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
        let mut edge = [0u8; 64];
        for my in 0..rows {
            for mx in 0..cols {
                let mcu = my * cols + mx;
                if self.interval != 0 && mcu != 0 && mcu % self.interval == 0 {
                    reader.restart(restart)?;
                    restart = (restart + 1) & 7;
                    predictors.fill(0);
                }
                for i in 0..n {
                    let c = layouts[i];
                    let dc = tables[self.selected[i].0].as_ref().unwrap();
                    let ac = tables[self.selected[i].1].as_ref().unwrap();
                    let quant = &quants[i];
                    for by in 0..c.v {
                        for bx in 0..c.h {
                            let last =
                                fast::block(&mut block, &mut reader, dc, ac, &mut predictors[i])?;
                            let (x, y) = (mx * 8, my * 8);
                            let inside = x + 8 <= width && y + 8 <= height;
                            let (target, stride) = if !gray {
                                let start = c.offset
                                    + (my % 3) * c.band_rows * c.stride
                                    + by * 8 * c.stride
                                    + mx * c.h * 8
                                    + bx * 8;
                                (&mut cache[start..], c.stride)
                            } else if inside {
                                (&mut output[y * width + x..], width)
                            } else {
                                (&mut edge[..], 8)
                            };
                            if last == 0 {
                                idct8_dc(block[0], quant, target, stride);
                                block[0] = 0;
                            } else {
                                idct8(&block, quant, target, stride);
                                block = [0; 64];
                            }
                            if gray && !inside {
                                let w = (width - x).min(8);
                                for row in 0..(height - y).min(8) {
                                    output[(y + row) * width + x..(y + row) * width + x + w]
                                        .copy_from_slice(&edge[row * 8..row * 8 + w]);
                                }
                            }
                        }
                    }
                }
            }
            reader.check()?;
            if !gray && my > 0 {
                self.emit(
                    &cache,
                    &layouts,
                    (my - 1) * max_v * 8,
                    max_v * 8,
                    method,
                    &mut output,
                );
            }
        }
        if !gray {
            self.emit(
                &cache,
                &layouts,
                (rows - 1) * max_v * 8,
                max_v * 8,
                method,
                &mut output,
            );
        }
        reader.finish()?;
        debug_assert_eq!(output.len(), count);
        Ok(Image {
            width: width as u32,
            height: height as u32,
            color_type: if gray {
                ColorType::Grayscale
            } else {
                ColorType::Rgb
            },
            bit_depth: 8,
            samples: Samples::U8(output),
        })
    }

    fn emit(
        &self,
        cache: &[u8],
        layout: &[Layout; 4],
        start: usize,
        rows: usize,
        method: Upsampling,
        out: &mut Vec<u8>,
    ) {
        let width = usize::from(self.frame.width);
        let height = usize::from(self.frame.height);
        let max_h = layout[..self.frame.components.len()]
            .iter()
            .map(|c| c.h)
            .max()
            .unwrap();
        let max_v = layout[..self.frame.components.len()]
            .iter()
            .map(|c| c.v)
            .max()
            .unwrap();
        let full = layout[..3].iter().all(|c| c.h == max_h && c.v == max_v);
        // 4:2:0 and 4:2:2: half-width chroma, half or full height.
        let half = layout[0].h == max_h
            && layout[0].v == max_v
            && layout[1].h * 2 == max_h
            && (layout[1].v * 2 == max_v || layout[1].v == max_v)
            && layout[2].h == layout[1].h
            && layout[2].v == layout[1].v;
        let half_height = layout[1].v * 2 == max_v;
        for y in start..(start + rows).min(height) {
            let samplers: [Row<'_>; 4] = core::array::from_fn(|i| {
                if i < self.frame.components.len() {
                    Row::new(cache, layout[i], y, max_h, max_v, method)
                } else {
                    Row::empty()
                }
            });
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
                for x in 0..width {
                    let values = core::array::from_fn(|i| {
                        if i < self.frame.components.len() {
                            samplers[i].sample(x)
                        } else {
                            0
                        }
                    });
                    let rgb = match self.color {
                        ColorSpace::YCbCr => ycbcr_to_rgb(values[..3].try_into().unwrap(), 8),
                        ColorSpace::Rgb => values[..3].try_into().unwrap(),
                        ColorSpace::Cmyk => cmyk_to_rgb(values, self.polarity, 8),
                        ColorSpace::Ycck => cmyk_to_rgb(ycck_to_cmyk(values, 8), self.polarity, 8),
                        _ => unreachable!(),
                    };
                    out.extend(rgb.map(|v| v as u8));
                }
            }
        }
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
            let iy = sy.div_euclid(dy);
            (
                iy.clamp(0, c.height as i32 - 1) as usize,
                (iy + 1).clamp(0, c.height as i32 - 1) as usize,
                sy.rem_euclid(dy),
                dy,
            )
        };
        let get = |y: usize| {
            let start = c.offset
                + (y / c.band_rows % 3) * c.band_rows * c.stride
                + y % c.band_rows * c.stride;
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
    #[inline]
    fn sample(self, x: usize) -> i32 {
        if self.method == Upsampling::Nearest {
            return i32::from(
                self.top[((2 * x + 1) * self.h / (2 * self.max_h)).min(self.top.len() - 1)],
            );
        }
        if self.h == self.max_h && self.fy == 0 {
            return i32::from(self.top[x]);
        }
        let sx = ((2 * x + 1) * self.h) as i32 - self.max_h as i32;
        let dx = 2 * self.max_h as i32;
        let ix = sx.div_euclid(dx);
        let fx = sx.rem_euclid(dx);
        let left = ix.clamp(0, self.top.len() as i32 - 1) as usize;
        let right = (ix + 1).clamp(0, self.top.len() as i32 - 1) as usize;
        let top = i32::from(self.top[left]) * (dx - fx) + i32::from(self.top[right]) * fx;
        let bottom = i32::from(self.bottom[left]) * (dx - fx) + i32::from(self.bottom[right]) * fx;
        (top * (self.dy - self.fy) + bottom * self.fy + dx * self.dy / 2) / (dx * self.dy)
    }
}
