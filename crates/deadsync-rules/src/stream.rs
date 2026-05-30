#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamSegment {
    pub start: usize,
    pub end: usize,
    pub is_break: bool,
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
