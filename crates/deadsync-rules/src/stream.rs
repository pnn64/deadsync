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
    let streams: Vec<_> = measures
        .iter()
        .enumerate()
        .filter(|(_, n)| **n >= threshold)
        .map(|(i, _)| i + 1)
        .collect();

    if streams.is_empty() {
        return Vec::new();
    }

    let mut segs = Vec::new();
    let first_break = streams[0].saturating_sub(1);
    if first_break >= 2 {
        segs.push(StreamSegment {
            start: 0,
            end: first_break,
            is_break: true,
        });
    }

    let (mut count, mut end) = (1usize, None);
    for (i, &cur) in streams.iter().enumerate() {
        let next = streams.get(i + 1).copied().unwrap_or(usize::MAX);
        if cur + 1 == next {
            count += 1;
            end = Some(cur + 1);
            continue;
        }

        let e = end.unwrap_or(cur);
        segs.push(StreamSegment {
            start: e - count,
            end: e,
            is_break: false,
        });

        let bstart = cur;
        let bend = if next == usize::MAX {
            measures.len()
        } else {
            next - 1
        };
        if bend >= bstart + 2 {
            segs.push(StreamSegment {
                start: bstart,
                end: bend,
                is_break: true,
            });
        }
        count = 1;
        end = None;
    }
    segs
}

#[inline(always)]
fn zmod_stream_density(measures: &[usize], threshold: usize, multiplier: f32) -> f32 {
    let segs = stream_sequences_threshold(measures, threshold);
    if segs.is_empty() {
        return 0.0;
    }
    let mut total_stream = 0.0_f32;
    let mut total_measures = 0.0_f32;
    for seg in &segs {
        let seg_len = ((seg.end.saturating_sub(seg.start)) as f32 * multiplier).floor();
        if seg_len <= 0.0 {
            continue;
        }
        if !seg.is_break {
            total_stream += seg_len;
        }
        total_measures += seg_len;
    }
    if total_measures <= 0.0 {
        0.0
    } else {
        total_stream / total_measures
    }
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
            let d24 = zmod_stream_density(measures, threshold, multiplier);
            if d24 < 0.2 {
                threshold = 18 + addition;
                multiplier = 1.25;
                let d20 = zmod_stream_density(measures, threshold, multiplier);
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

const DENSITY_ROW_ZERO: u8 = 1;
const DENSITY_ROW_STEP: u8 = 1 << 1;

fn measure_densities_impl<const LANES: usize>(data: &[u8]) -> Vec<usize> {
    let mut densities = Vec::with_capacity(data.len() / ((LANES + 1) * 4) + 1);
    let mut measure = Vec::with_capacity(64);
    let mut measure_steps = 0usize;
    let mut done = false;

    for raw in data.split(|&byte| byte == b'\n') {
        let line = skip_ws(trim_cr(raw));
        if line.is_empty() || line[0] == b'/' {
            continue;
        }

        match line[0] {
            b',' => push_density_measure(&mut measure, &mut measure_steps, &mut densities),
            b';' => {
                push_density_measure(&mut measure, &mut measure_steps, &mut densities);
                done = true;
                break;
            }
            _ if line.len() >= LANES => {
                let flags = density_row_flags::<LANES>(line);
                measure_steps += usize::from((flags & DENSITY_ROW_STEP) != 0);
                measure.push(flags);
            }
            _ => {}
        }
    }

    if !done {
        push_density_measure(&mut measure, &mut measure_steps, &mut densities);
    }

    densities
}

fn push_density_measure(
    measure: &mut Vec<u8>,
    measure_steps: &mut usize,
    densities: &mut Vec<usize>,
) {
    if measure.is_empty() {
        densities.push(0);
        return;
    }
    let shift = density_reduce_shift(measure);
    let density = if shift == 0 {
        *measure_steps
    } else {
        let step = 1usize << shift;
        let len = measure.len() >> shift;
        (0..len)
            .map(|i| usize::from((measure[i * step] & DENSITY_ROW_STEP) != 0))
            .sum()
    };
    densities.push(density);
    measure.clear();
    *measure_steps = 0;
}

fn density_reduce_shift(measure: &[u8]) -> usize {
    if measure.len() < 2 {
        return 0;
    }

    let mut shift = 0usize;
    let mut step = 2usize;
    for _ in 0..measure.len().trailing_zeros() {
        let mut i = step / 2;
        while i < measure.len() {
            if (measure[i] & DENSITY_ROW_ZERO) == 0 {
                return shift;
            }
            i += step;
        }
        shift += 1;
        step <<= 1;
    }
    shift
}

fn density_row_flags<const LANES: usize>(line: &[u8]) -> u8 {
    let mut all_zero = true;
    let mut has_step = false;
    for &byte in &line[..LANES] {
        all_zero &= byte == b'0';
        has_step |= matches!(byte, b'1' | b'2' | b'4');
    }
    u8::from(all_zero) | (u8::from(has_step) << 1)
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
