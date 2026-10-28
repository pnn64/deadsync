//! Predictive JPEG-1 sample reconstruction, T.81 Annex H.
use crate::arithmetic::Conditioning;
use crate::decode::{parse_huffman, parse_quant};
use crate::entropy::{Bits, Huffman};
use crate::{
    ColorSpace, ComponentPlane, Error, Event, FrameHeader, IccProfile, Limits, MarkerReader,
    MetadataSegment, PlanarImage, Process, Result,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LosslessCoding {
    pub predictor: u8,
    pub point_transform: u8,
}

/// Native unsigned sample planes. At Pt=0 these are the exact source integers;
/// point-transformed streams reconstruct with Pt zero low bits. No DCT, level
/// shift, color conversion or precision scaling is applied by this API.
#[derive(Clone, Debug)]
pub struct LosslessImage<'a> {
    pub frame: FrameHeader,
    pub components: Vec<ComponentPlane>,
    pub coding: Vec<LosslessCoding>,
    pub metadata: Vec<MetadataSegment<'a>>,
    pub scans: usize,
    pub consumed: usize,
    pub(crate) limits: Limits,
    pub(crate) memory_bytes: usize,
}
impl<'a> LosslessImage<'a> {
    pub fn color_space(&self) -> ColorSpace {
        crate::reconstruct::infer_color(&self.frame, &self.metadata)
    }
    pub fn icc_profile(&self) -> Result<Option<IccProfile<'a>>> {
        IccProfile::from_segments(&self.metadata, self.limits.max_metadata_bytes)
    }
    /// Move sample buffers into the common native planar/color interface.
    /// Read borrowed metadata/profile views before consuming the image.
    pub fn into_planar(self) -> Result<PlanarImage> {
        let memory_bytes = self.validate()?.max(self.memory_bytes);
        Ok(PlanarImage {
            width: self.frame.width,
            height: self.frame.height,
            precision: self.frame.precision,
            color_space: self.color_space(),
            cmyk_polarity: if self.components.len() == 4 {
                crate::reconstruct::infer_cmyk_polarity(&self.metadata)
            } else {
                crate::CmykPolarity::Conventional
            },
            components: self.components,
            limits: self.limits,
            memory_bytes,
        })
    }
    pub(crate) fn validate(&self) -> Result<usize> {
        let f = &self.frame;
        if f.process != Process::Lossless
            || f.width == 0
            || f.height == 0
            || !(2..=16).contains(&f.precision)
            || f.components.is_empty()
            || f.components.len() > 255
            || self.components.len() != f.components.len()
            || self.coding.len() != f.components.len()
        {
            return Err(Error::Invalid("lossless image frame"));
        }
        if f.width as usize * f.height as usize > self.limits.max_pixels {
            return Err(Error::LimitExceeded("pixels"));
        }
        for (i, c) in f.components.iter().enumerate() {
            if !(1..=4).contains(&c.horizontal_sampling)
                || !(1..=4).contains(&c.vertical_sampling)
                || c.quantization_table != 0
                || f.components[..i].iter().any(|p| p.id == c.id)
            {
                return Err(Error::Invalid("lossless component specification"));
            }
        }
        let (max_h, max_v) = sampling(f);
        let maximum = (1u32 << f.precision) - 1;
        let mut bytes = f.components.len()
            * (core::mem::size_of::<ComponentPlane>()
                + core::mem::size_of::<LosslessCoding>()
                + core::mem::size_of::<crate::ComponentSpec>());
        for (i, c) in self.components.iter().enumerate() {
            if c.spec != f.components[i]
                || c.width
                    != (f.width as usize * c.spec.horizontal_sampling as usize).div_ceil(max_h)
                || c.height
                    != (f.height as usize * c.spec.vertical_sampling as usize).div_ceil(max_v)
                || c.width.checked_mul(c.height) != Some(c.samples.len())
                || c.samples.iter().any(|&v| u32::from(v) > maximum)
                || !(1..=7).contains(&self.coding[i].predictor)
                || self.coding[i].point_transform >= f.precision
            {
                return Err(Error::Invalid("lossless image samples or coding"));
            }
            bytes = bytes
                .checked_add(c.samples.capacity().saturating_mul(2))
                .ok_or(Error::LimitExceeded("lossless samples"))?;
        }
        let mut metadata_bytes = 0usize;
        let mut previous = 0;
        for s in &self.metadata {
            if !matches!(s.marker, 0xe0..=0xef | 0xfe)
                || s.data.len() > 65533
                || s.preceding_scans < previous
            {
                return Err(Error::Invalid("lossless metadata"));
            }
            previous = s.preceding_scans;
            metadata_bytes = metadata_bytes
                .checked_add(s.data.len().saturating_add(core::mem::size_of_val(s)))
                .ok_or(Error::LimitExceeded("metadata"))?;
        }
        if metadata_bytes > self.limits.max_metadata_bytes {
            return Err(Error::LimitExceeded("metadata"));
        }
        bytes = bytes
            .checked_add(metadata_bytes)
            .ok_or(Error::LimitExceeded("lossless samples"))?;
        if bytes > self.limits.max_memory_bytes {
            return Err(Error::LimitExceeded("lossless samples"));
        }
        Ok(bytes)
    }
}

pub(crate) fn sampling(frame: &FrameHeader) -> (usize, usize) {
    (
        frame
            .components
            .iter()
            .map(|c| c.horizontal_sampling as usize)
            .max()
            .unwrap(),
        frame
            .components
            .iter()
            .map(|c| c.vertical_sampling as usize)
            .max()
            .unwrap(),
    )
}

pub(crate) fn predictor(selection: u8, a: i32, b: i32, c: i32) -> i32 {
    match selection {
        1 => a,
        2 => b,
        3 => c,
        4 => a + b - c,
        5 => a + ((b - c) >> 1),
        6 => b + ((a - c) >> 1),
        7 => (a + b) >> 1,
        _ => unreachable!("validated predictor selection"),
    }
}

fn allocate(
    frame: &FrameHeader,
    limits: Limits,
    metadata_bytes: usize,
) -> Result<(Vec<ComponentPlane>, Vec<LosslessCoding>, usize)> {
    if frame.width as usize * frame.height as usize > limits.max_pixels {
        return Err(Error::LimitExceeded("pixels"));
    }
    let (max_h, max_v) = sampling(frame);
    let cols = (frame.width as usize).div_ceil(max_h);
    let rows = (frame.height as usize).div_ceil(max_v);
    let mut bytes = frame.components.len()
        * (core::mem::size_of::<ComponentPlane>()
            + core::mem::size_of::<LosslessCoding>()
            + core::mem::size_of::<crate::ComponentSpec>());
    for c in &frame.components {
        let count = cols
            .checked_mul(rows)
            .and_then(|n| n.checked_mul(c.horizontal_sampling as usize))
            .and_then(|n| n.checked_mul(c.vertical_sampling as usize))
            .and_then(|n| n.checked_mul(2))
            .ok_or(Error::LimitExceeded("lossless samples"))?;
        bytes = bytes
            .checked_add(count)
            .ok_or(Error::LimitExceeded("lossless samples"))?;
    }
    if bytes
        .checked_add(metadata_bytes)
        .is_none_or(|n| n > limits.max_memory_bytes)
    {
        return Err(Error::LimitExceeded("lossless samples"));
    }
    let mut components = Vec::new();
    let mut coding = Vec::new();
    components
        .try_reserve_exact(frame.components.len())
        .map_err(|_| Error::Allocation)?;
    coding
        .try_reserve_exact(frame.components.len())
        .map_err(|_| Error::Allocation)?;
    for &spec in &frame.components {
        let length =
            cols * rows * spec.horizontal_sampling as usize * spec.vertical_sampling as usize;
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(length)
            .map_err(|_| Error::Allocation)?;
        samples.resize(length, 0);
        components.push(ComponentPlane {
            spec,
            width: (frame.width as usize * spec.horizontal_sampling as usize).div_ceil(max_h),
            height: (frame.height as usize * spec.vertical_sampling as usize).div_ceil(max_v),
            samples,
        });
        coding.push(LosslessCoding::default());
    }
    Ok((components, coding, bytes))
}

/// Decode SOF3 predictive lossless Huffman JPEG into exact native sample planes.
pub fn decode_lossless(input: &[u8], limits: Limits) -> Result<LosslessImage<'_>> {
    let mut reader = MarkerReader::new(input)?;
    let mut frame = None;
    let mut components = Vec::new();
    let mut coding = Vec::new();
    let mut metadata = Vec::new();
    let mut memory_bytes = 0usize;
    let mut metadata_bytes = 0usize;
    let mut tables: [Option<Huffman>; 8] = core::array::from_fn(|_| None);
    let mut quant = [None; 4];
    let mut conditioning = Conditioning::default();
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
                marker: 0xc4, data, ..
            } => parse_huffman(data, &mut tables)?,
            Event::Segment {
                marker: 0xdb, data, ..
            } => parse_quant(data, &mut quant)?,
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
                    return Err(Error::LimitExceeded("lossless samples and metadata"));
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
                if header.process != Process::Lossless {
                    return Err(Error::Unsupported("lossless Huffman frame"));
                }
                if header.height == 0 {
                    return Err(Error::Unsupported("DNL height"));
                }
                (components, coding, memory_bytes) = allocate(&header, limits, metadata_bytes)?;
                frame = Some(header);
            }
            Event::Scan {
                header, entropy, ..
            } => {
                scans += 1;
                if scans > limits.max_scans {
                    return Err(Error::LimitExceeded("scans"));
                }
                scan(
                    header,
                    entropy,
                    frame.as_ref().ok_or(Error::Invalid("scan before frame"))?,
                    &mut components,
                    &mut coding,
                    &tables,
                    interval,
                )?;
            }
            Event::Segment { marker: 0xdc, .. } => return Err(Error::Unsupported("DNL height")),
            Event::Segment {
                marker: 0xde | 0xdf,
                ..
            } => return Err(Error::Unsupported("hierarchical process")),
            Event::Segment { .. } => return Err(Error::Invalid("reserved or unexpected marker")),
        }
    }
    if !ended {
        return Err(Error::Truncated);
    }
    let frame = frame.ok_or(Error::Invalid("missing frame"))?;
    if coding.iter().any(|c| c.predictor == 0) {
        return Err(Error::Invalid("missing component scan"));
    }
    let (max_h, _) = sampling(&frame);
    for (c, code) in components.iter_mut().zip(&coding) {
        let stride = (frame.width as usize).div_ceil(max_h) * c.spec.horizontal_sampling as usize;
        for y in 0..c.height {
            c.samples
                .copy_within(y * stride..y * stride + c.width, y * c.width);
        }
        c.samples.truncate(c.width * c.height);
        for value in &mut c.samples {
            *value <<= code.point_transform;
        }
    }
    Ok(LosslessImage {
        frame,
        components,
        coding,
        metadata,
        scans,
        consumed: reader.consumed(),
        limits,
        memory_bytes: memory_bytes + metadata_bytes,
    })
}

#[allow(clippy::too_many_arguments)]
fn scan(
    header: &[u8],
    entropy: &[u8],
    frame: &FrameHeader,
    components: &mut [ComponentPlane],
    coding: &mut [LosslessCoding],
    tables: &[Option<Huffman>; 8],
    interval: usize,
) -> Result<()> {
    if header.len() < 4 {
        return Err(Error::Invalid("SOS length"));
    }
    let n = header[0] as usize;
    if !(1..=4).contains(&n) || header.len() != 4 + 2 * n {
        return Err(Error::Invalid("SOS components"));
    }
    let selection = header[1 + 2 * n];
    let point = header[3 + 2 * n] & 15;
    if !(1..=7).contains(&selection)
        || header[2 + 2 * n] != 0
        || header[3 + 2 * n] >> 4 != 0
        || point >= frame.precision
    {
        return Err(Error::Invalid("lossless scan parameters"));
    }
    let mut selected = [(0usize, 0usize); 4];
    let mut blocks = 0;
    for (j, pair) in header[1..1 + 2 * n].as_chunks::<2>().0.iter().enumerate() {
        let i = frame
            .components
            .iter()
            .position(|c| c.id == pair[0])
            .ok_or(Error::Invalid("SOS component id"))?;
        let table = (pair[1] >> 4) as usize;
        if table > 3 || pair[1] & 15 != 0 || tables[table].is_none() {
            return Err(Error::Invalid("lossless entropy table selector"));
        }
        if coding[i].predictor != 0 || selected[..j].iter().any(|s| s.0 == i) {
            return Err(Error::Invalid("duplicate lossless component scan"));
        }
        if j != 0 && i < selected[j - 1].0 {
            return Err(Error::Invalid("SOS component order"));
        }
        selected[j] = (i, table);
        blocks += frame.components[i].horizontal_sampling as usize
            * frame.components[i].vertical_sampling as usize;
    }
    if n > 1 && blocks > 10 {
        return Err(Error::Invalid("more than ten samples per MCU"));
    }
    let (max_h, max_v) = sampling(frame);
    let (cols, rows) = if n == 1 {
        let c = &components[selected[0].0];
        (c.width, c.height)
    } else {
        (
            (frame.width as usize).div_ceil(max_h),
            (frame.height as usize).div_ceil(max_v),
        )
    };
    if interval != 0 && !interval.is_multiple_of(cols) {
        return Err(Error::Invalid("lossless restart row boundary"));
    }
    let mut bits = Bits::new(entropy);
    let mut restart = 0;
    let mut first_row = 0;
    let maximum = (1u32 << (frame.precision - point)) - 1;
    let initial = 1i32 << (frame.precision - point - 1);
    for mcu in 0..cols * rows {
        if interval != 0 && mcu != 0 && mcu % interval == 0 {
            bits.restart(restart)?;
            restart = (restart + 1) & 7;
            first_row = mcu / cols;
        }
        for &(i, table) in &selected[..n] {
            let c = &mut components[i];
            let (h, v) = if n == 1 {
                (1, 1)
            } else {
                (
                    c.spec.horizontal_sampling as usize,
                    c.spec.vertical_sampling as usize,
                )
            };
            let stride =
                (frame.width as usize).div_ceil(max_h) * c.spec.horizontal_sampling as usize;
            for yy in 0..v {
                for xx in 0..h {
                    let x = mcu % cols * h + xx;
                    let y = mcu / cols * v + yy;
                    let offset = y * stride + x;
                    let a = if x == 0 {
                        0
                    } else {
                        i32::from(c.samples[offset - 1])
                    };
                    let b = if y == 0 {
                        0
                    } else {
                        i32::from(c.samples[offset - stride])
                    };
                    let cc = if x == 0 || y == 0 {
                        0
                    } else {
                        i32::from(c.samples[offset - stride - 1])
                    };
                    let prediction = if y == first_row * v {
                        if x == 0 { initial } else { a }
                    } else if x == 0 {
                        b
                    } else {
                        predictor(selection, a, b, cc)
                    };
                    let size = tables[table].as_ref().unwrap().decode(&mut bits)?;
                    if size > 16 {
                        return Err(Error::Invalid("lossless difference category"));
                    }
                    let difference = if size == 16 {
                        32768
                    } else {
                        bits.receive(size)?
                    };
                    let value = (prediction + difference) as u16;
                    if u32::from(value) > maximum {
                        return Err(Error::Invalid("lossless sample range"));
                    }
                    c.samples[offset] = value;
                }
            }
        }
    }
    bits.finish()?;
    for &(i, _) in &selected[..n] {
        coding[i] = LosslessCoding {
            predictor: selection,
            point_transform: point,
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn predictors_keep_full_precision_and_arithmetic_shifts() {
        assert_eq!(
            (1..=7)
                .map(|s| predictor(s, 40000, 60000, 10000))
                .collect::<Vec<_>>(),
            [40000, 60000, 10000, 90000, 65000, 75000, 50000]
        );
        assert_eq!(
            (1..=7).map(|s| predictor(s, 1, 0, 3)).collect::<Vec<_>>(),
            [1, 0, 3, -2, -1, -1, 0]
        );
    }
}
