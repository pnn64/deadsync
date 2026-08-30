use arrayvec::ArrayVec;
use deadsync_core::timing::ROWS_PER_BEAT;
use std::cmp::Ordering;

const INLINE_BPM_SEGMENTS: usize = 16;
const ESTIMATED_COMPONENT_BYTES: usize = 8;

#[derive(Clone, Copy, Debug)]
struct BpmSegment {
    beat: f64,
    bpm: f64,
    seconds_at_beat: f64,
}

#[derive(Clone, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "the inline variant deliberately keeps common BPM maps off the heap"
)]
enum BpmSegments {
    Inline(ArrayVec<BpmSegment, INLINE_BPM_SEGMENTS>),
    Heap(Vec<BpmSegment>),
}

impl BpmSegments {
    fn push(&mut self, segment: BpmSegment, estimated_capacity: usize) {
        match self {
            Self::Inline(inline) if inline.len() < INLINE_BPM_SEGMENTS => inline.push(segment),
            Self::Inline(inline) => {
                let mut heap = Vec::with_capacity(
                    estimated_capacity.max(INLINE_BPM_SEGMENTS.saturating_add(1)),
                );
                heap.extend(inline.iter().copied());
                heap.push(segment);
                *self = Self::Heap(heap);
            }
            Self::Heap(heap) => heap.push(segment),
        }
    }

    fn insert_front(&mut self, segment: BpmSegment, estimated_capacity: usize) {
        match self {
            Self::Inline(inline) if inline.len() < INLINE_BPM_SEGMENTS => {
                inline.insert(0, segment);
            }
            Self::Inline(inline) => {
                let mut heap = Vec::with_capacity(
                    estimated_capacity.max(INLINE_BPM_SEGMENTS.saturating_add(1)),
                );
                heap.push(segment);
                heap.extend(inline.iter().copied());
                *self = Self::Heap(heap);
            }
            Self::Heap(heap) => heap.insert(0, segment),
        }
    }

    fn as_slice(&self) -> &[BpmSegment] {
        match self {
            Self::Inline(inline) => inline.as_slice(),
            Self::Heap(heap) => heap.as_slice(),
        }
    }

    fn as_mut_slice(&mut self) -> &mut [BpmSegment] {
        match self {
            Self::Inline(inline) => inline.as_mut_slice(),
            Self::Heap(heap) => heap.as_mut_slice(),
        }
    }
}

/// Parsed BPM timing retained for repeated beat/second conversions.
///
/// Up to 16 segments are stored inline. Larger pathological maps spill once
/// to the heap, while lookups reuse cumulative segment times without further
/// allocation.
#[derive(Clone, Debug)]
pub struct BpmTimeline {
    segments: BpmSegments,
    ordered: bool,
}

impl Default for BpmTimeline {
    fn default() -> Self {
        Self::new("")
    }
}

impl BpmTimeline {
    #[must_use]
    pub fn new(normalized_bpms: &str) -> Self {
        let estimated_capacity = normalized_bpms.len().div_ceil(ESTIMATED_COMPONENT_BYTES);
        let mut segments = BpmSegments::Inline(ArrayVec::new());
        let mut previous_beat = f64::NEG_INFINITY;
        let mut ordered = true;
        for component in normalized_bpms.split(',') {
            let Some((left, right)) = component.split_once('=') else {
                continue;
            };
            let Some(beat) = parse_beat_or_row(left) else {
                continue;
            };
            let Some(bpm) = right
                .trim()
                .parse::<f64>()
                .ok()
                .map(|value| f64::from(value as f32))
            else {
                continue;
            };
            ordered &= beat >= previous_beat;
            previous_beat = beat;
            segments.push(
                BpmSegment {
                    beat,
                    bpm,
                    seconds_at_beat: 0.0,
                },
                estimated_capacity,
            );
        }

        if !ordered {
            match &mut segments {
                BpmSegments::Inline(inline) => stable_insertion_sort(inline.as_mut_slice()),
                BpmSegments::Heap(heap) => heap.sort_by(segment_beat_cmp),
            }
        }

        if segments.as_slice().is_empty() {
            segments.push(
                BpmSegment {
                    beat: 0.0,
                    bpm: 60.0,
                    seconds_at_beat: 0.0,
                },
                estimated_capacity,
            );
        } else {
            let first = segments.as_slice()[0];
            if first.beat != 0.0 {
                // Preserve the old synthetic zero segment and its floating-point
                // operation order exactly. Negative-beat input becomes
                // intentionally non-monotonic and uses the linear fallback.
                segments.insert_front(
                    BpmSegment {
                        beat: 0.0,
                        bpm: first.bpm,
                        seconds_at_beat: 0.0,
                    },
                    estimated_capacity,
                );
            }
        }

        ordered = segments
            .as_slice()
            .windows(2)
            .all(|pair| pair[0].beat <= pair[1].beat);
        if ordered {
            populate_cumulative_seconds(segments.as_mut_slice());
        }
        Self { segments, ordered }
    }

    #[must_use]
    pub fn sec_at_beat(&self, target_beat: f64) -> f64 {
        if !target_beat.is_finite() || target_beat <= 0.0 {
            return 0.0;
        }
        if !self.ordered {
            return sec_at_beat_linear(self.segments.as_slice(), target_beat);
        }
        let segments = self.segments.as_slice();
        let index = segments
            .partition_point(|segment| segment.beat <= target_beat)
            .saturating_sub(1);
        let segment = segments[index];
        let delta_beats = (target_beat - segment.beat).max(0.0);
        let mut time = segment.seconds_at_beat;
        if segment.bpm > 0.0 {
            time += (delta_beats * 60.0) / segment.bpm;
        }
        time.max(0.0)
    }

    #[must_use]
    pub fn beat_at_sec(&self, target_sec: f64) -> f64 {
        if !target_sec.is_finite() || target_sec <= 0.0 {
            return 0.0;
        }
        if !self.ordered {
            return beat_at_sec_linear(self.segments.as_slice(), target_sec);
        }
        let segments = self.segments.as_slice();
        let next = segments.partition_point(|segment| segment.seconds_at_beat < target_sec);
        let segment = segments[next.saturating_sub(1).min(segments.len() - 1)];
        let remain = (target_sec - segment.seconds_at_beat).max(0.0);
        let add_beats = if segment.bpm > 0.0 {
            remain * segment.bpm / 60.0
        } else {
            0.0
        };
        (segment.beat + add_beats).max(0.0)
    }
}

#[must_use]
pub fn sec_at_beat_from_bpms(normalized_bpms: &str, target_beat: f64) -> f64 {
    BpmTimeline::new(normalized_bpms).sec_at_beat(target_beat)
}

#[must_use]
pub fn beat_at_sec_from_bpms(normalized_bpms: &str, target_sec: f64) -> f64 {
    BpmTimeline::new(normalized_bpms).beat_at_sec(target_sec)
}

fn parse_beat_or_row(raw: &str) -> Option<f64> {
    let mut value = raw.trim();
    let is_row = value
        .strip_suffix(['r', 'R'])
        .is_some_and(|without_suffix| {
            value = without_suffix.trim_end();
            true
        });
    let value = value
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())?;
    Some(if is_row {
        f64::from(value) / f64::from(ROWS_PER_BEAT)
    } else {
        f64::from(value)
    })
}

fn stable_insertion_sort(segments: &mut [BpmSegment]) {
    for index in 1..segments.len() {
        let value = segments[index];
        let mut destination = index;
        while destination > 0 && value.beat < segments[destination - 1].beat {
            segments[destination] = segments[destination - 1];
            destination -= 1;
        }
        segments[destination] = value;
    }
}

fn segment_beat_cmp(left: &BpmSegment, right: &BpmSegment) -> Ordering {
    left.beat
        .partial_cmp(&right.beat)
        .unwrap_or(Ordering::Equal)
}

fn populate_cumulative_seconds(segments: &mut [BpmSegment]) {
    let mut elapsed = 0.0;
    let mut last_beat = segments[0].beat;
    let mut last_bpm = segments[0].bpm;
    segments[0].seconds_at_beat = 0.0;
    for segment in &mut segments[1..] {
        if segment.beat > last_beat && last_bpm > 0.0 {
            elapsed += ((segment.beat - last_beat) * 60.0) / last_bpm;
        }
        segment.seconds_at_beat = elapsed;
        last_beat = segment.beat;
        last_bpm = segment.bpm;
    }
}

fn sec_at_beat_linear(segments: &[BpmSegment], target_beat: f64) -> f64 {
    let mut time = 0.0;
    let mut last_beat = 0.0;
    let mut last_bpm = segments[0].bpm;
    for segment in segments {
        if target_beat <= segment.beat {
            let delta_beats = (target_beat - last_beat).max(0.0);
            if last_bpm > 0.0 {
                time += (delta_beats * 60.0) / last_bpm;
            }
            return time.max(0.0);
        }
        if segment.beat > last_beat && last_bpm > 0.0 {
            time += ((segment.beat - last_beat) * 60.0) / last_bpm;
        }
        last_beat = segment.beat;
        last_bpm = segment.bpm;
    }
    if last_bpm > 0.0 {
        time += ((target_beat - last_beat).max(0.0) * 60.0) / last_bpm;
    }
    time.max(0.0)
}

fn beat_at_sec_linear(segments: &[BpmSegment], target_sec: f64) -> f64 {
    let mut elapsed = 0.0;
    let mut last_beat = 0.0;
    let mut last_bpm = segments[0].bpm;
    for segment in segments {
        let delta_beats = (segment.beat - last_beat).max(0.0);
        let delta_sec = if last_bpm > 0.0 {
            (delta_beats * 60.0) / last_bpm
        } else {
            0.0
        };
        if elapsed + delta_sec >= target_sec {
            let remain = (target_sec - elapsed).max(0.0);
            let add_beats = if last_bpm > 0.0 {
                remain * last_bpm / 60.0
            } else {
                0.0
            };
            return (last_beat + add_beats).max(0.0);
        }
        elapsed += delta_sec;
        last_beat = segment.beat;
        last_bpm = segment.bpm;
    }
    let remain = (target_sec - elapsed).max(0.0);
    let add_beats = if last_bpm > 0.0 {
        remain * last_bpm / 60.0
    } else {
        0.0
    };
    (last_beat + add_beats).max(0.0)
}

#[cfg(any(test, feature = "bench-support"))]
fn normalized_bpm_map_reference(normalized_bpms: &str) -> Vec<(f64, f64)> {
    let mut bpm_map = rssp::bpm::parse_bpm_map(normalized_bpms);
    if bpm_map.is_empty() {
        bpm_map.push((0.0, 60.0));
    }
    if bpm_map.first().is_none_or(|(beat, _)| *beat != 0.0) {
        let first_bpm = bpm_map[0].1;
        bpm_map.insert(0, (0.0, first_bpm));
    }
    bpm_map
}

#[cfg(any(test, feature = "bench-support"))]
fn sec_at_beat_from_map_reference(bpm_map: &[(f64, f64)], target_beat: f64) -> f64 {
    if !target_beat.is_finite() || target_beat <= 0.0 {
        return 0.0;
    }
    let mut time = 0.0;
    let mut last_beat = 0.0;
    let mut last_bpm = bpm_map[0].1;
    for &(beat, bpm) in bpm_map {
        if target_beat <= beat {
            let delta_beats = (target_beat - last_beat).max(0.0);
            if last_bpm > 0.0 {
                time += (delta_beats * 60.0) / last_bpm;
            }
            return time.max(0.0);
        }
        if beat > last_beat && last_bpm > 0.0 {
            time += ((beat - last_beat) * 60.0) / last_bpm;
        }
        last_beat = beat;
        last_bpm = bpm;
    }
    if last_bpm > 0.0 {
        time += ((target_beat - last_beat).max(0.0) * 60.0) / last_bpm;
    }
    time.max(0.0)
}

#[cfg(any(test, feature = "bench-support"))]
fn beat_at_sec_from_map_reference(bpm_map: &[(f64, f64)], target_sec: f64) -> f64 {
    if !target_sec.is_finite() || target_sec <= 0.0 {
        return 0.0;
    }
    let mut elapsed = 0.0;
    let mut last_beat = 0.0;
    let mut last_bpm = bpm_map[0].1;
    for &(beat, bpm) in bpm_map {
        let delta_beats = (beat - last_beat).max(0.0);
        let delta_sec = if last_bpm > 0.0 {
            (delta_beats * 60.0) / last_bpm
        } else {
            0.0
        };
        if elapsed + delta_sec >= target_sec {
            let remain = (target_sec - elapsed).max(0.0);
            let add_beats = if last_bpm > 0.0 {
                remain * last_bpm / 60.0
            } else {
                0.0
            };
            return (last_beat + add_beats).max(0.0);
        }
        elapsed += delta_sec;
        last_beat = beat;
        last_bpm = bpm;
    }
    let remain = (target_sec - elapsed).max(0.0);
    let add_beats = if last_bpm > 0.0 {
        remain * last_bpm / 60.0
    } else {
        0.0
    };
    (last_beat + add_beats).max(0.0)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn sec_at_beat_from_bpms_reference(normalized_bpms: &str, target_beat: f64) -> f64 {
    sec_at_beat_from_map_reference(&normalized_bpm_map_reference(normalized_bpms), target_beat)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn beat_at_sec_from_bpms_reference(normalized_bpms: &str, target_sec: f64) -> f64 {
    beat_at_sec_from_map_reference(&normalized_bpm_map_reference(normalized_bpms), target_sec)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub struct BpmTimelineReference {
    bpm_map: Vec<(f64, f64)>,
}

#[cfg(feature = "bench-support")]
impl BpmTimelineReference {
    #[must_use]
    pub fn new(normalized_bpms: &str) -> Self {
        Self {
            bpm_map: normalized_bpm_map_reference(normalized_bpms),
        }
    }

    #[must_use]
    pub fn beat_at_sec(&self, target_sec: f64) -> f64 {
        beat_at_sec_from_map_reference(&self.bpm_map, target_sec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_same(left: f64, right: f64, context: &str) {
        assert!(
            left.to_bits() == right.to_bits() || (left.is_nan() && right.is_nan()),
            "{context}: left={left:?}, right={right:?}"
        );
    }

    #[test]
    fn sec_at_beat_uses_bpm_segments() {
        let bpms = "0.000=120.000,4.000=60.000";

        assert_eq!(sec_at_beat_from_bpms(bpms, 4.0), 2.0);
        assert_eq!(sec_at_beat_from_bpms(bpms, 6.0), 4.0);
    }

    #[test]
    fn beat_at_sec_uses_bpm_segments() {
        let bpms = "0.000=120.000,4.000=60.000";

        assert_eq!(beat_at_sec_from_bpms(bpms, 2.0), 4.0);
        assert_eq!(beat_at_sec_from_bpms(bpms, 4.0), 6.0);
    }

    #[test]
    fn empty_bpm_map_defaults_to_sixty_bpm() {
        assert_eq!(sec_at_beat_from_bpms("", 2.0), 2.0);
        assert_eq!(beat_at_sec_from_bpms("", 2.0), 2.0);
    }

    #[test]
    fn optimized_timeline_matches_allocating_reference() {
        let maps = [
            "",
            "0=120",
            "4=120,8=180,16=90",
            "8=180,4=140,0=120,4=150",
            "96r=150,bad,48R=120,NaN=999,144r=nope,0=90",
            " , 0=120, missing, 4=150, =180, 8=nope, ",
            "4=120,4=150,8=0,12=-20,16=200",
            "-4=90,0=120,8=180",
            "0=NaN,4=120,8=inf,12=180",
            "32=200,64=100",
        ];
        let targets = [
            f64::NEG_INFINITY,
            -1.0,
            0.0,
            0.125,
            1.0,
            2.0,
            4.0,
            7.5,
            16.0,
            64.0,
            1_000.0,
            f64::INFINITY,
            f64::NAN,
        ];

        for map in maps {
            let timeline = BpmTimeline::new(map);
            for target in targets {
                assert_same(
                    sec_at_beat_from_bpms_reference(map, target),
                    timeline.sec_at_beat(target),
                    &format!("seconds map={map:?}, target={target:?}"),
                );
                assert_same(
                    beat_at_sec_from_bpms_reference(map, target),
                    timeline.beat_at_sec(target),
                    &format!("beats map={map:?}, target={target:?}"),
                );
            }
        }
    }

    #[test]
    fn heap_spill_matches_reference_beyond_inline_capacity() {
        let mut map = String::new();
        for index in (0..64).rev() {
            if !map.is_empty() {
                map.push(',');
            }
            use std::fmt::Write as _;
            write!(&mut map, "{}={}", index * 4, 90 + index).unwrap();
        }
        let timeline = BpmTimeline::new(&map);
        for target in [1.0, 31.0, 127.0, 255.0, 511.0] {
            assert_same(
                sec_at_beat_from_bpms_reference(&map, target),
                timeline.sec_at_beat(target),
                "heap seconds",
            );
            assert_same(
                beat_at_sec_from_bpms_reference(&map, target),
                timeline.beat_at_sec(target),
                "heap beats",
            );
        }
    }
}
