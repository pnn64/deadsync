//! Hierarchical frame parsing and reference reconstruction, T.81 Annex J.
use crate::arithmetic::Conditioning;
use crate::decode::{allocate_components, decode_scan, parse_huffman, parse_quant};
use crate::entropy::Huffman;
use crate::{
    CoefficientComponent, ColorSpace, ComponentPlane, ComponentSpec, Error, Event, FrameHeader,
    IccProfile, Limits, LosslessCoding, MarkerReader, MetadataSegment, PlanarImage, Process,
    Result,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Expansion {
    pub horizontal: bool,
    pub vertical: bool,
}
impl Expansion {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() != 1 || data[0] >> 4 > 1 || data[0] & 15 > 1 || data[0] == 0 {
            return Err(Error::Invalid("EXP parameters"));
        }
        Ok(Self {
            horizontal: data[0] >> 4 != 0,
            vertical: data[0] & 15 != 0,
        })
    }
    fn any(self) -> bool {
        self.horizontal || self.vertical
    }
}

/// DHP describes the completed image, independently of any one frame process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HierarchyHeader {
    pub precision: u8,
    pub width: u16,
    pub height: u16,
    pub components: Vec<ComponentSpec>,
}
impl HierarchyHeader {
    pub fn parse(data: &[u8]) -> Result<Self> {
        // DHP has the lossless SOF parameter ranges and requires Tq=0.
        let frame = FrameHeader::parse(Process::Lossless as u8, data)?;
        Ok(Self {
            precision: frame.precision,
            width: frame.width,
            height: frame.height,
            components: frame.components,
        })
    }
    pub(crate) fn validate(&self, limits: Limits) -> Result<()> {
        if self.height == 0 {
            return Err(Error::Unsupported("DNL hierarchy height"));
        }
        if self.width == 0 || !(2..=16).contains(&self.precision) {
            return Err(Error::Invalid("DHP dimensions or precision"));
        }
        specs(&self.components, true)?;
        if self.width as usize * self.height as usize > limits.max_pixels {
            return Err(Error::LimitExceeded("hierarchical pixels"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum HierarchicalData {
    Dct(Vec<CoefficientComponent>),
    /// Differential sample planes contain signed two's-complement values
    /// modulo 65536, after Pt restoration, rather than unsigned P-bit samples.
    Lossless {
        components: Vec<ComponentPlane>,
        coding: Vec<LosslessCoding>,
    },
}
#[derive(Clone, Debug)]
pub struct HierarchicalFrame {
    pub frame: FrameHeader,
    pub expansion: Expansion,
    pub data: HierarchicalData,
    /// Source SOF position and global scan index (both zero-based).
    pub offset: usize,
    pub first_scan: usize,
    pub scans: usize,
}
#[derive(Clone, Debug)]
pub struct HierarchicalImage<'a> {
    pub header: HierarchyHeader,
    pub frames: Vec<HierarchicalFrame>,
    pub metadata: Vec<MetadataSegment<'a>>,
    pub scans: usize,
    pub consumed: usize,
    pub(crate) limits: Limits,
    pub(crate) memory_bytes: usize,
}

impl<'a> HierarchicalImage<'a> {
    /// Construct a hierarchy from native frame data without parsing a JPEG.
    /// Frame scan counts describe metadata positions; first_scan is assigned
    /// cumulatively. Each frame must declare at least one nominal source scan.
    pub fn from_frames(
        header: HierarchyHeader,
        mut frames: Vec<HierarchicalFrame>,
        metadata: Vec<MetadataSegment<'a>>,
        limits: Limits,
    ) -> Result<Self> {
        let mut scans = 0usize;
        for frame in &mut frames {
            frame.first_scan = scans;
            scans = scans
                .checked_add(frame.scans)
                .ok_or(Error::LimitExceeded("scans"))?;
        }
        let mut image = Self {
            header,
            frames,
            metadata,
            scans,
            consumed: 0,
            limits,
            memory_bytes: 0,
        };
        image.memory_bytes = image.validate()?;
        Ok(image)
    }
    pub fn encode(&self, options: crate::HierarchicalOptions<'_>) -> Result<Vec<u8>> {
        crate::hierarchical_encode::encode_hierarchical(self, options)
    }
    pub fn encode_with_metadata(
        &self,
        options: crate::HierarchicalOptions<'_>,
        metadata: crate::MetadataOptions<'_>,
    ) -> Result<Vec<u8>> {
        crate::hierarchical_encode::encode_hierarchical_with_metadata(self, options, metadata)
    }
    pub fn color_space(&self) -> ColorSpace {
        crate::reconstruct::infer_components(&self.header.components, &self.metadata)
    }
    pub fn icc_profile(&self) -> Result<Option<IccProfile<'a>>> {
        IccProfile::from_segments(&self.metadata, self.limits.max_metadata_bytes)
    }

    /// Compose native reference planes with Annex J's modulo-65536 arithmetic.
    /// Raw values retain out-of-range DCT reconstruction results. No color
    /// conversion or final unsigned output clipping is performed by this view.
    pub fn reconstruct_reference(&self) -> Result<Vec<ComponentPlane>> {
        let stored = self.validate()?.max(self.memory_bytes);
        let record_bytes =
            self.header.components.len() * core::mem::size_of::<Option<ComponentPlane>>();
        check_memory(stored, record_bytes, self.limits)?;
        let mut planes = Vec::new();
        planes
            .try_reserve_exact(self.header.components.len())
            .map_err(|_| Error::Allocation)?;
        planes.resize_with(self.header.components.len(), || None::<ComponentPlane>);
        let mut live = record_bytes;
        for frame in &self.frames {
            let (max_h, max_v) = crate::lossless::sampling(&frame.frame);
            for (local, spec) in frame.frame.components.iter().enumerate() {
                let index = self
                    .header
                    .components
                    .iter()
                    .position(|c| c.id == spec.id)
                    .unwrap();
                let width = (frame.frame.width as usize * spec.horizontal_sampling as usize)
                    .div_ceil(max_h);
                let height =
                    (frame.frame.height as usize * spec.vertical_sampling as usize).div_ceil(max_v);
                let bytes = width
                    .checked_mul(height)
                    .and_then(|n| n.checked_mul(2))
                    .ok_or(Error::LimitExceeded("hierarchical reconstruction"))?;
                check_memory(
                    stored,
                    live.checked_add(bytes)
                        .ok_or(Error::LimitExceeded("hierarchical reconstruction"))?,
                    self.limits,
                )?;
                let mut samples = Vec::new();
                samples
                    .try_reserve_exact(width * height)
                    .map_err(|_| Error::Allocation)?;
                samples.resize(width * height, 0);
                let previous = planes[index].as_ref();
                let reference = |x, y| {
                    if frame.frame.process.is_differential() {
                        expanded(previous.unwrap(), x, y, frame.expansion)
                    } else {
                        0
                    }
                };
                match &frame.data {
                    HierarchicalData::Lossless { components, .. } => {
                        for (position, &v) in components[local].samples.iter().enumerate() {
                            samples[position] =
                                v.wrapping_add(reference(position % width, position / width));
                        }
                    }
                    HierarchicalData::Dct(components) => {
                        let c = &components[local];
                        for by in 0..height.div_ceil(8) {
                            for bx in 0..width.div_ceil(8) {
                                let block = &c.blocks[by * c.blocks_wide + bx];
                                let values = if frame.frame.process.is_differential() {
                                    crate::transform::idct_differential(block, &c.quantization)
                                } else {
                                    crate::transform::idct(
                                        block,
                                        &c.quantization,
                                        frame.frame.precision,
                                    )
                                };
                                for y in 0..(height - by * 8).min(8) {
                                    for x in 0..(width - bx * 8).min(8) {
                                        let (px, py) = (bx * 8 + x, by * 8 + y);
                                        samples[py * width + px] =
                                            values[y * 8 + x].wrapping_add(reference(px, py));
                                    }
                                }
                            }
                        }
                    }
                }
                live += bytes;
                if let Some(old) = planes[index].take() {
                    live -= old.samples.capacity() * 2;
                }
                planes[index] = Some(ComponentPlane {
                    spec: self.header.components[index],
                    width,
                    height,
                    samples,
                });
            }
        }
        check_memory(
            stored,
            live.checked_add(self.header.components.len() * core::mem::size_of::<ComponentPlane>())
                .ok_or(Error::LimitExceeded("hierarchical reconstruction"))?,
            self.limits,
        )?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(planes.len())
            .map_err(|_| Error::Allocation)?;
        for plane in planes {
            output.push(plane.unwrap());
        }
        Ok(output)
    }

    /// Convert the completed reference image to native unsigned P-bit planes.
    /// DCT excursions are clipped for display; raw modulo values remain
    /// available through `reconstruct_reference`. Pure lossless Pt=0 is exact.
    pub fn reconstruct(&self) -> Result<PlanarImage> {
        let mut components = self.reconstruct_reference()?;
        if self.header.precision < 16 {
            let max = (1i32 << self.header.precision) - 1;
            for c in &mut components {
                for v in &mut c.samples {
                    *v = i32::from(*v as i16).clamp(0, max) as u16;
                }
            }
        }
        let sample_bytes = components
            .iter()
            .try_fold(0usize, |n, c| n.checked_add(c.samples.capacity() * 2))
            .ok_or(Error::LimitExceeded("hierarchical reconstruction"))?;
        let sample_bytes = sample_bytes
            .checked_add(components.capacity() * core::mem::size_of::<ComponentPlane>())
            .ok_or(Error::LimitExceeded("hierarchical reconstruction"))?;
        let memory_bytes = self
            .validate()?
            .max(self.memory_bytes)
            .checked_add(sample_bytes)
            .ok_or(Error::LimitExceeded("hierarchical reconstruction"))?;
        Ok(PlanarImage {
            width: self.header.width,
            height: self.header.height,
            precision: self.header.precision,
            color_space: self.color_space(),
            cmyk_polarity: if components.len() == 4 {
                crate::reconstruct::infer_cmyk_polarity(&self.metadata)
            } else {
                crate::CmykPolarity::Conventional
            },
            components,
            limits: self.limits,
            memory_bytes,
        })
    }
    pub(crate) fn validate(&self) -> Result<usize> {
        self.header.validate(self.limits)?;
        if self.frames.is_empty() {
            return Err(Error::Invalid("missing hierarchical frames"));
        }
        if self.frames.len() > self.limits.max_scans {
            return Err(Error::LimitExceeded("hierarchical frames"));
        }
        let mut rules = Rules::new(&self.header);
        let mut scans = 0;
        for frame in &self.frames {
            rules.accept(&frame.frame, frame.expansion)?;
            if frame.first_scan != scans || frame.scans == 0 {
                return Err(Error::Invalid("hierarchical frame scan counts"));
            }
            scans = scans
                .checked_add(frame.scans)
                .ok_or(Error::LimitExceeded("scans"))?;
            validate_data(frame)?;
        }
        if scans > self.limits.max_scans {
            return Err(Error::LimitExceeded("scans"));
        }
        if scans != self.scans {
            return Err(Error::Invalid("hierarchical scan count"));
        }
        rules.finish()?;
        let mut last = 0;
        let mut meta = 0usize;
        for m in &self.metadata {
            if !matches!(m.marker, 0xe0..=0xef | 0xfe)
                || m.data.len() > 65533
                || m.preceding_scans < last
                || m.preceding_scans > self.scans
            {
                return Err(Error::Invalid("hierarchical metadata"));
            }
            last = m.preceding_scans;
            meta = meta
                .checked_add(m.data.len() + core::mem::size_of_val(m))
                .ok_or(Error::LimitExceeded("metadata"))?;
        }
        if meta > self.limits.max_metadata_bytes {
            return Err(Error::LimitExceeded("metadata"));
        }
        let bytes = storage(&self.header, &self.frames, &self.metadata)?;
        check_memory(bytes, 0, self.limits)?;
        Ok(bytes)
    }
}

fn check_memory(stored: usize, extra: usize, limits: Limits) -> Result<()> {
    if stored
        .checked_add(extra)
        .is_none_or(|n| n > limits.max_memory_bytes)
    {
        Err(Error::LimitExceeded("hierarchical storage"))
    } else {
        Ok(())
    }
}
fn specs(components: &[ComponentSpec], lossless: bool) -> Result<()> {
    if components.is_empty() || components.len() > 255 {
        return Err(Error::Invalid("hierarchical component count"));
    }
    let mut seen = [false; 256];
    for c in components {
        if !(1..=4).contains(&c.horizontal_sampling)
            || !(1..=4).contains(&c.vertical_sampling)
            || c.quantization_table > 3
            || (lossless && c.quantization_table != 0)
            || seen[c.id as usize]
        {
            return Err(Error::Invalid("hierarchical component specification"));
        }
        seen[c.id as usize] = true;
    }
    Ok(())
}
pub(crate) struct Rules {
    width: u16,
    height: u16,
    precision: u8,
    component_count: usize,
    final_sizes: [(usize, usize); 255],
    indices: [u8; 256],
    sizes: [(usize, usize); 255],
    terminal: [bool; 255],
    base_lossless: Option<bool>,
    arithmetic: Option<bool>,
}
impl Rules {
    pub(crate) fn new(header: &HierarchyHeader) -> Self {
        let mut indices = [255; 256];
        let mut final_sizes = [(0, 0); 255];
        let max_h = header
            .components
            .iter()
            .map(|c| c.horizontal_sampling as usize)
            .max()
            .unwrap();
        let max_v = header
            .components
            .iter()
            .map(|c| c.vertical_sampling as usize)
            .max()
            .unwrap();
        for (i, c) in header.components.iter().enumerate() {
            indices[c.id as usize] = i as u8;
            final_sizes[i] = (
                (header.width as usize * c.horizontal_sampling as usize).div_ceil(max_h),
                (header.height as usize * c.vertical_sampling as usize).div_ceil(max_v),
            );
        }
        Self {
            width: header.width,
            height: header.height,
            precision: header.precision,
            component_count: header.components.len(),
            final_sizes,
            indices,
            sizes: [(0, 0); 255],
            terminal: [false; 255],
            base_lossless: None,
            arithmetic: None,
        }
    }
    fn accept(&mut self, frame: &FrameHeader, expansion: Expansion) -> Result<()> {
        self.accept_process(frame, expansion, frame.process)
    }
    pub(crate) fn accept_process(
        &mut self,
        frame: &FrameHeader,
        expansion: Expansion,
        process: Process,
    ) -> Result<()> {
        specs(&frame.components, process.is_lossless())?;
        if frame.height == 0 {
            return Err(Error::Unsupported("DNL hierarchy height"));
        }
        if frame.width == 0
            || frame.width > self.width
            || frame.height > self.height
            || frame.precision != self.precision
            || (process == Process::Baseline && frame.precision != 8)
            || (!process.is_lossless() && !matches!(frame.precision, 8 | 12))
            || (process.is_progressive() && frame.components.len() > 4)
        {
            return Err(Error::Invalid(
                "hierarchical frame dimensions, precision or components",
            ));
        }
        if expansion.any() && !process.is_differential() {
            return Err(Error::Invalid("EXP before non-differential frame"));
        }
        if process != Process::Baseline {
            let arithmetic = process.is_arithmetic();
            if self.arithmetic.is_some_and(|a| a != arithmetic) {
                return Err(Error::Invalid("mixed hierarchical entropy processes"));
            }
            self.arithmetic = Some(arithmetic);
        }
        let max_h = frame
            .components
            .iter()
            .map(|c| c.horizontal_sampling as usize)
            .max()
            .unwrap();
        let max_v = frame
            .components
            .iter()
            .map(|c| c.vertical_sampling as usize)
            .max()
            .unwrap();
        for c in &frame.components {
            let index = self.indices[c.id as usize] as usize;
            if index == 255 {
                return Err(Error::Invalid("frame component absent from DHP"));
            }
            let width = (frame.width as usize * c.horizontal_sampling as usize).div_ceil(max_h);
            let height = (frame.height as usize * c.vertical_sampling as usize).div_ceil(max_v);
            if width > self.final_sizes[index].0 || height > self.final_sizes[index].1 {
                return Err(Error::Invalid("frame component exceeds DHP dimensions"));
            }
            let previous = self.sizes[index];
            if process.is_differential() {
                if previous.0 == 0 {
                    return Err(Error::Invalid("differential frame without reference"));
                }
                if self.terminal[index] {
                    return Err(Error::Invalid(
                        "frame follows final lossless differential frame",
                    ));
                }
                if self.base_lossless == Some(true) && !process.is_lossless() {
                    return Err(Error::Invalid("DCT differential frame after lossless base"));
                }
                let fits = |old: usize, new: usize, expand: bool| {
                    if expand {
                        // Frame/component rounding can discard more than one
                        // expanded edge sample with fractional factors. Keep
                        // the visible grid within the doubled reference.
                        (new > old && new <= old * 2) || (old == 1 && new == 1)
                    } else {
                        new == old
                    }
                };
                if !fits(previous.0, width, expansion.horizontal)
                    || !fits(previous.1, height, expansion.vertical)
                {
                    return Err(Error::Invalid(
                        "hierarchical reference expansion dimensions",
                    ));
                }
                if self.base_lossless == Some(false) && process.is_lossless() {
                    self.terminal[index] = true;
                }
            } else {
                if previous.0 != 0 {
                    return Err(Error::Invalid("repeated non-differential component"));
                }
                let lossless = process.is_lossless();
                if self.base_lossless.is_some_and(|l| l != lossless) {
                    return Err(Error::Invalid("mixed hierarchical initial coding families"));
                }
                self.base_lossless = Some(lossless);
            }
            self.sizes[index] = (width, height);
        }
        Ok(())
    }
    pub(crate) fn finish(&self) -> Result<()> {
        for i in 0..self.component_count {
            if self.sizes[i] != self.final_sizes[i] {
                return Err(Error::Invalid(
                    "incomplete hierarchical component resolution",
                ));
            }
        }
        Ok(())
    }
}

fn validate_data(frame: &HierarchicalFrame) -> Result<()> {
    let (max_h, max_v) = crate::lossless::sampling(&frame.frame);
    let dims = |s: ComponentSpec| {
        (
            (frame.frame.width as usize * s.horizontal_sampling as usize).div_ceil(max_h),
            (frame.frame.height as usize * s.vertical_sampling as usize).div_ceil(max_v),
        )
    };
    match &frame.data {
        HierarchicalData::Dct(components) => {
            if frame.frame.process.is_lossless() || components.len() != frame.frame.components.len()
            {
                return Err(Error::Invalid("hierarchical DCT component count"));
            }
            for (i, c) in components.iter().enumerate() {
                if c.spec != frame.frame.components[i]
                    || (c.width, c.height) != dims(c.spec)
                    || c.blocks_wide < c.width.div_ceil(8)
                    || c.blocks_high < c.height.div_ceil(8)
                    || c.blocks_wide.checked_mul(c.blocks_high) != Some(c.blocks.len())
                    || c.quantization.contains(&0)
                    || !(0..=13).contains(&c.successive_approximation[0])
                {
                    return Err(Error::Invalid("hierarchical coefficient grid"));
                }
            }
        }
        HierarchicalData::Lossless { components, coding } => {
            if !frame.frame.process.is_lossless()
                || components.len() != frame.frame.components.len()
                || coding.len() != components.len()
            {
                return Err(Error::Invalid("hierarchical sample component count"));
            }
            let max = (1u32 << frame.frame.precision) - 1;
            for (i, (c, code)) in components.iter().zip(coding).enumerate() {
                if c.spec != frame.frame.components[i]
                    || (c.width, c.height) != dims(c.spec)
                    || c.width.checked_mul(c.height) != Some(c.samples.len())
                    || code.point_transform >= frame.frame.precision
                    || (if frame.frame.process.is_differential() {
                        code.predictor != 0
                    } else {
                        !(1..=7).contains(&code.predictor)
                    })
                    || (!frame.frame.process.is_differential()
                        && c.samples.iter().any(|&v| u32::from(v) > max))
                {
                    return Err(Error::Invalid("hierarchical sample grid or coding"));
                }
            }
        }
    }
    Ok(())
}
fn frame_storage(frame: &HierarchicalFrame) -> Result<usize> {
    let mut bytes = frame.frame.components.capacity() * core::mem::size_of::<ComponentSpec>();
    let mut add = |n: usize| {
        bytes = bytes
            .checked_add(n)
            .ok_or(Error::LimitExceeded("hierarchical storage"))?;
        Ok(())
    };
    match &frame.data {
        HierarchicalData::Dct(c) => {
            add(c.capacity() * core::mem::size_of::<CoefficientComponent>())?;
            for c in c {
                add(c
                    .blocks
                    .capacity()
                    .checked_mul(128)
                    .ok_or(Error::LimitExceeded("hierarchical storage"))?)?;
            }
        }
        HierarchicalData::Lossless { components, coding } => {
            add(components.capacity() * core::mem::size_of::<ComponentPlane>())?;
            add(coding.capacity() * core::mem::size_of::<LosslessCoding>())?;
            for c in components {
                add(c
                    .samples
                    .capacity()
                    .checked_mul(2)
                    .ok_or(Error::LimitExceeded("hierarchical storage"))?)?;
            }
        }
    }
    Ok(bytes)
}
fn storage(
    header: &HierarchyHeader,
    frames: &Vec<HierarchicalFrame>,
    metadata: &Vec<MetadataSegment<'_>>,
) -> Result<usize> {
    let mut bytes = header.components.capacity() * core::mem::size_of::<ComponentSpec>()
        + frames.capacity() * core::mem::size_of::<HierarchicalFrame>()
        + metadata.capacity() * core::mem::size_of::<MetadataSegment<'_>>();
    for f in frames {
        bytes = bytes
            .checked_add(frame_storage(f)?)
            .ok_or(Error::LimitExceeded("hierarchical storage"))?;
    }
    for m in metadata {
        bytes = bytes
            .checked_add(m.data.len())
            .ok_or(Error::LimitExceeded("hierarchical storage"))?;
    }
    Ok(bytes)
}

fn expanded(c: &ComponentPlane, x: usize, y: usize, expansion: Expansion) -> u16 {
    let sx = if expansion.horizontal { x / 2 } else { x };
    let sy = if expansion.vertical { y / 2 } else { y };
    let get = |xx: usize, yy: usize| {
        u32::from(c.samples[yy.min(c.height - 1) * c.width + xx.min(c.width - 1)])
    };
    let horizontal = |yy| {
        if expansion.horizontal && !x.is_multiple_of(2) {
            (get(sx, yy) + get(sx + 1, yy)) / 2
        } else {
            get(sx, yy)
        }
    };
    if expansion.vertical && !y.is_multiple_of(2) {
        ((horizontal(sy) + horizontal(sy + 1)) / 2) as u16
    } else {
        horizontal(sy) as u16
    }
}

struct Active {
    frame: HierarchicalFrame,
    seen: Vec<bool>,
}
fn finish(active: Option<Active>, frames: &mut Vec<HierarchicalFrame>) -> Result<()> {
    if let Some(mut active) = active {
        if active.frame.scans == 0 || active.seen.iter().any(|&v| !v) {
            return Err(Error::Invalid("missing hierarchical component scan"));
        }
        if let HierarchicalData::Lossless { components, coding } = &mut active.frame.data {
            if coding.iter().any(|c| c.point_transform == u8::MAX) {
                return Err(Error::Invalid("missing hierarchical lossless scan"));
            }
            crate::lossless::finish_samples(&active.frame.frame, components, coding);
        }
        frames.push(active.frame);
    }
    Ok(())
}

/// Decode a DHP hierarchy while retaining every frame's original coefficients
/// or native sample differences. Tables persist across frames; APPn/COM borrow
/// the input and retain global scan positions. DNL remains unsupported.
pub fn decode_hierarchical(input: &[u8], limits: Limits) -> Result<HierarchicalImage<'_>> {
    let mut reader = MarkerReader::new(input)?;
    let mut header = None;
    let mut rules = None;
    let mut frames = Vec::new();
    let mut metadata = Vec::new();
    let mut active = None::<Active>;
    let mut quant = [None; 4];
    let mut huffman: [Option<Huffman>; 8] = core::array::from_fn(|_| None);
    let mut conditioning = Conditioning::default();
    let mut interval = 0;
    let mut pending = None;
    let mut metadata_bytes = 0usize;
    let mut scans = 0;
    let mut ended = false;
    for event in reader.by_ref() {
        match event? {
            Event::End { .. } => {
                finish(active.take(), &mut frames)?;
                ended = true;
                break;
            }
            Event::Segment {
                marker: 0xde, data, ..
            } => {
                if header.is_some() || !frames.is_empty() || active.is_some() {
                    return Err(Error::Invalid("duplicate or late DHP"));
                }
                let h = HierarchyHeader::parse(data)?;
                h.validate(limits)?;
                rules = Some(Rules::new(&h));
                header = Some(h);
            }
            Event::Segment {
                marker: 0xdf, data, ..
            } => {
                if header.is_none() || pending.is_some() || (frames.is_empty() && active.is_none())
                {
                    return Err(Error::Invalid("unexpected or repeated EXP"));
                }
                finish(active.take(), &mut frames)?;
                pending = Some(Expansion::parse(data)?);
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
                interval = u16::from_be_bytes(data.try_into().unwrap()) as usize;
            }
            Event::Segment {
                marker: marker @ (0xe0..=0xef | 0xfe),
                data,
                offset,
            } => {
                metadata_bytes = metadata_bytes
                    .checked_add(data.len() + core::mem::size_of::<MetadataSegment<'_>>())
                    .ok_or(Error::LimitExceeded("metadata"))?;
                if metadata_bytes > limits.max_metadata_bytes {
                    return Err(Error::LimitExceeded("metadata"));
                }
                if metadata_bytes > limits.max_memory_bytes {
                    return Err(Error::LimitExceeded("hierarchical metadata storage"));
                }
                metadata.try_reserve(1).map_err(|_| Error::Allocation)?;
                metadata.push(MetadataSegment {
                    marker,
                    data,
                    offset,
                    preceding_scans: scans,
                });
            }
            Event::Segment {
                marker,
                data,
                offset,
            } if Process::try_from(marker).is_ok() => {
                finish(active.take(), &mut frames)?;
                let header = header.as_ref().ok_or(Error::Invalid("frame before DHP"))?;
                if frames.len() >= limits.max_scans {
                    return Err(Error::LimitExceeded("hierarchical frames"));
                }
                let frame = FrameHeader::parse(marker, data)?;
                let expansion = pending.take().unwrap_or_default();
                rules.as_mut().unwrap().accept(&frame, expansion)?;
                frames.try_reserve_exact(1).map_err(|_| Error::Allocation)?;
                let used = storage(header, &frames, &metadata)?;
                let overhead = if frame.process.is_lossless() {
                    0
                } else {
                    frame.components.len()
                        * (core::mem::size_of::<CoefficientComponent>()
                            + core::mem::size_of::<ComponentSpec>()
                            + 1)
                };
                let available = limits
                    .max_memory_bytes
                    .checked_sub(used)
                    .and_then(|n| n.checked_sub(overhead))
                    .ok_or(Error::LimitExceeded("hierarchical storage"))?;
                let mut seen = Vec::new();
                let data = if frame.process.is_lossless() {
                    let (components, coding, _) = crate::lossless::allocate(
                        &frame,
                        Limits {
                            max_memory_bytes: available,
                            ..limits
                        },
                        0,
                    )?;
                    HierarchicalData::Lossless { components, coding }
                } else {
                    let (components, _) = allocate_components(
                        &frame,
                        Limits {
                            max_memory_bytes: available,
                            ..limits
                        },
                    )?;
                    seen.try_reserve_exact(frame.components.len())
                        .map_err(|_| Error::Allocation)?;
                    seen.resize(frame.components.len(), false);
                    HierarchicalData::Dct(components)
                };
                active = Some(Active {
                    frame: HierarchicalFrame {
                        frame,
                        expansion,
                        data,
                        offset,
                        first_scan: scans,
                        scans: 0,
                    },
                    seen,
                });
            }
            Event::Scan {
                header: scan_header,
                entropy,
                ..
            } => {
                let h = header.as_ref().ok_or(Error::Invalid("scan before DHP"))?;
                let current = active
                    .as_mut()
                    .ok_or(Error::Invalid("scan outside hierarchical frame"))?;
                scans += 1;
                if scans > limits.max_scans {
                    return Err(Error::LimitExceeded("scans"));
                }
                let used = storage(h, &frames, &metadata)?
                    .checked_add(frame_storage(&current.frame)?)
                    .and_then(|n| n.checked_add(current.seen.capacity()))
                    .ok_or(Error::LimitExceeded("hierarchical storage"))?;
                let available = limits
                    .max_memory_bytes
                    .checked_sub(used)
                    .ok_or(Error::LimitExceeded("hierarchical storage"))?;
                match &mut current.frame.data {
                    HierarchicalData::Dct(components) => decode_scan(
                        scan_header,
                        entropy,
                        &current.frame.frame,
                        components,
                        &mut current.seen,
                        &quant,
                        &huffman,
                        interval,
                        conditioning,
                    )?,
                    HierarchicalData::Lossless { components, coding } => crate::lossless::scan(
                        scan_header,
                        entropy,
                        &current.frame.frame,
                        components,
                        coding,
                        &huffman,
                        interval,
                        conditioning,
                        available,
                    )?,
                }
                current.frame.scans += 1;
            }
            Event::Segment { marker: 0xdc, .. } => {
                return Err(Error::Unsupported("DNL hierarchy height"));
            }
            Event::Segment { .. } => {
                return Err(Error::Invalid("reserved or unexpected hierarchical marker"));
            }
        }
        if let Some(h) = &header {
            let used = storage(h, &frames, &metadata)?;
            let active_bytes = if let Some(a) = &active {
                frame_storage(&a.frame)?
                    .checked_add(a.seen.capacity())
                    .ok_or(Error::LimitExceeded("hierarchical storage"))?
            } else {
                0
            };
            check_memory(used, active_bytes, limits)?;
        }
    }
    if !ended {
        return Err(Error::Truncated);
    }
    if pending.is_some() {
        return Err(Error::Invalid("EXP without following frame"));
    }
    let header = header.ok_or(Error::Invalid("missing DHP"))?;
    let memory_bytes = storage(&header, &frames, &metadata)?;
    let image = HierarchicalImage {
        header,
        frames,
        metadata,
        scans,
        consumed: reader.consumed(),
        limits,
        memory_bytes,
    };
    image.validate()?;
    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn annex_j_expansion_orders_truncation_and_edge_replication() {
        let c = ComponentPlane {
            spec: ComponentSpec {
                id: 1,
                horizontal_sampling: 1,
                vertical_sampling: 1,
                quantization_table: 0,
            },
            width: 2,
            height: 2,
            samples: vec![0, 1, 1, 2],
        };
        let expansion = Expansion {
            horizontal: true,
            vertical: true,
        };
        let actual: Vec<_> = (0..4)
            .flat_map(|y| (0..4).map(move |x| (x, y)))
            .map(|(x, y)| expanded(&c, x, y, expansion))
            .collect();
        assert_eq!(actual, [0, 0, 1, 1, 0, 0, 1, 1, 1, 1, 2, 2, 1, 1, 2, 2]);
    }
}
