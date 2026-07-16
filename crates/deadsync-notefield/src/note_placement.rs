use crate::{ScrollTravel, notes::ScrollRangeKey};
use deadsync_core::timing::{ROWS_PER_BEAT, beat_to_note_row};
use deadsync_rules::note::Note;
use std::mem::size_of;

const RANGE_GUARD_ROWS: i32 = ROWS_PER_BEAT * 4;
const MAX_INCREMENT_NS: i64 = 250_000_000;
const MAX_INCREMENT_BEATS: f32 = 1.0;
const REANCHOR_BEATS: f32 = 8.0;
const CAPACITY_WINDOW_BEATS: f32 = 48.0;
const CAPACITY_MARGIN: usize = 128;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct NotePlacement {
    pub note_index: u32,
    pub local_col: u8,
    pub adjusted_travel: f32,
    pub center: [f32; 2],
    pub actor_alpha: f32,
    pub glow_alpha: f32,
    pub world_z: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct NotefieldPlacementPlan<'a> {
    pub visible_row_range: Option<(i32, i32)>,
    pub notes: &'a [NotePlacement],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NotefieldPlacementScratchStats {
    pub full_range_solves: u64,
    pub incremental_ranges: u64,
    pub capacity_growths: u64,
    pub max_placements: usize,
}

#[derive(Clone, Copy, Debug)]
struct RangeCursor {
    key: ScrollRangeKey,
    range: Option<(i32, i32)>,
    search_beat: f32,
    anchor_beat: f32,
    current_time_ns: i64,
}

/// Song-lifetime storage for one player's per-frame note placement plan.
///
/// Owner/thread model: the gameplay presentation thread owns this value and
/// accesses it through the theme's per-player `RefCell`; it is not thread-safe.
/// Lifetime/capacity: one song, pre-sized from the densest 48-beat chart window
/// plus a fixed safety margin. Warmup happens while gameplay `State` is built.
/// A normal frame clears and refills the existing allocation. If malformed or
/// extreme chart data exceeds the estimate, correctness wins: `Vec` grows once
/// and `capacity_growths` records the miss. There is no eviction or pruning;
/// memory is freed when gameplay state is dropped. Range cursors do no timing
/// samples on incremental frames and fall back to one bounded 44-sample solve
/// after seeks, modifier changes, stalls, or eight beats of accumulated drift.
#[derive(Debug, Default)]
pub struct NotefieldPlacementScratch {
    placements: Vec<NotePlacement>,
    cursor: Option<RangeCursor>,
    stats: NotefieldPlacementScratchStats,
}

impl NotefieldPlacementScratch {
    pub fn with_notes(notes: &[Note]) -> Self {
        Self {
            placements: Vec::with_capacity(placement_capacity(notes)),
            cursor: None,
            stats: NotefieldPlacementScratchStats::default(),
        }
    }

    pub fn stats(&self) -> NotefieldPlacementScratchStats {
        self.stats
    }

    pub fn capacity(&self) -> usize {
        self.placements.capacity()
    }

    pub fn fixed_bytes(&self) -> usize {
        self.placements.capacity() * size_of::<NotePlacement>()
    }

    pub(crate) fn begin_frame(&mut self, travel: &ScrollTravel<'_>) -> Option<(i32, i32)> {
        self.placements.clear();
        let key = travel.range_key();
        let search_beat = travel.search_beat();
        let current_time_ns = travel.current_time_ns();
        if let Some(cursor) = self.cursor {
            let delta_beat = search_beat - cursor.search_beat;
            let delta_ns = current_time_ns.saturating_sub(cursor.current_time_ns);
            let stalled = delta_ns > 2_000_000 && delta_beat.abs() <= 0.000_01;
            let can_advance = cursor.key == key
                && search_beat.is_finite()
                && (0.0..=MAX_INCREMENT_BEATS).contains(&delta_beat)
                && (0..=MAX_INCREMENT_NS).contains(&delta_ns)
                && !stalled
                && search_beat - cursor.anchor_beat <= REANCHOR_BEATS;
            if can_advance {
                let row_delta = beat_to_note_row(search_beat)
                    .saturating_sub(beat_to_note_row(cursor.search_beat));
                let range = cursor.range.map(|(low, high)| {
                    (
                        low.saturating_add(row_delta),
                        high.saturating_add(row_delta),
                    )
                });
                self.cursor = Some(RangeCursor {
                    range,
                    search_beat,
                    current_time_ns,
                    ..cursor
                });
                self.stats.incremental_ranges += 1;
                return range;
            }
        }

        let range = expand_range(travel.visible_row_range());
        self.cursor = Some(RangeCursor {
            key,
            range,
            search_beat,
            anchor_beat: search_beat,
            current_time_ns,
        });
        self.stats.full_range_solves += 1;
        range
    }

    pub(crate) fn push(&mut self, placement: NotePlacement) {
        if self.placements.len() == self.placements.capacity() {
            self.stats.capacity_growths += 1;
        }
        self.placements.push(placement);
        self.stats.max_placements = self.stats.max_placements.max(self.placements.len());
    }

    pub(crate) fn plan(&self, visible_row_range: Option<(i32, i32)>) -> NotefieldPlacementPlan<'_> {
        NotefieldPlacementPlan {
            visible_row_range,
            notes: &self.placements,
        }
    }
}

fn expand_range(range: Option<(i32, i32)>) -> Option<(i32, i32)> {
    range.map(|(low, high)| {
        (
            low.saturating_sub(RANGE_GUARD_ROWS),
            high.saturating_add(RANGE_GUARD_ROWS),
        )
    })
}

fn placement_capacity(notes: &[Note]) -> usize {
    if notes.is_empty() {
        return 0;
    }
    if notes.iter().any(|note| !note.beat.is_finite()) {
        return notes.len();
    }
    let mut left = 0;
    let mut densest = 0;
    for right in 0..notes.len() {
        while left < right && notes[right].beat - notes[left].beat > CAPACITY_WINDOW_BEATS {
            left += 1;
        }
        densest = densest.max(right - left + 1);
    }
    densest.saturating_add(CAPACITY_MARGIN).min(notes.len())
}

#[cfg(feature = "bench-support")]
mod bench {
    use super::{NotePlacement, NotefieldPlacementScratch};
    use crate::{
        AccelYParams, ScrollTravel, ScrollTravelRequest, appearance_note_actor_alpha,
        appearance_note_actor_alpha_from_alpha, appearance_note_alpha, appearance_note_glow,
        appearance_note_glow_from_alpha, for_each_visible_note_index, scroll_travel,
    };
    use deadsync_core::note::NoteType;
    use deadsync_core::song_time::song_time_ns_add_seconds;
    use deadsync_core::timing::beat_to_note_row;
    use deadsync_rules::note::Note;
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::timing::{TimingData, TimingSegments};

    const LANES: usize = 4;
    const FRAME_RATE: f32 = 120.0;
    const SONG_FRAMES: usize = 120 * 90;
    const DRAW_AFTER: f32 = 320.0;
    const DRAW_BEFORE: f32 = 640.0;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct PlacementBenchFrame {
        pub checksum: u64,
        pub placements: usize,
    }

    pub struct PlacementBench {
        timing: TimingData,
        notes: Vec<Note>,
        lanes: [Vec<usize>; LANES],
        scratch: NotefieldPlacementScratch,
    }

    impl Default for PlacementBench {
        fn default() -> Self {
            let timing = TimingData::from_segments(
                0.0,
                0.0,
                &TimingSegments {
                    bpms: vec![(0.0, 120.0), (64.0, 180.0), (128.0, 90.0)],
                    ..TimingSegments::default()
                },
                &[],
            );
            let mut notes = Vec::with_capacity(256 * 16 * LANES);
            let mut lanes: [Vec<usize>; LANES] = std::array::from_fn(|_| Vec::new());
            for row in 0..256 * 16 {
                let beat = row as f32 / 16.0;
                for (lane, indices) in lanes.iter_mut().enumerate() {
                    let note_index = notes.len();
                    notes.push(Note {
                        beat,
                        quantization_idx: (row % 8) as u8,
                        column: lane,
                        note_type: NoteType::Tap,
                        row_index: beat_to_note_row(beat).max(0) as usize,
                        result: None,
                        early_result: None,
                        hold: None,
                        mine_result: None,
                        is_fake: false,
                        can_be_judged: true,
                    });
                    indices.push(note_index);
                }
            }
            let scratch = NotefieldPlacementScratch::with_notes(&notes);
            Self {
                timing,
                notes,
                lanes,
                scratch,
            }
        }
    }

    impl PlacementBench {
        pub fn old_frame(&self, frame: usize) -> PlacementBenchFrame {
            let travel = bench_travel(&self.timing, frame);
            let range = travel.visible_row_range();
            let _hold_range = travel.visible_row_range();
            let mut output = PlacementBenchFrame::default();
            for (local_col, indices) in self.lanes.iter().enumerate() {
                for_each_visible_note_index(indices, &self.notes, range, |note_index| {
                    let note = &self.notes[note_index];
                    let raw = travel.raw_note(note, false);
                    let adjusted = travel.adjusted(raw);
                    if !(-DRAW_AFTER..=DRAW_BEFORE).contains(&adjusted) {
                        return;
                    }
                    let lane_offset = travel.lane_offset(local_col);
                    let y = travel.lane_y(local_col, 160.0, 1.0, raw);
                    let alpha = appearance_note_actor_alpha(
                        travel.adjusted(raw) + lane_offset,
                        frame as f32 / FRAME_RATE,
                        0.0,
                        bench_alpha_params(),
                    );
                    let glow = appearance_note_glow(
                        travel.adjusted(raw) + lane_offset,
                        frame as f32 / FRAME_RATE,
                        0.0,
                        bench_alpha_params(),
                    );
                    if alpha <= f32::EPSILON && glow <= f32::EPSILON {
                        return;
                    }
                    let x = 320.0 + local_col as f32 * 64.0 + travel.adjusted(raw) * 0.1;
                    add_output(&mut output, note_index, adjusted, x, y, alpha, glow);
                });
            }
            output
        }

        pub fn new_frame(&mut self, frame: usize) -> PlacementBenchFrame {
            let travel = bench_travel(&self.timing, frame);
            let range = self.scratch.begin_frame(&travel);
            for (local_col, indices) in self.lanes.iter().enumerate() {
                for_each_visible_note_index(indices, &self.notes, range, |note_index| {
                    let note = &self.notes[note_index];
                    let adjusted = travel.adjusted(travel.raw_note(note, false));
                    if !(-DRAW_AFTER..=DRAW_BEFORE).contains(&adjusted) {
                        return;
                    }
                    let lane_offset = travel.lane_offset(local_col);
                    let percent = appearance_note_alpha(
                        adjusted + lane_offset,
                        frame as f32 / FRAME_RATE,
                        0.0,
                        bench_alpha_params(),
                    );
                    let actor_alpha = appearance_note_actor_alpha_from_alpha(percent);
                    let glow_alpha = appearance_note_glow_from_alpha(percent);
                    if actor_alpha <= f32::EPSILON && glow_alpha <= f32::EPSILON {
                        return;
                    }
                    self.scratch.push(NotePlacement {
                        note_index: note_index as u32,
                        local_col: local_col as u8,
                        adjusted_travel: adjusted,
                        center: [
                            320.0 + local_col as f32 * 64.0 + adjusted * 0.1,
                            160.0 + adjusted + lane_offset,
                        ],
                        actor_alpha,
                        glow_alpha,
                        world_z: 0.0,
                    });
                });
            }
            let mut output = PlacementBenchFrame::default();
            for note in self.scratch.plan(range).notes {
                add_output(
                    &mut output,
                    note.note_index as usize,
                    note.adjusted_travel,
                    note.center[0],
                    note.center[1],
                    note.actor_alpha,
                    note.glow_alpha,
                );
            }
            output
        }

        pub fn fixed_bytes(&self) -> usize {
            self.scratch.fixed_bytes()
        }

        pub fn capacity_growths(&self) -> u64 {
            self.scratch.stats().capacity_growths
        }
    }

    fn bench_travel(timing: &TimingData, frame: usize) -> ScrollTravel<'_> {
        let frame = frame % SONG_FRAMES;
        let elapsed = frame as f32 / FRAME_RATE;
        let start = timing.get_time_for_beat_ns(4.0);
        let time_ns = song_time_ns_add_seconds(start, elapsed);
        let beat = timing.get_beat_for_time_ns(time_ns);
        scroll_travel(ScrollTravelRequest {
            timing,
            accel: AccelYParams {
                boost: 0.25,
                brake: 0.1,
                wave: 0.2,
                boomerang: 0.0,
                expand: 0.0,
            },
            scroll_speed: ScrollSpeedSetting::XMod(1.75),
            current_time_ns: time_ns,
            visible_beat: beat,
            search_beat: beat,
            scroll_reference_bpm: 180.0,
            music_rate: 1.0,
            edit_beat_spacing: false,
            draw_distance_after_targets: DRAW_AFTER,
            draw_distance_before_targets: DRAW_BEFORE,
            field_zoom: 1.0,
            elapsed_screen_s: elapsed,
            effect_height: 640.0,
            screen_height: 720.0,
            note_count_stats: &[],
            arrow_effect_time_s: elapsed,
            lane_tipsy: 0.0,
            lane_move_y: &[],
        })
    }

    fn bench_alpha_params() -> crate::NoteAlphaParams {
        crate::NoteAlphaParams {
            hidden: 0.35,
            hidden_offset: 0.1,
            sudden: 0.2,
            sudden_offset: -0.1,
            stealth: 0.0,
            blink: 0.0,
            random_vanish: 0.15,
        }
    }

    fn add_output(
        output: &mut PlacementBenchFrame,
        note_index: usize,
        adjusted: f32,
        x: f32,
        y: f32,
        alpha: f32,
        glow: f32,
    ) {
        let values = [
            note_index as u64,
            u64::from(adjusted.to_bits()),
            u64::from(x.to_bits()),
            u64::from(y.to_bits()),
            u64::from(alpha.to_bits()),
            u64::from(glow.to_bits()),
        ];
        for value in values {
            output.checksum = output.checksum.rotate_left(7) ^ value;
        }
        output.placements += 1;
    }

    #[cfg(test)]
    mod tests {
        use super::{PlacementBench, SONG_FRAMES};

        #[test]
        fn old_and_planned_frames_are_bit_exact_for_full_song_sweep() {
            let mut bench = PlacementBench::default();
            for frame in 0..SONG_FRAMES {
                let old = bench.old_frame(frame);
                let new = bench.new_frame(frame);
                assert_eq!(new, old, "frame {frame}");
            }
            assert_eq!(bench.capacity_growths(), 0);
        }
    }
}

#[cfg(feature = "bench-support")]
pub use bench::{PlacementBench, PlacementBenchFrame};

#[cfg(test)]
mod tests {
    use super::{NotefieldPlacementScratch, RANGE_GUARD_ROWS, expand_range, placement_capacity};
    use crate::{AccelYParams, ScrollTravelRequest, scroll_travel};
    use deadsync_core::note::NoteType;
    use deadsync_core::song_time::song_time_ns_add_seconds;
    use deadsync_rules::note::Note;
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::timing::{StopSegment, TimingData, TimingSegments};

    fn note(beat: f32) -> Note {
        Note {
            beat,
            row_index: 0,
            column: 0,
            note_type: NoteType::Tap,
            quantization_idx: 0,
            result: None,
            early_result: None,
            mine_result: None,
            hold: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    #[test]
    fn range_expansion_is_saturating() {
        assert_eq!(
            expand_range(Some((i32::MIN, i32::MAX))),
            Some((i32::MIN, i32::MAX))
        );
        assert_eq!(
            expand_range(Some((100, 200))),
            Some((100 - RANGE_GUARD_ROWS, 200 + RANGE_GUARD_ROWS))
        );
    }

    #[test]
    fn capacity_tracks_dense_window_and_caps_at_chart_size() {
        let sparse: Vec<_> = (0..300).map(|i| note(i as f32)).collect();
        assert!(placement_capacity(&sparse) < sparse.len());
        let dense: Vec<_> = (0..100).map(|i| note(i as f32 / 100.0)).collect();
        assert_eq!(placement_capacity(&dense), dense.len());
    }

    fn timing() -> TimingData {
        TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0), (8.0, 180.0), (16.0, 90.0)],
                stops: vec![StopSegment {
                    beat: 12.0,
                    duration: 0.25,
                }],
                ..TimingSegments::default()
            },
            &[],
        )
    }

    fn travel<'a>(
        timing: &'a TimingData,
        speed: ScrollSpeedSetting,
        accel: AccelYParams,
        time_ns: i64,
        elapsed_screen_s: f32,
    ) -> crate::ScrollTravel<'a> {
        let beat = timing.get_beat_for_time_ns(time_ns);
        scroll_travel(ScrollTravelRequest {
            timing,
            accel,
            scroll_speed: speed,
            current_time_ns: time_ns,
            visible_beat: beat,
            search_beat: beat,
            scroll_reference_bpm: 180.0,
            music_rate: 1.0,
            edit_beat_spacing: false,
            draw_distance_after_targets: 320.0,
            draw_distance_before_targets: 640.0,
            field_zoom: 1.0,
            elapsed_screen_s,
            effect_height: 640.0,
            screen_height: 720.0,
            note_count_stats: &[],
            arrow_effect_time_s: elapsed_screen_s,
            lane_tipsy: 0.0,
            lane_move_y: &[],
        })
    }

    fn assert_contains(actual: Option<(i32, i32)>, exact: Option<(i32, i32)>) {
        let (actual_low, actual_high) = actual.expect("planned range");
        let (exact_low, exact_high) = exact.expect("exact range");
        assert!(
            actual_low <= exact_low && actual_high >= exact_high,
            "planned {actual:?} does not contain exact {exact:?}"
        );
    }

    #[test]
    fn incremental_ranges_cover_full_solver_across_scroll_and_accel_modes() {
        let timing = timing();
        let modes = [
            (ScrollSpeedSetting::CMod(600.0), AccelYParams::default()),
            (ScrollSpeedSetting::XMod(2.0), AccelYParams::default()),
            (ScrollSpeedSetting::MMod(500.0), AccelYParams::default()),
            (
                ScrollSpeedSetting::XMod(1.5),
                AccelYParams {
                    boost: 0.35,
                    brake: 0.2,
                    wave: 0.25,
                    boomerang: 0.15,
                    expand: 0.0,
                },
            ),
        ];
        for (speed, accel) in modes {
            let mut scratch = NotefieldPlacementScratch::with_notes(&[]);
            let start = timing.get_time_for_beat_ns(2.0);
            for frame in 0..1_000 {
                let elapsed = frame as f32 / 120.0;
                let time_ns = song_time_ns_add_seconds(start, elapsed);
                let travel = travel(&timing, speed, accel, time_ns, elapsed);
                assert_contains(scratch.begin_frame(&travel), travel.visible_row_range());
            }
            let stats = scratch.stats();
            assert!(stats.incremental_ranges > stats.full_range_solves * 20);
        }
    }

    #[test]
    fn seeks_modifier_changes_and_expand_animation_force_full_solves() {
        let timing = timing();
        let mut scratch = NotefieldPlacementScratch::with_notes(&[]);
        let t4 = timing.get_time_for_beat_ns(4.0);
        let first = travel(
            &timing,
            ScrollSpeedSetting::XMod(1.0),
            AccelYParams::default(),
            t4,
            0.0,
        );
        scratch.begin_frame(&first);
        let forward = travel(
            &timing,
            ScrollSpeedSetting::XMod(1.0),
            AccelYParams::default(),
            song_time_ns_add_seconds(t4, 1.0 / 120.0),
            1.0 / 120.0,
        );
        scratch.begin_frame(&forward);
        assert_eq!(scratch.stats().incremental_ranges, 1);

        let changed = travel(
            &timing,
            ScrollSpeedSetting::MMod(500.0),
            AccelYParams::default(),
            song_time_ns_add_seconds(t4, 2.0 / 120.0),
            2.0 / 120.0,
        );
        scratch.begin_frame(&changed);
        let seek = travel(
            &timing,
            ScrollSpeedSetting::MMod(500.0),
            AccelYParams::default(),
            timing.get_time_for_beat_ns(1.0),
            0.0,
        );
        scratch.begin_frame(&seek);
        let expand = AccelYParams {
            expand: 1.0,
            ..AccelYParams::default()
        };
        scratch.begin_frame(&travel(
            &timing,
            ScrollSpeedSetting::MMod(500.0),
            expand,
            timing.get_time_for_beat_ns(1.01),
            0.1,
        ));
        scratch.begin_frame(&travel(
            &timing,
            ScrollSpeedSetting::MMod(500.0),
            expand,
            timing.get_time_for_beat_ns(1.02),
            0.2,
        ));
        assert_eq!(scratch.stats().full_range_solves, 5);
    }
}
