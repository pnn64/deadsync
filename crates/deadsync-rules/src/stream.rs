#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamSegment {
    start_and_break: u32,
    end: u32,
}

impl StreamSegment {
    const BREAK_FLAG: u32 = 1 << 31;

    pub const fn new(start: u32, end: u32, is_break: bool) -> Self {
        assert!(
            start < Self::BREAK_FLAG,
            "stream segment start exceeds 31 bits"
        );
        Self {
            start_and_break: start | if is_break { Self::BREAK_FLAG } else { 0 },
            end,
        }
    }

    pub const fn start(self) -> u32 {
        self.start_and_break & !Self::BREAK_FLAG
    }

    pub const fn end(self) -> u32 {
        self.end
    }

    pub const fn is_break(self) -> bool {
        self.start_and_break & Self::BREAK_FLAG != 0
    }
}

/// Owned stream data prepared for gameplay counters and `ZMod` progress.
#[derive(Debug, Default, PartialEq)]
pub struct StreamOutputs {
    pub counter_segments: Vec<StreamSegment>,
    pub zmod_segments: Vec<StreamSegment>,
    pub total_stream: f32,
    pub total_break: f32,
}

pub fn measure_densities(data: &[u8], lanes: usize) -> Vec<usize> {
    match lanes {
        8 => measure_densities_impl::<8>(data),
        _ => measure_densities_impl::<4>(data),
    }
}

/// Counts non-empty rows per measure, saturated at the highest stream threshold.
pub fn stream_measure_densities(data: &[u8], lanes: usize) -> Vec<u8> {
    match lanes {
        8 => stream_measure_densities_impl::<8>(data),
        _ => stream_measure_densities_impl::<4>(data),
    }
}

/// Returns the measure's one-based position and total length within a stream run.
///
/// Note data is scanned only until the requested measure's run ends. The query
/// does not materialize either the full density list or the segment list.
pub fn stream_run_progress(
    data: &[u8],
    lanes: usize,
    threshold: usize,
    current_measure: usize,
) -> Option<(usize, usize)> {
    match lanes {
        8 => stream_run_progress_impl::<8>(data, threshold, current_measure),
        _ => stream_run_progress_impl::<4>(data, threshold, current_measure),
    }
}

pub fn stream_sequences_threshold(measures: &[u8], threshold: usize) -> Vec<StreamSegment> {
    let mut segs = Vec::with_capacity(measures.len().min(64));
    for_each_stream_segment(measures, threshold, |segment| segs.push(segment));
    segs
}

/// Builds optional measure-counter and `ZMod` output in one density traversal.
pub fn stream_outputs_full_measures(
    measures: &[u8],
    counter_threshold: Option<usize>,
    constant_bpm: bool,
    include_zmod: bool,
) -> StreamOutputs {
    if !include_zmod {
        return StreamOutputs {
            counter_segments: counter_threshold.map_or_else(Vec::new, |threshold| {
                stream_sequences_threshold(measures, threshold)
            }),
            ..StreamOutputs::default()
        };
    }

    let Some(counter_threshold) = counter_threshold else {
        let (zmod_segments, total_stream, total_break) =
            zmod_stream_totals_full_measures(measures, constant_bpm);
        return StreamOutputs {
            zmod_segments,
            total_stream,
            total_break,
            ..StreamOutputs::default()
        };
    };

    let (zmod_params, counter_capacity) =
        zmod_params_with_count(measures, constant_bpm, counter_threshold);
    build_stream_outputs(measures, counter_threshold, counter_capacity, zmod_params)
}

fn for_each_stream_segment(
    measures: &[u8],
    threshold: usize,
    mut visit: impl FnMut(StreamSegment),
) {
    assert!(
        measures.len() < StreamSegment::BREAK_FLAG as usize,
        "stream chart exceeds 31-bit measure indices"
    );
    let mut runs = StreamRuns::default();
    for (idx, &density) in measures.iter().enumerate() {
        if usize::from(density) >= threshold {
            runs.record(idx, &mut visit);
        }
    }
    runs.finish(measures.len(), visit);
}

#[derive(Default)]
struct StreamRuns {
    start: Option<usize>,
    end: usize,
}

impl StreamRuns {
    fn record(&mut self, idx: usize, mut visit: impl FnMut(StreamSegment)) {
        match self.start {
            None => {
                if idx >= 2 {
                    visit(stream_segment(0, idx, true));
                }
                self.start = Some(idx);
                self.end = idx + 1;
            }
            Some(_) if idx == self.end => self.end += 1,
            Some(start) => {
                visit(stream_segment(start, self.end, false));
                if idx >= self.end + 2 {
                    visit(stream_segment(self.end, idx, true));
                }
                self.start = Some(idx);
                self.end = idx + 1;
            }
        }
    }

    fn finish(self, measure_count: usize, mut visit: impl FnMut(StreamSegment)) {
        let Some(start) = self.start else {
            return;
        };
        visit(stream_segment(start, self.end, false));
        if measure_count >= self.end + 2 {
            visit(stream_segment(self.end, measure_count, true));
        }
    }
}

fn stream_segment(start: usize, end: usize, is_break: bool) -> StreamSegment {
    debug_assert!(start < StreamSegment::BREAK_FLAG as usize);
    debug_assert!(end < StreamSegment::BREAK_FLAG as usize);
    StreamSegment {
        start_and_break: start as u32
            | if is_break {
                StreamSegment::BREAK_FLAG
            } else {
                0
            },
        end: end as u32,
    }
}

struct StreamDensityFold {
    runs: StreamRuns,
    threshold: usize,
    multiplier: f32,
    total_stream: f32,
    total_measures: f32,
    segment_count: usize,
}

#[derive(Clone, Copy)]
struct StreamDensity {
    ratio: f32,
    segment_count: usize,
}

impl StreamDensityFold {
    fn new(threshold: usize, multiplier: f32) -> Self {
        Self {
            runs: StreamRuns::default(),
            threshold,
            multiplier,
            total_stream: 0.0,
            total_measures: 0.0,
            segment_count: 0,
        }
    }

    fn record(&mut self, idx: usize, density: u8) {
        if usize::from(density) < self.threshold {
            return;
        }
        self.record_qualified(idx);
    }

    fn record_qualified(&mut self, idx: usize) {
        let multiplier = self.multiplier;
        let total_stream = &mut self.total_stream;
        let total_measures = &mut self.total_measures;
        let segment_count = &mut self.segment_count;
        self.runs.record(idx, |segment| {
            *segment_count += 1;
            add_density_segment(segment, multiplier, total_stream, total_measures);
        });
    }

    fn finish(self, measure_count: usize) -> StreamDensity {
        let Self {
            runs,
            multiplier,
            mut total_stream,
            mut total_measures,
            mut segment_count,
            ..
        } = self;
        runs.finish(measure_count, |segment| {
            segment_count += 1;
            add_density_segment(segment, multiplier, &mut total_stream, &mut total_measures);
        });
        let ratio = if total_measures <= 0.0 {
            0.0
        } else {
            total_stream / total_measures
        };
        StreamDensity {
            ratio,
            segment_count,
        }
    }
}

#[derive(Default)]
struct StreamCountFold {
    runs: StreamRuns,
    segment_count: usize,
}

impl StreamCountFold {
    fn record(&mut self, idx: usize, density: u8, threshold: usize) {
        if usize::from(density) < threshold {
            return;
        }
        self.record_qualified(idx);
    }

    fn record_qualified(&mut self, idx: usize) {
        let segment_count = &mut self.segment_count;
        self.runs.record(idx, |_| *segment_count += 1);
    }

    fn finish(self, measure_count: usize) -> usize {
        let Self {
            runs,
            mut segment_count,
        } = self;
        runs.finish(measure_count, |_| segment_count += 1);
        segment_count
    }
}

fn add_density_segment(
    segment: StreamSegment,
    multiplier: f32,
    total_stream: &mut f32,
    total_measures: &mut f32,
) {
    let len = ((segment.end().saturating_sub(segment.start())) as f32 * multiplier).floor();
    if len <= 0.0 {
        return;
    }
    if !segment.is_break() {
        *total_stream += len;
    }
    *total_measures += len;
}

#[inline(always)]
fn zmod_stream_density(measures: &[u8], threshold: usize, multiplier: f32) -> StreamDensity {
    let mut fold = StreamDensityFold::new(threshold, multiplier);
    for (idx, &density) in measures.iter().enumerate() {
        fold.record(idx, density);
    }
    fold.finish(measures.len())
}

fn zmod_density_pair(measures: &[u8]) -> ([StreamDensity; 2], usize) {
    let mut folds = [
        StreamDensityFold::new(24, 1.5),
        StreamDensityFold::new(20, 1.25),
    ];
    let mut count_fold = StreamCountFold::default();
    for (idx, &density) in measures.iter().enumerate() {
        if density >= 24 {
            folds[0].record_qualified(idx);
            folds[1].record_qualified(idx);
            count_fold.record_qualified(idx);
        } else if density >= 20 {
            folds[1].record_qualified(idx);
            count_fold.record_qualified(idx);
        } else if density >= 16 {
            count_fold.record_qualified(idx);
        }
    }
    let [first, second] = folds;
    (
        [first.finish(measures.len()), second.finish(measures.len())],
        count_fold.finish(measures.len()),
    )
}

fn zmod_params(measures: &[u8], constant_bpm: bool) -> (usize, f32, usize) {
    let addition = 2usize;
    if !constant_bpm {
        return (14 + addition, 1.0, 0);
    }

    let d32 = zmod_stream_density(measures, 30 + addition, 2.0);
    if d32.ratio >= 0.2 {
        return (30 + addition, 2.0, d32.segment_count);
    }

    let ([d24, d20], count16) = zmod_density_pair(measures);
    if d24.ratio >= 0.2 {
        (22 + addition, 1.5, d24.segment_count)
    } else if d20.ratio >= 0.2 {
        (18 + addition, 1.25, d20.segment_count)
    } else {
        (14 + addition, 1.0, count16)
    }
}

fn zmod_params_with_count(
    measures: &[u8],
    constant_bpm: bool,
    count_threshold: usize,
) -> ((usize, f32, usize), usize) {
    if !constant_bpm {
        return ((16, 1.0, 0), 0);
    }

    let mut d32 = StreamDensityFold::new(32, 2.0);
    let mut count_fold = StreamCountFold::default();
    for (idx, &density) in measures.iter().enumerate() {
        d32.record(idx, density);
        if count_threshold != 32 {
            count_fold.record(idx, density, count_threshold);
        }
    }
    let d32 = d32.finish(measures.len());
    let counter_capacity = if count_threshold == 32 {
        d32.segment_count
    } else {
        count_fold.finish(measures.len())
    };
    if d32.ratio >= 0.2 {
        return ((32, 2.0, d32.segment_count), counter_capacity);
    }

    let ([d24, d20], count16) = zmod_density_pair(measures);
    let params = if d24.ratio >= 0.2 {
        (24, 1.5, d24.segment_count)
    } else if d20.ratio >= 0.2 {
        (20, 1.25, d20.segment_count)
    } else {
        (16, 1.0, count16)
    };
    (params, counter_capacity)
}

#[derive(Default)]
struct StreamTotals {
    total_stream: f32,
    total_break: f32,
    edge_break: f32,
    pending_break: f32,
    saw_stream: bool,
    last_stream: bool,
}

impl StreamTotals {
    fn record(&mut self, segment: StreamSegment) {
        let len = segment.end().saturating_sub(segment.start()) as f32;
        if len <= 0.0 {
            return;
        }
        if segment.is_break() {
            if self.saw_stream {
                self.pending_break = len;
            } else {
                self.edge_break += len;
            }
            self.last_stream = false;
            return;
        }

        if self.pending_break > 0.0 {
            self.total_break += self.pending_break;
            self.pending_break = 0.0;
        } else if self.last_stream {
            self.total_break += 1.0;
        }
        self.total_stream += len;
        self.saw_stream = true;
        self.last_stream = true;
    }

    fn finish(mut self, multiplier: f32) -> (f32, f32) {
        self.edge_break += self.pending_break;
        if self.total_stream + self.total_break < 10.0
            || self.total_stream + self.total_break < self.edge_break
        {
            self.total_break += self.edge_break;
        }
        (
            self.total_stream * multiplier,
            self.total_break * multiplier,
        )
    }
}

fn build_zmod_totals(
    measures: &[u8],
    threshold: usize,
    multiplier: f32,
    segment_capacity: usize,
) -> (Vec<StreamSegment>, f32, f32) {
    let mut segs = Vec::with_capacity(segment_capacity);
    let mut totals = StreamTotals::default();
    for_each_stream_segment(measures, threshold, |segment| {
        totals.record(segment);
        segs.push(segment);
    });
    let (total_stream, total_break) = totals.finish(multiplier);
    (segs, total_stream, total_break)
}

fn build_stream_outputs(
    measures: &[u8],
    counter_threshold: usize,
    counter_capacity: usize,
    (zmod_threshold, multiplier, zmod_capacity): (usize, f32, usize),
) -> StreamOutputs {
    let mut counter_segments = Vec::with_capacity(counter_capacity);
    let mut zmod_segments = Vec::with_capacity(zmod_capacity);
    let mut totals = StreamTotals::default();

    if counter_threshold == zmod_threshold {
        for_each_stream_segment(measures, zmod_threshold, |segment| {
            counter_segments.push(segment);
            zmod_segments.push(segment);
            totals.record(segment);
        });
    } else {
        let mut counter_runs = StreamRuns::default();
        let mut zmod_runs = StreamRuns::default();
        for (idx, &density) in measures.iter().enumerate() {
            if usize::from(density) >= counter_threshold {
                counter_runs.record(idx, |segment| counter_segments.push(segment));
            }
            if usize::from(density) >= zmod_threshold {
                zmod_runs.record(idx, |segment| {
                    totals.record(segment);
                    zmod_segments.push(segment);
                });
            }
        }
        counter_runs.finish(measures.len(), |segment| counter_segments.push(segment));
        zmod_runs.finish(measures.len(), |segment| {
            totals.record(segment);
            zmod_segments.push(segment);
        });
    }

    let (total_stream, total_break) = totals.finish(multiplier);
    StreamOutputs {
        counter_segments,
        zmod_segments,
        total_stream,
        total_break,
    }
}

#[inline(always)]
pub fn zmod_stream_totals_full_measures(
    measures: &[u8],
    constant_bpm: bool,
) -> (Vec<StreamSegment>, f32, f32) {
    let (threshold, multiplier, segment_capacity) = zmod_params(measures, constant_bpm);
    build_zmod_totals(measures, threshold, multiplier, segment_capacity)
}

fn measure_densities_impl<const LANES: usize>(data: &[u8]) -> Vec<usize> {
    const ROWS_PER_MEASURE_HINT: usize = 16;
    let mut densities = Vec::with_capacity(data.len() / ((LANES + 1) * ROWS_PER_MEASURE_HINT) + 1);
    for_each_measure_density::<LANES, 0>(data, |density| {
        densities.push(density);
        true
    });
    densities
}

fn stream_measure_densities_impl<const LANES: usize>(data: &[u8]) -> Vec<u8> {
    const ROWS_PER_MEASURE_HINT: usize = 16;
    let mut densities = Vec::with_capacity(data.len() / ((LANES + 1) * ROWS_PER_MEASURE_HINT) + 1);
    for_each_measure_density::<LANES, 32>(data, |density| {
        densities.push(density as u8);
        true
    });
    densities
}

fn stream_run_progress_impl<const LANES: usize>(
    data: &[u8],
    threshold: usize,
    current_measure: usize,
) -> Option<(usize, usize)> {
    let mut progress = StreamProgress::new(threshold, current_measure);
    for_each_measure_density::<LANES, 0>(data, |density| progress.record(density));
    progress.finish()
}

struct StreamProgress {
    threshold: usize,
    current: usize,
    index: usize,
    run_start: Option<usize>,
    result: Option<(usize, usize)>,
}

impl StreamProgress {
    const fn new(threshold: usize, current: usize) -> Self {
        Self {
            threshold,
            current,
            index: 0,
            run_start: None,
            result: None,
        }
    }

    fn record(&mut self, density: usize) -> bool {
        let index = self.index;
        self.index += 1;
        if density >= self.threshold {
            self.run_start.get_or_insert(index);
            return true;
        }

        let run_start = self.run_start.take();
        if index < self.current {
            return true;
        }
        if index == self.current {
            return false;
        }
        let Some(start) = run_start.filter(|start| self.current >= *start) else {
            return false;
        };
        self.result = Some((self.current - start + 1, index - start));
        false
    }

    fn finish(mut self) -> Option<(usize, usize)> {
        if self.result.is_none()
            && self.current < self.index
            && let Some(start) = self.run_start.take().filter(|start| self.current >= *start)
        {
            self.result = Some((self.current - start + 1, self.index - start));
        }
        self.result
    }
}

fn for_each_measure_density<const LANES: usize, const CAP: usize>(
    data: &[u8],
    mut visit: impl FnMut(usize) -> bool,
) {
    // Empty-subdivision reduction cannot remove a step: a nonzero off-grid row
    // prevents that reduction level. The reduced density is this direct count.
    let mut measure_steps = 0usize;
    let mut done = false;

    for raw in data.split(|&byte| byte == b'\n') {
        let line = skip_ws(trim_cr(raw));
        if line.is_empty() || line[0] == b'/' {
            continue;
        }

        match line[0] {
            b',' => {
                if !visit(std::mem::take(&mut measure_steps)) {
                    return;
                }
            }
            b';' => {
                visit(std::mem::take(&mut measure_steps));
                done = true;
                break;
            }
            _ if line.len() >= LANES && (CAP == 0 || measure_steps < CAP) => {
                measure_steps += usize::from(density_row_has_step::<LANES>(line));
            }
            _ => {}
        }
    }

    if !done {
        visit(measure_steps);
    }
}

#[cfg(feature = "bench-support")]
pub mod bench_support {
    use super::{
        StreamCountFold, StreamDensityFold, StreamOutputs, StreamSegment, build_stream_outputs,
        build_zmod_totals, for_each_stream_segment, skip_ws, trim_cr, zmod_density_pair,
        zmod_params, zmod_params_with_count,
    };

    pub fn measure_densities_overreserved(data: &[u8], lanes: usize) -> Vec<usize> {
        match lanes {
            8 => measure_densities_overreserved_impl::<8>(data),
            _ => measure_densities_overreserved_impl::<4>(data),
        }
    }

    fn measure_densities_overreserved_impl<const LANES: usize>(data: &[u8]) -> Vec<usize> {
        let mut densities = Vec::with_capacity(data.len() / ((LANES + 1) * 4) + 1);
        let mut measure_steps = 0usize;
        let mut done = false;
        for raw in data.split(|&byte| byte == b'\n') {
            let line = skip_ws(trim_cr(raw));
            if line.is_empty() || line[0] == b'/' {
                continue;
            }
            match line[0] {
                b',' => densities.push(std::mem::take(&mut measure_steps)),
                b';' => {
                    densities.push(std::mem::take(&mut measure_steps));
                    done = true;
                    break;
                }
                _ if line.len() >= LANES => {
                    measure_steps += usize::from(
                        line[..LANES]
                            .iter()
                            .any(|byte| matches!(byte, b'1' | b'2' | b'4')),
                    );
                }
                _ => {}
            }
        }
        if !done {
            densities.push(measure_steps);
        }
        densities
    }

    pub fn zmod_fused_growth(
        measures: &[u8],
        constant_bpm: bool,
    ) -> (Vec<StreamSegment>, f32, f32) {
        let (threshold, multiplier, _) = zmod_params(measures, constant_bpm);
        build_zmod_totals(measures, threshold, multiplier, 0)
    }

    pub fn stream_outputs_counter_growth(
        measures: &[u8],
        counter_threshold: usize,
        constant_bpm: bool,
    ) -> StreamOutputs {
        let (params, _) = zmod_params_with_count(measures, constant_bpm, counter_threshold);
        build_stream_outputs(measures, counter_threshold, 0, params)
    }

    pub fn stream_outputs_separate(
        measures: &[u8],
        counter_threshold: usize,
        constant_bpm: bool,
    ) -> StreamOutputs {
        let (params, counter_capacity) =
            zmod_params_with_count(measures, constant_bpm, counter_threshold);
        let counter_segments = {
            let mut segments = Vec::with_capacity(counter_capacity);
            for_each_stream_segment(measures, counter_threshold, |segment| {
                segments.push(segment);
            });
            segments
        };
        let (zmod_segments, total_stream, total_break) =
            build_zmod_totals(measures, params.0, params.1, params.2);
        StreamOutputs {
            counter_segments,
            zmod_segments,
            total_stream,
            total_break,
        }
    }

    pub fn zmod_fallback_probe(measures: &[u8]) -> ([u32; 2], usize) {
        let ([d24, d20], count16) = zmod_density_pair(measures);
        ([d24.ratio.to_bits(), d20.ratio.to_bits()], count16)
    }

    pub fn zmod_fallback_probe_independent(measures: &[u8]) -> ([u32; 2], usize) {
        let mut d24 = StreamDensityFold::new(24, 1.5);
        let mut d20 = StreamDensityFold::new(20, 1.25);
        let mut count16 = StreamCountFold::default();
        for (idx, &density) in measures.iter().enumerate() {
            d24.record(idx, density);
            d20.record(idx, density);
            count16.record(idx, density, 16);
        }
        (
            [
                d24.finish(measures.len()).ratio.to_bits(),
                d20.finish(measures.len()).ratio.to_bits(),
            ],
            count16.finish(measures.len()),
        )
    }
}

fn density_row_has_step<const LANES: usize>(line: &[u8]) -> bool {
    const IS_STEP: [bool; 256] = {
        let mut table = [false; 256];
        table[b'1' as usize] = true;
        table[b'2' as usize] = true;
        table[b'4' as usize] = true;
        table
    };
    line[..LANES].iter().any(|&byte| IS_STEP[byte as usize])
}

fn trim_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

const fn skip_ws(mut line: &[u8]) -> &[u8] {
    while let [byte, rest @ ..] = line {
        if !byte.is_ascii_whitespace() {
            break;
        }
        line = rest;
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg_tuple(seg: &StreamSegment) -> (usize, usize, bool) {
        (seg.start() as usize, seg.end() as usize, seg.is_break())
    }

    fn stream_sequences_reference(measures: &[u8], threshold: usize) -> Vec<StreamSegment> {
        let streams: Vec<_> = measures
            .iter()
            .enumerate()
            .filter(|(_, density)| usize::from(**density) >= threshold)
            .map(|(idx, _)| idx + 1)
            .collect();
        if streams.is_empty() {
            return Vec::new();
        }

        let mut segments = Vec::new();
        let first_break = streams[0].saturating_sub(1);
        if first_break >= 2 {
            segments.push(stream_segment(0, first_break, true));
        }
        let (mut count, mut end) = (1usize, None);
        for (idx, &current) in streams.iter().enumerate() {
            let next = streams.get(idx + 1).copied().unwrap_or(usize::MAX);
            if current + 1 == next {
                count += 1;
                end = Some(current + 1);
                continue;
            }
            let stream_end = end.unwrap_or(current);
            segments.push(stream_segment(stream_end - count, stream_end, false));
            let break_end = if next == usize::MAX {
                measures.len()
            } else {
                next - 1
            };
            if break_end >= current + 2 {
                segments.push(stream_segment(current, break_end, true));
            }
            count = 1;
            end = None;
        }
        segments
    }

    fn materialized_stream_density(measures: &[u8], threshold: usize, multiplier: f32) -> f32 {
        let segments = stream_sequences_reference(measures, threshold);
        let mut total_stream = 0.0_f32;
        let mut total_measures = 0.0_f32;
        for segment in segments {
            let len = ((segment.end().saturating_sub(segment.start())) as f32 * multiplier).floor();
            if len <= 0.0 {
                continue;
            }
            if !segment.is_break() {
                total_stream += len;
            }
            total_measures += len;
        }
        if total_measures <= 0.0 {
            0.0
        } else {
            total_stream / total_measures
        }
    }

    fn zmod_totals_reference(
        measures: &[u8],
        constant_bpm: bool,
    ) -> (Vec<StreamSegment>, f32, f32) {
        let (mut threshold, mut multiplier) = (16, 1.0_f32);
        if constant_bpm {
            (threshold, multiplier) = (32, 2.0);
            if materialized_stream_density(measures, threshold, multiplier) < 0.2 {
                (threshold, multiplier) = (24, 1.5);
                if materialized_stream_density(measures, threshold, multiplier) < 0.2 {
                    (threshold, multiplier) = (20, 1.25);
                    if materialized_stream_density(measures, threshold, multiplier) < 0.2 {
                        (threshold, multiplier) = (16, 1.0);
                    }
                }
            }
        }

        let segments = stream_sequences_reference(measures, threshold);
        let mut total_stream = 0.0_f32;
        let mut total_break = 0.0_f32;
        let mut edge_break = 0.0_f32;
        let mut last_stream = false;
        for (index, segment) in segments.iter().enumerate() {
            let len = segment.end().saturating_sub(segment.start()) as f32;
            if segment.is_break() && index > 0 && index + 1 < segments.len() {
                total_break += len;
                last_stream = false;
            } else if segment.is_break() {
                edge_break += len;
                last_stream = false;
            } else {
                if last_stream {
                    total_break += 1.0;
                }
                total_stream += len;
                last_stream = true;
            }
        }
        if total_stream + total_break < 10.0 || total_stream + total_break < edge_break {
            total_break += edge_break;
        }
        (
            segments,
            total_stream * multiplier,
            total_break * multiplier,
        )
    }

    fn note_data_from_densities<const LANES: usize>(densities: &[usize]) -> Vec<u8> {
        let mut data = Vec::new();
        for (measure, &density) in densities.iter().enumerate() {
            for row in 0..32 {
                let mut cells = [b'0'; LANES];
                if row < density {
                    cells[row % LANES] = b'1';
                }
                data.extend_from_slice(&cells);
                data.push(b'\n');
            }
            data.extend_from_slice(if measure + 1 == densities.len() {
                b";\n"
            } else {
                b",\n"
            });
        }
        data
    }

    #[test]
    fn stream_sequences_build_streams_and_breaks() {
        let segs = stream_sequences_threshold(&[0, 0, 16, 17, 0, 0, 18], 16);
        let tuples: Vec<_> = segs.iter().map(seg_tuple).collect();

        assert_eq!(
            tuples,
            vec![(0, 2, true), (2, 4, false), (4, 6, true), (6, 7, false)]
        );
    }

    #[test]
    fn stream_sequences_returns_empty_without_stream_measures() {
        assert!(stream_sequences_threshold(&[0, 1, 2, 3], 16).is_empty());
    }

    #[test]
    fn direct_stream_segments_match_qualifying_index_reference() {
        for len in 0..=12 {
            for mask in 0usize..(1usize << len) {
                let measures: Vec<_> = (0..len)
                    .map(|idx| u8::from(mask & (1 << idx) != 0) * 16)
                    .collect();
                assert_eq!(
                    stream_sequences_threshold(&measures, 16),
                    stream_sequences_reference(&measures, 16),
                    "length {len}, mask {mask:#x}"
                );
            }
        }
    }

    #[test]
    fn measure_densities_count_non_empty_note_rows() {
        let data = b"1000\n0100\n0000\n0010\n,\n0000\n0000\n0001\n0000\n;";

        assert_eq!(measure_densities(data, 4), vec![3, 1]);
    }

    #[test]
    fn measure_densities_reduce_empty_subdivisions() {
        let data = b"1000\n0000\n0100\n0000\n,\n0000\n0000\n0000\n0000\n;";

        assert_eq!(measure_densities(data, 4), vec![2, 0]);
    }

    #[test]
    fn measure_densities_support_eight_lanes() {
        let data = b"10000000\n00001000\n,\n00000000\n;";

        assert_eq!(measure_densities(data, 8), vec![2, 0]);
    }

    #[test]
    fn measure_densities_counts_steps_without_row_scratch() {
        let data = b"  // ignored\r\n  1000\r\n0000\nM000\n2000\n3000\n4000\n,\n,\n0100\n";

        assert_eq!(measure_densities(data, 4), vec![3, 0, 1]);
    }

    #[test]
    fn compact_density_saturates_without_losing_stream_classification() {
        let mut data = b"1000\n".repeat(300);
        data.extend_from_slice(b";\n");

        let exact = measure_densities(&data, 4);
        let densities = stream_measure_densities(&data, 4);

        assert_eq!(exact, [300]);
        assert_eq!(densities, [32]);
        assert_eq!(stream_sequences_threshold(&densities, 32).len(), 1);
    }

    #[test]
    fn stream_segments_pack_break_state_into_start_index() {
        let stream = StreamSegment::new(7, 12, false);
        let gap = StreamSegment::new(12, 16, true);

        assert_eq!(std::mem::size_of::<StreamSegment>(), 8);
        assert_eq!(
            (stream.start(), stream.end(), stream.is_break()),
            (7, 12, false)
        );
        assert_eq!((gap.start(), gap.end(), gap.is_break()), (12, 16, true));
    }

    #[test]
    fn stream_run_progress_matches_materialized_segments() {
        let densities = [0, 16, 17, 32, 0, 20, 0, 16, 16];
        let data = note_data_from_densities::<4>(&densities);
        let expected = [
            None,
            Some((1, 3)),
            Some((2, 3)),
            Some((3, 3)),
            None,
            Some((1, 1)),
            None,
            Some((1, 2)),
            Some((2, 2)),
            None,
        ];
        for (current, expected) in expected.into_iter().enumerate() {
            assert_eq!(stream_run_progress(&data, 4, 16, current), expected);
        }

        let eight_lane_data = note_data_from_densities::<8>(&densities);
        assert_eq!(
            stream_run_progress(&eight_lane_data, 8, 16, 2),
            Some((2, 3))
        );
    }

    #[test]
    fn streamed_density_matches_materialized_segments() {
        let measures: Vec<_> = (0..257)
            .map(|idx| ((idx * 37 + idx / 7) % 40) as u8)
            .collect();
        for threshold in [1, 8, 16, 20, 24, 32, 40] {
            for multiplier in [1.0, 1.25, 1.5, 2.0] {
                assert_eq!(
                    zmod_stream_density(&measures, threshold, multiplier)
                        .ratio
                        .to_bits(),
                    materialized_stream_density(&measures, threshold, multiplier).to_bits(),
                    "threshold {threshold}, multiplier {multiplier}"
                );
            }
        }
        let (pair, count16) = zmod_density_pair(&measures);
        assert_eq!(
            pair.map(|density| density.ratio.to_bits()),
            [
                materialized_stream_density(&measures, 24, 1.5).to_bits(),
                materialized_stream_density(&measures, 20, 1.25).to_bits(),
            ]
        );
        assert_eq!(count16, stream_sequences_reference(&measures, 16).len());
    }

    #[test]
    fn fused_zmod_totals_match_materialized_reference() {
        for len in 0..=12 {
            for mask in 0usize..(1usize << len) {
                let measures: Vec<_> = (0..len)
                    .map(|idx| u8::from(mask & (1 << idx) != 0) * 32)
                    .collect();
                for constant_bpm in [false, true] {
                    let expected = zmod_totals_reference(&measures, constant_bpm);
                    let actual = zmod_stream_totals_full_measures(&measures, constant_bpm);
                    assert_eq!(actual.0, expected.0, "len {len}, mask {mask:#x}");
                    assert_eq!(actual.1.to_bits(), expected.1.to_bits());
                    assert_eq!(actual.2.to_bits(), expected.2.to_bits());
                }
            }
        }

        let measures: Vec<_> = (0..1_031)
            .map(|idx| ((idx * 37 + idx / 7 + idx / 31) % 40) as u8)
            .collect();
        let expected = zmod_totals_reference(&measures, true);
        let actual = zmod_stream_totals_full_measures(&measures, true);
        assert_eq!(actual.0, expected.0);
        assert_eq!(actual.1.to_bits(), expected.1.to_bits());
        assert_eq!(actual.2.to_bits(), expected.2.to_bits());
    }

    #[test]
    fn combined_stream_outputs_match_independent_builders() {
        for len in 0..=10 {
            for mask in 0usize..(1usize << len) {
                let measures: Vec<_> = (0..len)
                    .map(|idx| {
                        if mask & (1 << idx) == 0 {
                            (idx % 12) as u8
                        } else {
                            (16 + (idx * 7) % 24) as u8
                        }
                    })
                    .collect();
                for counter_threshold in [12, 16, 24, 32] {
                    for constant_bpm in [false, true] {
                        let expected_counter =
                            stream_sequences_reference(&measures, counter_threshold);
                        let expected_zmod = zmod_totals_reference(&measures, constant_bpm);
                        let actual = stream_outputs_full_measures(
                            &measures,
                            Some(counter_threshold),
                            constant_bpm,
                            true,
                        );
                        assert_eq!(actual.counter_segments, expected_counter);
                        assert_eq!(actual.zmod_segments, expected_zmod.0);
                        assert_eq!(actual.total_stream.to_bits(), expected_zmod.1.to_bits());
                        assert_eq!(actual.total_break.to_bits(), expected_zmod.2.to_bits());
                    }
                }
            }
        }
    }

    #[test]
    fn zmod_stream_totals_include_edge_break_for_short_charts() {
        let (_segs, total_stream, total_break) =
            zmod_stream_totals_full_measures(&[0, 0, 16, 17, 0, 0], false);

        assert_eq!(total_stream, 2.0);
        assert_eq!(total_break, 4.0);
    }

    #[test]
    fn zmod_constant_bpm_uses_high_density_multiplier() {
        let measures = [32u8; 8];
        let (_segs, total_stream, total_break) = zmod_stream_totals_full_measures(&measures, true);

        assert_eq!(total_stream, 16.0);
        assert_eq!(total_break, 0.0);
    }
}
