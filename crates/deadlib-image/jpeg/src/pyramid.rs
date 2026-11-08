//! Source pyramids and matching decoder references, T.81 Annex J/K.5.
use crate::hierarchical::{Rules, expanded, visit_component};
use crate::lossless::sampling;
use crate::lossless_encode::intersect;
use crate::pixel_encode::{ColorTransform, quantization_tables, resample, validate_raster};
use crate::{
    CmykPolarity, CodingOptions, CoefficientComponent, ColorSpace, ComponentPlane, ComponentSpec,
    EncodeInput, EntropyCoding, Error, Expansion, FrameHeader, HierarchicalData, HierarchicalFrame,
    HierarchicalFrameOptions, HierarchicalImage, HierarchyHeader, Limits, LosslessOptions,
    MetadataOptions, MetadataSegment, Process, Result, Subsampling, ZIGZAG,
};
use deadlib_image::{ColorType, Image, Samples};

/// One frame in an automatically generated hierarchy. The first has no EXP;
/// later flags expand the preceding reference. Final dimensions are supplied by
/// the source; earlier dimensions are calculated backwards with ceil division.
#[derive(Clone, Copy, Debug)]
pub struct PyramidLevel<'a> {
    pub expansion: Expansion,
    pub coding: HierarchicalFrameOptions<'a>,
    /// DCT quality, 1..100; unused by predictive frames.
    pub quality: u8,
    /// Cap generated DCT quantizers at 255 rather than 65535.
    pub baseline_quantization: bool,
}
impl<'a> PyramidLevel<'a> {
    pub fn dct(expansion: Expansion, quality: u8, coding: CodingOptions<'a>) -> Self {
        Self {
            expansion,
            coding: HierarchicalFrameOptions::Dct(coding),
            quality,
            baseline_quantization: true,
        }
    }
    pub fn lossless(expansion: Expansion, coding: LosslessOptions<'a>) -> Self {
        Self {
            expansion,
            coding: HierarchicalFrameOptions::Lossless(coding),
            quality: 75,
            baseline_quantization: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PyramidOptions<'a> {
    pub levels: &'a [PyramidLevel<'a>],
    pub limits: Limits,
    pub max_output_bytes: usize,
    /// Leave DHP's height zero and infer the completed height from frames.
    pub omit_dhp_height: bool,
}
impl<'a> PyramidOptions<'a> {
    pub fn new(levels: &'a [PyramidLevel<'a>]) -> Self {
        Self {
            levels,
            limits: Limits::default(),
            max_output_bytes: 512 * 1024 * 1024,
            omit_dhp_height: false,
        }
    }
}

/// Explicit conversion/sampling before hierarchy coding. Defaults preserve
/// source integers and color space. Precision zero keeps original precision.
/// DCT frames require 8/12-bit output; predictive frames permit 2..16 bits.
#[derive(Clone, Copy, Debug)]
pub struct PyramidRasterOptions {
    pub precision: u8,
    pub subsampling: Subsampling,
    pub rgb_to_ycbcr: bool,
    pub cmyk_to_ycck: bool,
    pub cmyk_polarity: CmykPolarity,
}
impl Default for PyramidRasterOptions {
    fn default() -> Self {
        Self {
            precision: 0,
            subsampling: Subsampling::S444,
            rgb_to_ycbcr: false,
            cmyk_to_ycck: false,
            cmyk_polarity: CmykPolarity::Conventional,
        }
    }
}

#[derive(Clone, Copy)]
enum Source<'a> {
    Planes(&'a [ComponentPlane]),
    Raster {
        input: EncodeInput<'a>,
        transform: ColorTransform,
        channels: usize,
        max_h: usize,
        max_v: usize,
    },
}
impl Source<'_> {
    fn at(self, i: usize, spec: ComponentSpec, x: usize, y: usize) -> u16 {
        match self {
            Self::Planes(planes) => {
                let c = &planes[i];
                c.samples[y.min(c.height - 1) * c.width + x.min(c.width - 1)]
            }
            Self::Raster {
                input,
                transform,
                channels,
                max_h,
                max_v,
            } => resample(
                &input,
                transform,
                i,
                x,
                y,
                channels,
                spec.horizontal_sampling as usize,
                spec.vertical_sampling as usize,
                max_h,
                max_v,
            ),
        }
    }
}
struct Target {
    width: u16,
    height: u16,
    planes: Vec<ComponentPlane>,
}
#[derive(Clone, Copy)]
enum Wanted<'a, 'b> {
    Original(Source<'a>),
    Planes(&'b [ComponentPlane]),
}
impl Wanted<'_, '_> {
    fn at(self, i: usize, spec: ComponentSpec, x: usize, y: usize) -> u16 {
        match self {
            Self::Original(source) => source.at(i, spec, x, y),
            Self::Planes(planes) => Source::Planes(planes).at(i, spec, x, y),
        }
    }
}
fn wanted<'a, 'b>(
    source: Source<'a>,
    targets: &'b [Target],
    width: u16,
    height: u16,
) -> Wanted<'a, 'b> {
    targets
        .iter()
        .find(|t| t.width == width && t.height == height)
        .map_or(Wanted::Original(source), |t| Wanted::Planes(&t.planes))
}

struct Budget {
    used: usize,
    limit: usize,
}
impl Budget {
    fn add(&mut self, bytes: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(bytes)
            .ok_or(Error::LimitExceeded("pyramid storage"))?;
        if self.used > self.limit {
            return Err(Error::LimitExceeded("pyramid storage"));
        }
        Ok(())
    }
    fn vector<T>(&mut self, n: usize) -> Result<Vec<T>> {
        let bytes = n
            .checked_mul(core::mem::size_of::<T>())
            .ok_or(Error::LimitExceeded("pyramid storage"))?;
        self.add(bytes)?;
        let mut v = Vec::new();
        v.try_reserve_exact(n).map_err(|_| Error::Allocation)?;
        self.add((v.capacity() - n) * core::mem::size_of::<T>())?;
        Ok(v)
    }
    fn filled<T: Clone>(&mut self, n: usize, value: T) -> Result<Vec<T>> {
        let mut v = self.vector(n)?;
        v.resize(n, value);
        Ok(v)
    }
    fn release_planes(&mut self, planes: Vec<ComponentPlane>) {
        self.used -= planes.capacity() * core::mem::size_of::<ComponentPlane>();
        for c in &planes {
            self.used -= c.samples.capacity() * 2;
        }
        drop(planes);
    }
}
fn effective(options: PyramidOptions<'_>) -> Limits {
    let mut limits = options.limits;
    for level in options.levels {
        if let HierarchicalFrameOptions::Lossless(c) = level.coding {
            limits = intersect(limits, c.limits);
        }
    }
    limits
}
fn dimensions(frame: &FrameHeader, spec: ComponentSpec) -> (usize, usize) {
    let (h, v) = sampling(frame);
    (
        (frame.width as usize * spec.horizontal_sampling as usize).div_ceil(h),
        (frame.height as usize * spec.vertical_sampling as usize).div_ceil(v),
    )
}
fn planes(frame: &FrameHeader, budget: &mut Budget) -> Result<Vec<ComponentPlane>> {
    let mut output = budget.vector(frame.components.len())?;
    for &spec in &frame.components {
        let (width, height) = dimensions(frame, spec);
        output.push(ComponentPlane {
            spec: ComponentSpec {
                quantization_table: 0,
                ..spec
            },
            width,
            height,
            samples: budget.filled(width * height, 0)?,
        });
    }
    Ok(output)
}
fn downsample(
    source: Wanted<'_, '_>,
    frame: &FrameHeader,
    expansion: Expansion,
    budget: &mut Budget,
) -> Result<Vec<ComponentPlane>> {
    let mut output = planes(frame, budget)?;
    for (i, c) in output.iter_mut().enumerate() {
        for y in 0..c.height {
            for x in 0..c.width {
                let sx = x * if expansion.horizontal { 2 } else { 1 };
                let sy = y * if expansion.vertical { 2 } else { 1 };
                let horizontal: &[(isize, u32)] = if expansion.horizontal {
                    &[(-1, 1), (0, 2), (1, 1)]
                } else {
                    &[(0, 1)]
                };
                let vertical: &[(isize, u32)] = if expansion.vertical {
                    &[(-1, 1), (0, 2), (1, 1)]
                } else {
                    &[(0, 1)]
                };
                let mut sum = 0u32;
                let mut weight = 0;
                for &(dy, wy) in vertical {
                    for &(dx, wx) in horizontal {
                        sum += u32::from(source.at(
                            i,
                            c.spec,
                            sx.saturating_add_signed(dx),
                            sy.saturating_add_signed(dy),
                        )) * wx
                            * wy;
                        weight += wx * wy;
                    }
                }
                c.samples[y * c.width + x] = ((sum + weight / 2) / weight) as u16;
            }
        }
    }
    Ok(output)
}

fn headers(
    header: &HierarchyHeader,
    space: ColorSpace,
    options: PyramidOptions<'_>,
    budget: &mut Budget,
) -> Result<Vec<FrameHeader>> {
    let levels = options.levels;
    if levels.is_empty() || levels[0].expansion.horizontal || levels[0].expansion.vertical {
        return Err(Error::Invalid("pyramid first frame or empty progression"));
    }
    if levels.len() > options.limits.max_scans {
        return Err(Error::LimitExceeded("pyramid frames"));
    }
    let mut dims = budget.filled(levels.len(), (header.width, header.height))?;
    for i in (0..levels.len() - 1).rev() {
        let e = levels[i + 1].expansion;
        let (w, h) = dims[i + 1];
        dims[i] = (
            if e.horizontal { w.div_ceil(2) } else { w },
            if e.vertical { h.div_ceil(2) } else { h },
        );
    }
    let mut result = budget.vector(levels.len())?;
    let mut rules = Rules::new(header);
    let mut scans = 0usize;
    for (i, level) in levels.iter().enumerate() {
        let mut specs = budget.vector(header.components.len())?;
        specs.extend_from_slice(&header.components);
        let differential = i != 0;
        let mut frame = FrameHeader {
            process: Process::Lossless,
            precision: header.precision,
            width: dims[i].0,
            height: dims[i].1,
            components: specs,
        };
        let count = match level.coding {
            HierarchicalFrameOptions::Dct(coding) => {
                if !matches!(header.precision, 8 | 12) {
                    return Err(Error::Invalid("pyramid DCT precision"));
                }
                let q = quantization_tables(level.quality, level.baseline_quantization)?;
                for (j, c) in frame.components.iter_mut().enumerate() {
                    c.quantization_table = u8::from(
                        j > 0 && j < 3 && matches!(space, ColorSpace::YCbCr | ColorSpace::Ycck),
                    );
                }
                let wide = frame
                    .components
                    .iter()
                    .any(|c| q[c.quantization_table as usize].iter().any(|&v| v > 255));
                frame.process =
                    crate::encode::frame_process_for(header.precision, differential, wide, coding);
                if coding.entropy_coding == EntropyCoding::Arithmetic {
                    coding.arithmetic_conditioning.validate()?;
                }
                crate::scan_encode::Plan::new(
                    &frame.components,
                    coding.progressive,
                    coding.successive_approximation,
                    coding.scan_script,
                    options.limits.max_scans,
                )?
                .len()
            }
            HierarchicalFrameOptions::Lossless(mut coding) => {
                frame.process = match (differential, coding.entropy_coding) {
                    (false, EntropyCoding::Huffman) => Process::Lossless,
                    (true, EntropyCoding::Huffman) => Process::DifferentialLossless,
                    (false, EntropyCoding::Arithmetic) => Process::ArithmeticLossless,
                    (true, EntropyCoding::Arithmetic) => Process::ArithmeticDifferentialLossless,
                };
                coding.limits = options.limits;
                crate::lossless_encode::frame_requirements(&frame, coding)?.0
            }
        };
        rules.accept_process(&frame, level.expansion, frame.process)?;
        scans = scans
            .checked_add(count)
            .ok_or(Error::LimitExceeded("pyramid scans"))?;
        if scans > options.limits.max_scans {
            return Err(Error::LimitExceeded("pyramid scans"));
        }
        result.push(frame);
    }
    rules.finish()?;
    budget.used -= dims.capacity() * core::mem::size_of::<(u16, u16)>();
    drop(dims);
    Ok(result)
}

fn frame_data(
    frame: &FrameHeader,
    level: PyramidLevel<'_>,
    source: Wanted<'_, '_>,
    reference: &[ComponentPlane],
    budget: &mut Budget,
    limits: Limits,
) -> Result<(HierarchicalData, usize)> {
    let differential = frame.process.is_differential();
    match level.coding {
        HierarchicalFrameOptions::Lossless(mut options) => {
            options.limits = limits;
            let (resolved, count) = crate::lossless_encode::frame_coding(frame, options)?;
            let mut coding = budget.vector(frame.components.len())?;
            coding.extend_from_slice(&resolved[..frame.components.len()]);
            let mut components = planes(frame, budget)?;
            for (i, c) in components.iter_mut().enumerate() {
                c.spec = frame.components[i];
                let point = coding[i].point_transform;
                for y in 0..c.height {
                    for x in 0..c.width {
                        let target = source.at(i, c.spec, x, y);
                        c.samples[y * c.width + x] = if differential {
                            let difference =
                                target.wrapping_sub(expanded(&reference[i], x, y, level.expansion))
                                    as i16;
                            ((i32::from(difference) / (1i32 << point)) * (1i32 << point)) as u16
                        } else {
                            (target >> point) << point
                        };
                    }
                }
            }
            Ok((HierarchicalData::Lossless { components, coding }, count))
        }
        HierarchicalFrameOptions::Dct(options) => {
            let plan = crate::scan_encode::Plan::new(
                &frame.components,
                options.progressive,
                options.successive_approximation,
                options.scan_script,
                limits.max_scans,
            )?;
            let progression = if options.progressive {
                Some(plan.progressive_precision()?)
            } else {
                None
            };
            let (max_h, max_v) = sampling(frame);
            let cols = (frame.width as usize).div_ceil(8 * max_h);
            let rows = (frame.height as usize).div_ceil(8 * max_v);
            let tables = quantization_tables(level.quality, level.baseline_quantization)?;
            let mut components = budget.vector(frame.components.len())?;
            for (i, &spec) in frame.components.iter().enumerate() {
                let precision = progression.as_ref().map_or([0; 64], |p| p[i]);
                let (width, height) = dimensions(frame, spec);
                let bw = cols * spec.horizontal_sampling as usize;
                let bh = rows * spec.vertical_sampling as usize;
                let mut blocks = budget.filled(bw * bh, [0i16; 64])?;
                let q = tables[spec.quantization_table as usize];
                for by in 0..bh {
                    for bx in 0..bw {
                        let mut samples = [0; 64];
                        for y in 0..8 {
                            for x in 0..8 {
                                let sx = (bx * 8 + x).min(width - 1);
                                let sy = (by * 8 + y).min(height - 1);
                                let target = source.at(i, spec, sx, sy);
                                samples[y * 8 + x] = if differential {
                                    target.wrapping_sub(expanded(
                                        &reference[i],
                                        sx,
                                        sy,
                                        level.expansion,
                                    ))
                                } else {
                                    target
                                };
                            }
                        }
                        let mut block = if differential {
                            crate::transform::fdct_differential(&samples, &q)?
                        } else {
                            crate::transform::fdct(&samples, &q, frame.precision)
                        };
                        for (k, &index) in ZIGZAG.iter().enumerate() {
                            let al = precision[k];
                            block[index] = if al < 0 {
                                0
                            } else if k == 0 {
                                ((i32::from(block[index]) >> al) << al) as i16
                            } else {
                                let v = i32::from(block[index]);
                                (v.signum() * ((v.abs() >> al) << al)) as i16
                            };
                        }
                        blocks[by * bw + bx] = block;
                    }
                }
                components.push(CoefficientComponent {
                    spec,
                    width,
                    height,
                    blocks_wide: bw,
                    blocks_high: bh,
                    quantization: q,
                    successive_approximation: precision,
                    blocks,
                });
            }
            Ok((HierarchicalData::Dct(components), plan.len()))
        }
    }
}

/// The final unscaled spatial correction consumes the reference buffer in
/// place. Its old samples are needed only once, and no later reference exists.
fn final_correction(
    frame: &FrameHeader,
    level: PyramidLevel<'_>,
    source: Wanted<'_, '_>,
    mut components: Vec<ComponentPlane>,
    budget: &mut Budget,
    limits: Limits,
) -> Result<(HierarchicalData, usize)> {
    let HierarchicalFrameOptions::Lossless(mut options) = level.coding else {
        unreachable!()
    };
    options.limits = limits;
    let (resolved, count) = crate::lossless_encode::frame_coding(frame, options)?;
    let mut coding = budget.vector(frame.components.len())?;
    coding.extend_from_slice(&resolved[..frame.components.len()]);
    for (i, c) in components.iter_mut().enumerate() {
        c.spec = frame.components[i];
        let scale = 1i32 << coding[i].point_transform;
        for y in 0..c.height {
            for x in 0..c.width {
                let at = y * c.width + x;
                let delta = source.at(i, c.spec, x, y).wrapping_sub(c.samples[at]) as i16;
                c.samples[at] = ((i32::from(delta) / scale) * scale) as u16;
            }
        }
    }
    Ok((HierarchicalData::Lossless { components, coding }, count))
}

fn build<'a>(
    header: HierarchyHeader,
    source: Source<'_>,
    metadata: Vec<MetadataSegment<'a>>,
    mut options: PyramidOptions<'_>,
) -> Result<HierarchicalImage<'a>> {
    options.limits = effective(options);
    header.validate(options.limits)?;
    let meta =
        crate::encode::validate_metadata(&metadata, options.limits, MetadataOptions::default())?;
    let mut budget = Budget {
        used: 0,
        limit: options.limits.max_memory_bytes,
    };
    budget.add(header.components.capacity() * core::mem::size_of::<ComponentSpec>())?;
    budget.add(
        meta + (metadata.capacity() - metadata.len()) * core::mem::size_of::<MetadataSegment<'_>>(),
    )?;
    let space = crate::reconstruct::infer_components(&header.components, &metadata);
    let headers = headers(&header, space, options, &mut budget)?;
    let header_records = headers.capacity() * core::mem::size_of::<FrameHeader>();
    let mut last_use = budget.vector::<u8>(headers.len())?;
    for (i, f) in headers.iter().enumerate() {
        last_use.push(u8::from(
            headers
                .get(i + 1)
                .is_none_or(|n| (n.width, n.height) != (f.width, f.height)),
        ));
    }
    let mut targets = budget.vector::<Target>(headers.len())?;
    for i in (0..headers.len() - 1).rev() {
        let f = &headers[i];
        if (f.width, f.height) == (header.width, header.height)
            || targets
                .iter()
                .any(|t| t.width == f.width && t.height == f.height)
        {
            continue;
        }
        let next = &headers[i + 1];
        let input = wanted(source, &targets, next.width, next.height);
        let planes = downsample(input, f, options.levels[i + 1].expansion, &mut budget)?;
        targets.push(Target {
            width: f.width,
            height: f.height,
            planes,
        });
    }
    let mut frames = budget.vector(headers.len())?;
    let mut reference = Vec::<ComponentPlane>::new();
    for (i, (frame, &level)) in headers.into_iter().zip(options.levels).enumerate() {
        let input = wanted(source, &targets, frame.width, frame.height);
        let (data, scans) = if i + 1 == options.levels.len()
            && frame.process.is_lossless()
            && frame.process.is_differential()
            && !level.expansion.horizontal
            && !level.expansion.vertical
        {
            final_correction(
                &frame,
                level,
                input,
                core::mem::take(&mut reference),
                &mut budget,
                options.limits,
            )?
        } else {
            frame_data(
                &frame,
                level,
                input,
                &reference,
                &mut budget,
                options.limits,
            )?
        };
        let frame = HierarchicalFrame {
            frame,
            data,
            expansion: level.expansion,
            offset: 0,
            first_scan: 0,
            scans,
        };
        if i + 1 < options.levels.len() {
            let same_grid =
                !level.expansion.horizontal && !level.expansion.vertical && !reference.is_empty();
            if same_grid {
                for (j, c) in reference.iter_mut().enumerate() {
                    visit_component(&frame, j, |x, y, v| {
                        let at = y * c.width + x;
                        c.samples[at] = c.samples[at].wrapping_add(v);
                    });
                }
            } else {
                let reuse = if last_use[i] != 0 {
                    targets.iter().position(|t| {
                        t.width == frame.frame.width
                            && t.height == frame.frame.height
                            && !t.planes.is_empty()
                    })
                } else {
                    None
                };
                let mut next = if let Some(index) = reuse {
                    core::mem::take(&mut targets[index].planes)
                } else {
                    planes(&frame.frame, &mut budget)?
                };
                for (j, c) in next.iter_mut().enumerate() {
                    visit_component(&frame, j, |x, y, v| {
                        let old = if frame.frame.process.is_differential() {
                            expanded(&reference[j], x, y, level.expansion)
                        } else {
                            0
                        };
                        c.samples[y * c.width + x] = v.wrapping_add(old);
                    });
                }
                budget.release_planes(reference);
                reference = next;
            }
            if last_use[i] != 0
                && let Some(index) = targets.iter().position(|t| {
                    t.width == frame.frame.width
                        && t.height == frame.frame.height
                        && !t.planes.is_empty()
                })
            {
                budget.release_planes(core::mem::take(&mut targets[index].planes));
            }
        }
        frames.push(frame);
    }
    budget.used -= header_records;
    budget.release_planes(reference);
    drop(targets);
    HierarchicalImage::from_frames(header, frames, metadata, options.limits)
}

/// Generate source levels and residuals from borrowed native component planes.
/// P=2..16; DCT stages require 8/12. Source planes are read directly; metadata
/// payloads remain borrowed. Coarse targets use the edge-replicated K.5 filter, rounded once
/// per reduction. Returned frame values include actual scan/Pt precision.
pub fn build_hierarchical_planes<'a>(
    header: HierarchyHeader,
    components: &[ComponentPlane],
    metadata: Vec<MetadataSegment<'a>>,
    options: PyramidOptions<'_>,
) -> Result<HierarchicalImage<'a>> {
    let limits = effective(options);
    header.validate(limits)?;
    if components.len() != header.components.len() {
        return Err(Error::Invalid("pyramid source component count"));
    }
    let h = header
        .components
        .iter()
        .map(|c| c.horizontal_sampling as usize)
        .max()
        .unwrap();
    let v = header
        .components
        .iter()
        .map(|c| c.vertical_sampling as usize)
        .max()
        .unwrap();
    let max = (1u32 << header.precision) - 1;
    for (i, c) in components.iter().enumerate() {
        if c.spec != header.components[i]
            || c.width != (header.width as usize * c.spec.horizontal_sampling as usize).div_ceil(h)
            || c.height != (header.height as usize * c.spec.vertical_sampling as usize).div_ceil(v)
            || c.width.checked_mul(c.height) != Some(c.samples.len())
            || c.samples.iter().any(|&v| u32::from(v) > max)
        {
            return Err(Error::Invalid("pyramid source component samples"));
        }
    }
    build(header, Source::Planes(components), metadata, options)
}

pub fn encode_hierarchical_planes(
    header: HierarchyHeader,
    components: &[ComponentPlane],
    metadata: Vec<MetadataSegment<'_>>,
    options: PyramidOptions<'_>,
) -> Result<Vec<u8>> {
    encode_hierarchical_planes_with_metadata(
        header,
        components,
        metadata,
        options,
        MetadataOptions::default(),
    )
}
pub fn encode_hierarchical_planes_with_metadata(
    header: HierarchyHeader,
    components: &[ComponentPlane],
    metadata: Vec<MetadataSegment<'_>>,
    options: PyramidOptions<'_>,
    output_metadata: MetadataOptions<'_>,
) -> Result<Vec<u8>> {
    let image = build_hierarchical_planes(header, components, metadata, options)?;
    crate::hierarchical_encode::write_hierarchy(
        &image,
        options.limits,
        options.max_output_bytes,
        options.omit_dhp_height,
        output_metadata,
        |i, _| Ok(options.levels[i].coding),
    )
}

/// Prepare a hierarchy from interleaved native samples. Color conversion,
/// precision scaling and component sampling occur only when explicitly selected.
pub fn build_hierarchical_samples(
    input: &EncodeInput<'_>,
    raster: PyramidRasterOptions,
    options: PyramidOptions<'_>,
) -> Result<HierarchicalImage<'static>> {
    let limits = effective(options);
    let precision = if raster.precision == 0 {
        input.precision
    } else {
        raster.precision
    };
    let channels = validate_raster(input, precision, limits)?;
    let space = if input.color_space == ColorSpace::Rgb && raster.rgb_to_ycbcr {
        ColorSpace::YCbCr
    } else if input.color_space == ColorSpace::Cmyk && raster.cmyk_to_ycck {
        ColorSpace::Ycck
    } else {
        input.color_space
    };
    let mut factors = if channels == 1 {
        [(1, 1); 4]
    } else {
        raster.subsampling.factors()
    };
    if space == ColorSpace::Ycck && !matches!(raster.subsampling, Subsampling::Custom(_)) {
        factors[3] = factors[0];
    }
    let ids = match space {
        ColorSpace::Rgb => *b"RGB\0",
        ColorSpace::Cmyk => *b"CMYK",
        _ => [1, 2, 3, 4],
    };
    let mut specs = Vec::new();
    specs
        .try_reserve_exact(channels)
        .map_err(|_| Error::Allocation)?;
    for i in 0..channels {
        specs.push(ComponentSpec {
            id: ids[i],
            horizontal_sampling: factors[i].0,
            vertical_sampling: factors[i].1,
            quantization_table: 0,
        });
    }
    let header = HierarchyHeader {
        precision,
        width: input.width,
        height: input.height,
        components: specs,
    };
    header.validate(limits)?;
    let mut metadata = Vec::new();
    let adobe = match space {
        ColorSpace::Rgb => Some(b"Adobe\0\x64\0\0\0\0\0".as_slice()),
        ColorSpace::YCbCr => Some(b"Adobe\0\x64\0\0\0\0\x01".as_slice()),
        ColorSpace::Ycck => Some(b"Adobe\0\x64\0\0\0\0\x02".as_slice()),
        ColorSpace::Cmyk if raster.cmyk_polarity == CmykPolarity::AdobeInverted => {
            Some(b"Adobe\0\x64\0\0\0\0\0".as_slice())
        }
        _ => None,
    };
    if let Some(data) = adobe {
        metadata
            .try_reserve_exact(1)
            .map_err(|_| Error::Allocation)?;
        metadata.push(MetadataSegment {
            marker: 0xee,
            data,
            offset: 0,
            preceding_scans: 0,
        });
    }
    let polarity = if input.color_space == ColorSpace::Cmyk && space == ColorSpace::Cmyk {
        CmykPolarity::AdobeInverted
    } else {
        raster.cmyk_polarity
    };
    let source = Source::Raster {
        input: *input,
        transform: ColorTransform {
            precision,
            space,
            cmyk_polarity: polarity,
        },
        channels,
        max_h: factors[..channels]
            .iter()
            .map(|f| f.0 as usize)
            .max()
            .unwrap(),
        max_v: factors[..channels]
            .iter()
            .map(|f| f.1 as usize)
            .max()
            .unwrap(),
    };
    build(header, source, metadata, options)
}

pub fn encode_hierarchical_samples(
    input: &EncodeInput<'_>,
    raster: PyramidRasterOptions,
    options: PyramidOptions<'_>,
) -> Result<Vec<u8>> {
    encode_hierarchical_samples_with_metadata(input, raster, options, MetadataOptions::default())
}
pub fn encode_hierarchical_samples_with_metadata(
    input: &EncodeInput<'_>,
    raster: PyramidRasterOptions,
    options: PyramidOptions<'_>,
    metadata: MetadataOptions<'_>,
) -> Result<Vec<u8>> {
    let image = build_hierarchical_samples(input, raster, options)?;
    crate::hierarchical_encode::write_hierarchy(
        &image,
        options.limits,
        options.max_output_bytes,
        options.omit_dhp_height,
        metadata,
        |i, _| Ok(options.levels[i].coding),
    )
}
pub fn encode_hierarchical_image(
    image: &Image,
    raster: PyramidRasterOptions,
    options: PyramidOptions<'_>,
) -> Result<Vec<u8>> {
    encode_hierarchical_image_with_metadata(image, raster, options, MetadataOptions::default())
}
pub fn encode_hierarchical_image_with_metadata(
    image: &Image,
    raster: PyramidRasterOptions,
    options: PyramidOptions<'_>,
    metadata: MetadataOptions<'_>,
) -> Result<Vec<u8>> {
    image
        .validate()
        .map_err(|_| Error::Invalid("pyramid shared image"))?;
    let input = EncodeInput {
        width: u16::try_from(image.width)
            .map_err(|_| Error::Invalid("pyramid width exceeds 65535"))?,
        height: u16::try_from(image.height)
            .map_err(|_| Error::Invalid("pyramid height exceeds 65535"))?,
        precision: image.bit_depth,
        color_space: match image.color_type {
            ColorType::Grayscale => ColorSpace::Grayscale,
            ColorType::Rgb => ColorSpace::Rgb,
            _ => return Err(Error::Unsupported("pyramid shared color type")),
        },
        samples: match &image.samples {
            Samples::U8(v) => crate::SampleData::U8(v),
            Samples::U16(v) => crate::SampleData::U16(v),
        },
    };
    encode_hierarchical_samples_with_metadata(&input, raster, options, metadata)
}
