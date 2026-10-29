//! Direct hierarchical frame writing, T.81 Annex J.
use crate::encode::{DctFrame, emit_additional, emit_metadata, validate_metadata};
use crate::hierarchical::Rules;
use crate::lossless_encode::{Source, intersect};
use crate::output::{Output, Sink};
use crate::{
    CodingOptions, EntropyCoding, Error, HierarchicalData, HierarchicalFrame, HierarchicalImage,
    Limits, LosslessOptions, MetadataOptions, Process, Result,
};

/// Encoding controls for a single hierarchy frame. The variant must match its
/// native data family. Differential predictive scans always use PSV=0; explicit
/// lossless scripts must select zero themselves. Residual Pt divides signed
/// integers toward zero. All sample values/coefficients are preserved at Pt=0
/// with full DCT scans, independently of the source's original entropy tables.
#[derive(Clone, Copy, Debug)]
pub enum HierarchicalFrameOptions<'a> {
    Dct(CodingOptions<'a>),
    Lossless(LosslessOptions<'a>),
}

#[derive(Clone, Copy, Debug)]
pub struct HierarchicalOptions<'a> {
    /// Empty preserves each frame's entropy family and sequential/progressive
    /// choice, using full-precision generated scans and Pt=0. Otherwise supply
    /// exactly one matching descriptor per frame. Cross-frame process rules
    /// are checked after resolving the requested output processes.
    pub frame_options: &'a [HierarchicalFrameOptions<'a>],
    pub limits: Limits,
    pub max_output_bytes: usize,
    /// Omit the completed height in DHP. Each frame still declares its height
    /// through SOF or DNL; the decoder infers the final height from components.
    pub omit_dhp_height: bool,
}
impl Default for HierarchicalOptions<'_> {
    fn default() -> Self {
        Self {
            frame_options: &[],
            limits: Limits::default(),
            max_output_bytes: 512 * 1024 * 1024,
            omit_dhp_height: false,
        }
    }
}

fn selected<'a>(
    options: HierarchicalOptions<'a>,
    index: usize,
    frame: &HierarchicalFrame,
) -> Result<HierarchicalFrameOptions<'a>> {
    let result = if options.frame_options.is_empty() {
        let entropy_coding = if frame.frame.process.is_arithmetic() {
            EntropyCoding::Arithmetic
        } else {
            EntropyCoding::Huffman
        };
        if frame.frame.process.is_lossless() {
            HierarchicalFrameOptions::Lossless(LosslessOptions {
                entropy_coding,
                ..LosslessOptions::default()
            })
        } else {
            HierarchicalFrameOptions::Dct(CodingOptions {
                entropy_coding,
                progressive: frame.frame.process.is_progressive(),
                ..CodingOptions::default()
            })
        }
    } else {
        options.frame_options[index]
    };
    if matches!(result, HierarchicalFrameOptions::Lossless(_)) != frame.frame.process.is_lossless()
    {
        return Err(Error::Invalid("hierarchical frame encoding data family"));
    }
    Ok(result)
}
fn predictive_process(frame: &HierarchicalFrame, options: LosslessOptions<'_>) -> Process {
    match (
        frame.frame.process.is_differential(),
        options.entropy_coding,
    ) {
        (false, EntropyCoding::Huffman) => Process::Lossless,
        (false, EntropyCoding::Arithmetic) => Process::ArithmeticLossless,
        (true, EntropyCoding::Huffman) => Process::DifferentialLossless,
        (true, EntropyCoding::Arithmetic) => Process::ArithmeticDifferentialLossless,
    }
}

/// Encode native hierarchy frames into one bounded output buffer. Original
/// frame dimensions, sampling, coefficients, quantizers and sample differences
/// are read directly. No frame is cloned, separately compressed or concatenated.
pub fn encode_hierarchical(
    image: &HierarchicalImage<'_>,
    options: HierarchicalOptions<'_>,
) -> Result<Vec<u8>> {
    encode_hierarchical_with_metadata(image, options, MetadataOptions::default())
}

/// Original APPn/COM bytes keep their order and frame-relative scan positions
/// where possible; positions beyond a frame's new last scan move to its end.
/// Additional segments and replacement ICC chunks precede DHP.
pub fn encode_hierarchical_with_metadata(
    image: &HierarchicalImage<'_>,
    options: HierarchicalOptions<'_>,
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
    image: &HierarchicalImage<'_>,
    options: HierarchicalOptions<'_>,
    metadata: MetadataOptions<'_>,
    tables: &mut crate::abbreviated::TableWriter<'_>,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    write_with_sink(image, options, metadata, tables, &mut bytes)?;
    Ok(bytes)
}
pub(crate) fn write_with_sink<S: Sink>(
    image: &HierarchicalImage<'_>,
    options: HierarchicalOptions<'_>,
    metadata: MetadataOptions<'_>,
    tables: &mut crate::abbreviated::TableWriter<'_>,
    sink: &mut S,
) -> Result<()> {
    if !options.frame_options.is_empty() && options.frame_options.len() != image.frames.len() {
        return Err(Error::Invalid("hierarchical frame option count"));
    }
    write_hierarchy_to_sink(
        image,
        options.limits,
        options.max_output_bytes,
        options.omit_dhp_height,
        metadata,
        |index, frame| selected(options, index, frame),
        tables,
        sink,
    )
}

/// Resolve prepared frame controls lazily so raster pyramid emission need not
/// copy an array of descriptors or encode/concatenate individual JPEG frames.
pub(crate) fn write_hierarchy<'a>(
    image: &HierarchicalImage<'_>,
    limits: Limits,
    max_output_bytes: usize,
    omit_dhp_height: bool,
    metadata: MetadataOptions<'_>,
    controls: impl FnMut(usize, &HierarchicalFrame) -> Result<HierarchicalFrameOptions<'a>>,
) -> Result<Vec<u8>> {
    write_hierarchy_with_tables(
        image,
        limits,
        max_output_bytes,
        omit_dhp_height,
        metadata,
        controls,
        &mut crate::abbreviated::TableWriter::interchange(),
    )
}
pub(crate) fn write_hierarchy_with_tables<'a>(
    image: &HierarchicalImage<'_>,
    limits: Limits,
    max_output_bytes: usize,
    omit_dhp_height: bool,
    metadata: MetadataOptions<'_>,
    controls: impl FnMut(usize, &HierarchicalFrame) -> Result<HierarchicalFrameOptions<'a>>,
    tables: &mut crate::abbreviated::TableWriter<'_>,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    write_hierarchy_to_sink(
        image,
        limits,
        max_output_bytes,
        omit_dhp_height,
        metadata,
        controls,
        tables,
        &mut bytes,
    )?;
    Ok(bytes)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_hierarchy_to_sink<'a, S: Sink>(
    image: &HierarchicalImage<'_>,
    limits: Limits,
    max_output_bytes: usize,
    omit_dhp_height: bool,
    metadata: MetadataOptions<'_>,
    mut controls: impl FnMut(usize, &HierarchicalFrame) -> Result<HierarchicalFrameOptions<'a>>,
    tables: &mut crate::abbreviated::TableWriter<'_>,
    sink: &mut S,
) -> Result<()> {
    let stored = image.validate()?.max(image.memory_bytes);
    let mut limits = intersect(image.limits, limits);
    for (index, frame) in image.frames.iter().enumerate() {
        if let HierarchicalFrameOptions::Lossless(option) = controls(index, frame)? {
            limits = intersect(limits, option.limits);
        }
    }
    image.header.validate(limits)?;
    validate_metadata(&image.metadata, limits, metadata)?;
    let additional = validate_metadata(&[], limits, metadata)?;
    let mut rules = Rules::new(&image.header);
    let mut scans = 0usize;
    let mut scratch = 0usize;
    for (index, frame) in image.frames.iter().enumerate() {
        let (process, count, memory) = match (controls(index, frame)?, &frame.data) {
            (HierarchicalFrameOptions::Dct(option), HierarchicalData::Dct(components)) => {
                let source = DctFrame {
                    frame: &frame.frame,
                    components,
                };
                crate::encode::validate_frame(source, limits)?;
                let plan = crate::scan_encode::Plan::new(
                    &frame.frame.components,
                    option.progressive,
                    option.successive_approximation,
                    option.scan_script,
                    limits.max_scans,
                )?;
                (
                    crate::encode::frame_process(source, option),
                    plan.len(),
                    if option.entropy_coding == EntropyCoding::Huffman {
                        1024
                    } else {
                        0
                    },
                )
            }
            (HierarchicalFrameOptions::Lossless(mut option), HierarchicalData::Lossless { .. }) => {
                option.limits = limits;
                let (count, history) =
                    crate::lossless_encode::frame_requirements(&frame.frame, option)?;
                (
                    predictive_process(frame, option),
                    count,
                    history
                        + if option.entropy_coding == EntropyCoding::Huffman {
                            512
                        } else {
                            0
                        },
                )
            }
            _ => unreachable!(),
        };
        rules.accept_process(&frame.frame, frame.expansion, process)?;
        scans = scans
            .checked_add(count)
            .ok_or(Error::LimitExceeded("hierarchical encoder scans"))?;
        scratch = scratch.max(memory);
    }
    rules.finish()?;
    if scans > limits.max_scans {
        return Err(Error::LimitExceeded("hierarchical encoder scans"));
    }
    let available = limits
        .max_memory_bytes
        .checked_sub(stored)
        .and_then(|n| n.checked_sub(additional))
        .and_then(|n| n.checked_sub(scratch))
        .ok_or(Error::LimitExceeded("hierarchical encoder storage"))?;
    let mut out = Output::with_sink(sink, max_output_bytes, available)?;
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
    let h = &image.header;
    let mut header = [0; 771];
    header[0] = h.precision;
    header[1..3].copy_from_slice(&if omit_dhp_height { 0 } else { h.height }.to_be_bytes());
    header[3..5].copy_from_slice(&h.width.to_be_bytes());
    header[5] = h.components.len() as u8;
    for (i, c) in h.components.iter().enumerate() {
        header[6 + 3 * i] = c.id;
        header[7 + 3 * i] = (c.horizontal_sampling << 4) | c.vertical_sampling;
    }
    out.segment(0xde, &header[..6 + 3 * h.components.len()])?;
    for (index, frame) in image.frames.iter().enumerate() {
        emit_metadata(
            &mut out,
            &image.metadata,
            &mut metadata_index,
            frame.first_scan,
            metadata.icc,
        )?;
        if frame.expansion.horizontal || frame.expansion.vertical {
            out.segment(
                0xdf,
                &[
                    (u8::from(frame.expansion.horizontal) << 4)
                        | u8::from(frame.expansion.vertical),
                ],
            )?;
        }
        let mut before_scan = |out: &mut Output<_>, scan: usize| {
            let source_scan = frame.first_scan + scan.min(frame.scans - 1);
            emit_metadata(
                out,
                &image.metadata,
                &mut metadata_index,
                source_scan,
                metadata.icc,
            )
        };
        match (controls(index, frame)?, &frame.data) {
            (HierarchicalFrameOptions::Dct(option), HierarchicalData::Dct(components)) => {
                out.bounded(option.max_output_bytes, |out| {
                    crate::encode::encode_frame(
                        out,
                        DctFrame {
                            frame: &frame.frame,
                            components,
                        },
                        option,
                        limits,
                        true,
                        &mut before_scan,
                        tables,
                    )
                })?;
            }
            (
                HierarchicalFrameOptions::Lossless(mut option),
                HierarchicalData::Lossless { components, .. },
            ) => {
                option.limits = limits;
                out.bounded(option.max_output_bytes, |out| {
                    crate::lossless_encode::encode_frame(
                        out,
                        &frame.frame,
                        Source::Planes(components),
                        option,
                        true,
                        &mut before_scan,
                        tables,
                    )
                })?;
            }
            _ => unreachable!(),
        }
        emit_metadata(
            &mut out,
            &image.metadata,
            &mut metadata_index,
            frame.first_scan + frame.scans,
            metadata.icc,
        )?;
    }
    out.marker(0xd9)?;
    Ok(())
}
