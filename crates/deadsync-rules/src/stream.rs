#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamSegment {
    pub start: usize,
    pub end: usize,
    pub is_break: bool,
}

pub fn measure_densities(data: &[u8], lanes: usize) -> Vec<usize> {
    match lanes {
        8 => measure_densities_impl::<8>(data),
        _ => measure_densities_impl::<4>(data),
    }
}

pub fn stream_sequences_threshold(measures: &[usize], threshold: usize) -> Vec<StreamSegment> {
    let mut segs = Vec::new();
    for_each_stream_segment(measures, threshold, |segment| segs.push(segment));
    segs
}

fn for_each_stream_segment(
    measures: &[usize],
    threshold: usize,
    mut visit: impl FnMut(StreamSegment),
) {
    let mut runs = StreamRuns::default();
    for (idx, &density) in measures.iter().enumerate() {
        if density >= threshold {
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
                    visit(StreamSegment {
                        start: 0,
                        end: idx,
                        is_break: true,
                    });
                }
                self.start = Some(idx);
                self.end = idx + 1;
            }
            Some(_) if idx == self.end => self.end += 1,
            Some(start) => {
                visit(StreamSegment {
                    start,
                    end: self.end,
                    is_break: false,
                });
                if idx >= self.end + 2 {
                    visit(StreamSegment {
                        start: self.end,
                        end: idx,
                        is_break: true,
                    });
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
        visit(StreamSegment {
            start,
            end: self.end,
            is_break: false,
        });
        if measure_count >= self.end + 2 {
            visit(StreamSegment {
                start: self.end,
                end: measure_count,
                is_break: true,
            });
        }
    }
}

struct StreamDensityFold {
    runs: StreamRuns,
    threshold: usize,
    multiplier: f32,
    total_stream: f32,
    total_measures: f32,
}

impl StreamDensityFold {
    fn new(threshold: usize, multiplier: f32) -> Self {
        Self {
            runs: StreamRuns::default(),
            threshold,
            multiplier,
            total_stream: 0.0,
            total_measures: 0.0,
        }
    }

    fn record(&mut self, idx: usize, density: usize) {
        if density < self.threshold {
            return;
        }
        let multiplier = self.multiplier;
        let total_stream = &mut self.total_stream;
        let total_measures = &mut self.total_measures;
        self.runs.record(idx, |segment| {
            add_density_segment(segment, multiplier, total_stream, total_measures);
        });
    }

    fn finish(self, measure_count: usize) -> f32 {
        let Self {
            runs,
            multiplier,
            mut total_stream,
            mut total_measures,
            ..
        } = self;
        runs.finish(measure_count, |segment| {
            add_density_segment(segment, multiplier, &mut total_stream, &mut total_measures);
        });
        if total_measures <= 0.0 {
            0.0
        } else {
            total_stream / total_measures
        }
    }
}

fn add_density_segment(
    segment: StreamSegment,
    multiplier: f32,
    total_stream: &mut f32,
    total_measures: &mut f32,
) {
    let len = ((segment.end.saturating_sub(segment.start)) as f32 * multiplier).floor();
    if len <= 0.0 {
        return;
    }
    if !segment.is_break {
        *total_stream += len;
    }
    *total_measures += len;
}

#[inline(always)]
fn zmod_stream_density(measures: &[usize], threshold: usize, multiplier: f32) -> f32 {
    let mut fold = StreamDensityFold::new(threshold, multiplier);
    for (idx, &density) in measures.iter().enumerate() {
        fold.record(idx, density);
    }
    fold.finish(measures.len())
}

fn zmod_stream_density_pair(measures: &[usize], configs: [(usize, f32); 2]) -> [f32; 2] {
    let mut folds =
        configs.map(|(threshold, multiplier)| StreamDensityFold::new(threshold, multiplier));
    for (idx, &density) in measures.iter().enumerate() {
        folds[0].record(idx, density);
        folds[1].record(idx, density);
    }
    let [first, second] = folds;
    [first.finish(measures.len()), second.finish(measures.len())]
}

#[inline(always)]
pub fn zmod_stream_totals_full_measures(
    measures: &[usize],
    constant_bpm: bool,
) -> (Vec<StreamSegment>, f32, f32) {
    let addition = 2usize;

    let mut threshold = 14 + addition;
    let mut multiplier = 1.0_f32;
    if constant_bpm {
        threshold = 30 + addition;
        multiplier = 2.0;

        let d32 = zmod_stream_density(measures, threshold, multiplier);
        if d32 < 0.2 {
            threshold = 22 + addition;
            multiplier = 1.5;
            let [d24, d20] =
                zmod_stream_density_pair(measures, [(22 + addition, 1.5), (18 + addition, 1.25)]);
            if d24 < 0.2 {
                threshold = 18 + addition;
                multiplier = 1.25;
                if d20 < 0.2 {
                    threshold = 14 + addition;
                    multiplier = 1.0;
                }
            }
        }
    }

    let segs = stream_sequences_threshold(measures, threshold);
    if segs.is_empty() {
        return (segs, 0.0, 0.0);
    }

    let mut total_stream = 0.0_f32;
    let mut total_break = 0.0_f32;
    let mut edge_break = 0.0_f32;
    let mut last_stream = false;
    let len = segs.len();
    for (i, seg) in segs.iter().enumerate() {
        let seg_len = seg.end.saturating_sub(seg.start) as f32;
        if seg_len <= 0.0 {
            continue;
        }
        if seg.is_break && i > 0 && i + 1 < len {
            total_break += seg_len;
            last_stream = false;
        } else if seg.is_break {
            edge_break += seg_len;
            last_stream = false;
        } else {
            if last_stream {
                total_break += 1.0;
            }
            total_stream += seg_len;
            last_stream = true;
        }
    }

    if total_stream + total_break < 10.0 || total_stream + total_break < edge_break {
        total_break += edge_break;
    }

    (segs, total_stream * multiplier, total_break * multiplier)
}

fn measure_densities_impl<const LANES: usize>(data: &[u8]) -> Vec<usize> {
    let mut densities = Vec::with_capacity(data.len() / ((LANES + 1) * 4) + 1);
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
            b',' => push_density_measure(&mut measure_steps, &mut densities),
            b';' => {
                push_density_measure(&mut measure_steps, &mut densities);
                done = true;
                break;
            }
            _ if line.len() >= LANES => {
                measure_steps += usize::from(density_row_has_step::<LANES>(line));
            }
            _ => {}
        }
    }

    if !done {
        push_density_measure(&mut measure_steps, &mut densities);
    }

    densities
}

fn push_density_measure(measure_steps: &mut usize, densities: &mut Vec<usize>) {
    densities.push(*measure_steps);
    *measure_steps = 0;
}

fn density_row_has_step<const LANES: usize>(line: &[u8]) -> bool {
    line[..LANES]
        .iter()
        .any(|byte| matches!(byte, b'1' | b'2' | b'4'))
}

fn trim_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn skip_ws(mut line: &[u8]) -> &[u8] {
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
        (seg.start, seg.end, seg.is_break)
    }

    fn stream_sequences_reference(measures: &[usize], threshold: usize) -> Vec<StreamSegment> {
        let streams: Vec<_> = measures
            .iter()
            .enumerate()
            .filter(|(_, density)| **density >= threshold)
            .map(|(idx, _)| idx + 1)
            .collect();
        if streams.is_empty() {
            return Vec::new();
        }

        let mut segments = Vec::new();
        let first_break = streams[0].saturating_sub(1);
        if first_break >= 2 {
            segments.push(StreamSegment {
                start: 0,
                end: first_break,
                is_break: true,
            });
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
            segments.push(StreamSegment {
                start: stream_end - count,
                end: stream_end,
                is_break: false,
            });
            let break_end = if next == usize::MAX {
                measures.len()
            } else {
                next - 1
            };
            if break_end >= current + 2 {
                segments.push(StreamSegment {
                    start: current,
                    end: break_end,
                    is_break: true,
                });
            }
            count = 1;
            end = None;
        }
        segments
    }

    fn materialized_stream_density(measures: &[usize], threshold: usize, multiplier: f32) -> f32 {
        let segments = stream_sequences_reference(measures, threshold);
        let mut total_stream = 0.0_f32;
        let mut total_measures = 0.0_f32;
        for segment in segments {
            let len = ((segment.end.saturating_sub(segment.start)) as f32 * multiplier).floor();
            if len <= 0.0 {
                continue;
            }
            if !segment.is_break {
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
                    .map(|idx| usize::from(mask & (1 << idx) != 0) * 16)
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
    fn streamed_density_matches_materialized_segments() {
        let measures: Vec<_> = (0..257).map(|idx| (idx * 37 + idx / 7) % 40).collect();
        for threshold in [1, 8, 16, 20, 24, 32, 40] {
            for multiplier in [1.0, 1.25, 1.5, 2.0] {
                assert_eq!(
                    zmod_stream_density(&measures, threshold, multiplier).to_bits(),
                    materialized_stream_density(&measures, threshold, multiplier).to_bits(),
                    "threshold {threshold}, multiplier {multiplier}"
                );
            }
        }
        let pair = zmod_stream_density_pair(&measures, [(24, 1.5), (20, 1.25)]);
        assert_eq!(
            pair.map(f32::to_bits),
            [
                materialized_stream_density(&measures, 24, 1.5).to_bits(),
                materialized_stream_density(&measures, 20, 1.25).to_bits(),
            ]
        );
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
        let measures = [32usize; 8];
        let (_segs, total_stream, total_break) = zmod_stream_totals_full_measures(&measures, true);

        assert_eq!(total_stream, 16.0);
        assert_eq!(total_break, 0.0);
    }
}
