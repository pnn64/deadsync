use crate::song::{SongBackgroundChange, SongBackgroundChangeTarget, SongData};
use deadsync_core::timing::{ROWS_PER_BEAT, beat_to_note_row, note_row_to_beat};
use deadsync_rules::timing::{
    TimeSignatureSegment, TimingData, TimingSegments, default_time_signature,
};
use rustc_hash::FxHashSet;
use std::path::PathBuf;

const RANDOM_BG_CHANGE_MEASURES: i32 = 4;
const RANDOM_MOVIE_LIMIT: usize = 10;
const FOUR_FOUR_CHANGE_ROWS: usize = (RANDOM_BG_CHANGE_MEASURES * 4 * ROWS_PER_BEAT) as usize;
const DENSE_ROW_WORDS: usize = 1_024;
const DENSE_ROW_CAPACITY: i64 = (DENSE_ROW_WORDS * u64::BITS as usize) as i64;

// Load-local scratch: the 8 KiB stack path covers 65,536 contiguous note rows
// without heap work. Unusually wide or malformed charts promote to the sparse set.
struct UsedRows {
    first: i32,
    dense: bool,
    bits: [u64; DENSE_ROW_WORDS],
    sparse: FxHashSet<i32>,
}

impl UsedRows {
    fn new(first: i32, last: i32, sparse_capacity: usize) -> Self {
        let span = i64::from(last) - i64::from(first) + 1;
        let dense = (1..=DENSE_ROW_CAPACITY).contains(&span);
        let sparse = if dense {
            FxHashSet::default()
        } else {
            FxHashSet::with_capacity_and_hasher(sparse_capacity, Default::default())
        };
        Self {
            first,
            dense,
            bits: [0; DENSE_ROW_WORDS],
            sparse,
        }
    }

    fn insert(&mut self, row: i32) -> bool {
        let offset = i64::from(row) - i64::from(self.first);
        if self.dense && !(0..DENSE_ROW_CAPACITY).contains(&offset) {
            self.promote_sparse();
        }
        if !self.dense {
            return self.sparse.insert(row);
        }
        let offset = offset as usize;
        let bit = 1_u64 << (offset % u64::BITS as usize);
        let word = &mut self.bits[offset / u64::BITS as usize];
        let inserted = *word & bit == 0;
        *word |= bit;
        inserted
    }

    fn remove(&mut self, row: i32) {
        if !self.dense {
            self.sparse.remove(&row);
            return;
        }
        let offset = (i64::from(row) - i64::from(self.first)) as usize;
        self.bits[offset / u64::BITS as usize] &= !(1_u64 << (offset % u64::BITS as usize));
    }

    fn promote_sparse(&mut self) {
        for (word_index, mut word) in self.bits.iter().copied().enumerate() {
            while word != 0 {
                let bit_index = word.trailing_zeros() as usize;
                let offset = word_index * u64::BITS as usize + bit_index;
                self.sparse
                    .insert((i64::from(self.first) + offset as i64) as i32);
                word &= word - 1;
            }
        }
        self.dense = false;
    }
}

struct RandomExpansion<'a> {
    timing_segments: &'a TimingSegments,
    time_sigs: &'a [TimeSignatureSegment],
    used_rows: &'a mut UsedRows,
    cycle: &'a mut MovieCycle,
}

#[derive(Clone, Debug)]
struct MovieCycle {
    paths: Vec<PathBuf>,
    next: usize,
}

impl MovieCycle {
    fn new(mut paths: Vec<PathBuf>, seed_text: &str) -> Self {
        shuffle_paths(&mut paths, u64::from(crc32(seed_text.as_bytes())));
        paths.truncate(RANDOM_MOVIE_LIMIT);
        Self { paths, next: 0 }
    }

    fn next_path(&mut self) -> Option<PathBuf> {
        let path = self.paths.get(self.next)?.clone();
        self.next = (self.next + 1) % self.paths.len();
        Some(path)
    }
}

#[must_use]
pub fn expand_random_background_changes(
    song: &SongData,
    timing: &TimingData,
    timing_segments: &TimingSegments,
    paths: Vec<PathBuf>,
    seed_text: &str,
) -> Vec<SongBackgroundChange> {
    if paths.is_empty() || random_expansion_unneeded(&song.background_changes) {
        return song.background_changes.clone();
    }
    let mut cycle = MovieCycle::new(paths, seed_text);
    let last_beat =
        timing.get_beat_for_time(song.precise_last_second().max(song.music_length_seconds));
    let time_sigs = normalized_time_signatures(timing_segments);
    let estimated_changes = song
        .background_changes
        .len()
        .saturating_add(timing_segments.bpms.len())
        .saturating_add(beat_to_note_row(last_beat).max(0) as usize / FOUR_FOUR_CHANGE_ROWS);
    let (first_row, last_row) = background_row_bounds(&song.background_changes, last_beat);
    let mut used_rows = UsedRows::new(first_row, last_row, estimated_changes);
    let mut expansion = RandomExpansion {
        timing_segments,
        time_sigs: &time_sigs,
        used_rows: &mut used_rows,
        cycle: &mut cycle,
    };

    if song.background_changes.is_empty() {
        let mut out = Vec::with_capacity(estimated_changes);
        let template = SongBackgroundChange::new(0.0, SongBackgroundChangeTarget::Random);
        push_random_segment(&mut out, 0.0, last_beat, &mut expansion, &template);
        push_static_song_background(song, last_beat, &mut out);
        sort_background_changes(&mut out);
        return out;
    }

    let mut out = Vec::with_capacity(estimated_changes);
    for (ix, change) in song.background_changes.iter().enumerate() {
        match change.target {
            SongBackgroundChangeTarget::Random => {
                let end_beat = song
                    .background_changes
                    .get(ix + 1)
                    .map(|next| next.start_beat)
                    .unwrap_or(last_beat);
                push_random_segment(
                    &mut out,
                    change.start_beat,
                    end_beat,
                    &mut expansion,
                    change,
                );
            }
            _ => {
                expansion
                    .used_rows
                    .insert(beat_to_note_row(change.start_beat));
                out.push(change.clone());
            }
        }
    }
    sort_background_changes(&mut out);
    out
}

fn random_expansion_unneeded(changes: &[SongBackgroundChange]) -> bool {
    !changes.is_empty()
        && !changes
            .iter()
            .any(|change| matches!(change.target, SongBackgroundChangeTarget::Random))
}

fn background_row_bounds(changes: &[SongBackgroundChange], last_beat: f32) -> (i32, i32) {
    let last_song_row = beat_to_note_row(last_beat);
    changes.iter().fold(
        (0.min(last_song_row), 0.max(last_song_row)),
        |(first, last), change| {
            let row = beat_to_note_row(change.start_beat);
            (first.min(row), last.max(row))
        },
    )
}

fn sort_background_changes(changes: &mut [SongBackgroundChange]) {
    changes.sort_by(|a, b| a.start_beat.total_cmp(&b.start_beat));
}

fn push_static_song_background(
    song: &SongData,
    start_beat: f32,
    out: &mut Vec<SongBackgroundChange>,
) {
    let target = match song.background_path.as_ref() {
        Some(path) => SongBackgroundChangeTarget::File(path.clone()),
        None => SongBackgroundChangeTarget::NoSongBg,
    };
    out.push(SongBackgroundChange::new(start_beat, target));
}

fn push_random_segment(
    out: &mut Vec<SongBackgroundChange>,
    start_beat: f32,
    end_beat: f32,
    expansion: &mut RandomExpansion<'_>,
    template: &SongBackgroundChange,
) {
    let start_row = beat_to_note_row(start_beat);
    let end_row = beat_to_note_row(end_beat);
    if end_row <= start_row {
        return;
    }
    for (ix, sig) in expansion.time_sigs.iter().enumerate() {
        let sig_start_row = beat_to_note_row(sig.beat);
        let sig_end_row = expansion
            .time_sigs
            .get(ix + 1)
            .map(|next| beat_to_note_row(next.beat))
            .unwrap_or(end_row);
        let first_row = sig_start_row.max(start_row);
        let last_row = sig_end_row.min(end_row);
        if first_row >= last_row {
            continue;
        }
        let step_rows = RANDOM_BG_CHANGE_MEASURES * note_rows_per_measure(*sig);
        if step_rows <= 0 {
            continue;
        }
        let mut row = first_row;
        while row < last_row {
            push_random_change(out, expansion.used_rows, row, expansion.cycle, template);
            row += step_rows;
        }
    }

    for &(beat, _) in &expansion.timing_segments.bpms {
        let row = beat_to_note_row(beat);
        if row < start_row || row >= end_row || !row_starts_measure(row, expansion.time_sigs) {
            continue;
        }
        push_random_change(out, expansion.used_rows, row, expansion.cycle, template);
    }
}

fn push_random_change(
    out: &mut Vec<SongBackgroundChange>,
    used_rows: &mut UsedRows,
    row: i32,
    cycle: &mut MovieCycle,
    template: &SongBackgroundChange,
) {
    if !used_rows.insert(row) {
        return;
    }
    let Some(path) = cycle.next_path() else {
        used_rows.remove(row);
        return;
    };
    let mut change = template.clone();
    change.start_beat = note_row_to_beat(row);
    change.target = SongBackgroundChangeTarget::File(path);
    out.push(change);
}

fn normalized_time_signatures(timing_segments: &TimingSegments) -> Vec<TimeSignatureSegment> {
    let mut sigs = timing_segments.time_signatures.clone();
    if sigs.is_empty() {
        sigs.push(default_time_signature());
    }
    sigs.sort_by(|a, b| a.beat.total_cmp(&b.beat));
    if sigs
        .first()
        .is_none_or(|sig| beat_to_note_row(sig.beat) > 0)
    {
        sigs.insert(0, default_time_signature());
    }
    sigs
}

fn row_starts_measure(row: i32, sigs: &[TimeSignatureSegment]) -> bool {
    sigs.iter().any(|sig| {
        let sig_row = beat_to_note_row(sig.beat);
        row >= sig_row && (row - sig_row) % note_rows_per_measure(*sig) == 0
    })
}

fn note_rows_per_measure(sig: TimeSignatureSegment) -> i32 {
    let numerator = sig.numerator.max(1) as f32;
    let denominator = sig.denominator.max(1) as f32;
    (ROWS_PER_BEAT as f32 * numerator * 4.0 / denominator)
        .round()
        .max(1.0) as i32
}

fn shuffle_paths(paths: &mut [PathBuf], seed: u64) {
    if paths.len() <= 1 {
        return;
    }
    let mut rng = XorShift64::new(seed);
    for ix in (1..paths.len()).rev() {
        let jx = rng.gen_range(ix + 1);
        paths.swap(ix, jx);
    }
}

#[derive(Clone, Copy, Debug)]
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    const fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    const fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        (x >> 32) as u32
    }

    const fn gen_range(&mut self, upper_exclusive: usize) -> usize {
        if upper_exclusive <= 1 {
            0
        } else {
            (self.next_u32() as usize) % upper_exclusive
        }
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> PathBuf {
        PathBuf::from(name)
    }

    fn random_template() -> SongBackgroundChange {
        SongBackgroundChange::new(0.0, SongBackgroundChangeTarget::Random)
    }

    fn push_segment(
        out: &mut Vec<SongBackgroundChange>,
        start_beat: f32,
        end_beat: f32,
        segments: &TimingSegments,
        cycle: &mut MovieCycle,
        template: &SongBackgroundChange,
    ) {
        let time_sigs = normalized_time_signatures(segments);
        let first_row = out
            .iter()
            .map(|change| beat_to_note_row(change.start_beat))
            .fold(beat_to_note_row(start_beat), i32::min);
        let last_row = out
            .iter()
            .map(|change| beat_to_note_row(change.start_beat))
            .fold(beat_to_note_row(end_beat), i32::max);
        let mut used_rows = UsedRows::new(first_row, last_row, out.len() + 16);
        for change in out.iter() {
            used_rows.insert(beat_to_note_row(change.start_beat));
        }
        let mut expansion = RandomExpansion {
            timing_segments: segments,
            time_sigs: &time_sigs,
            used_rows: &mut used_rows,
            cycle,
        };
        push_random_segment(out, start_beat, end_beat, &mut expansion, template);
    }

    #[test]
    fn random_segments_change_every_four_measures_in_four_four() {
        let mut out = Vec::new();
        let mut cycle = MovieCycle::new(vec![path("a.avi"), path("b.avi")], "song");
        push_segment(
            &mut out,
            0.0,
            64.0,
            &TimingSegments::default(),
            &mut cycle,
            &random_template(),
        );
        let beats = out
            .iter()
            .map(|change| change.start_beat)
            .collect::<Vec<_>>();
        assert_eq!(beats, vec![0.0, 16.0, 32.0, 48.0]);
    }

    #[test]
    fn random_segments_use_time_signature_measure_length() {
        let segments = TimingSegments {
            time_signatures: vec![TimeSignatureSegment {
                beat: 0.0,
                numerator: 3,
                denominator: 4,
            }],
            ..TimingSegments::default()
        };
        let mut out = Vec::new();
        let mut cycle = MovieCycle::new(vec![path("a.avi"), path("b.avi")], "song");
        push_segment(
            &mut out,
            0.0,
            48.0,
            &segments,
            &mut cycle,
            &random_template(),
        );
        let beats = out
            .iter()
            .map(|change| change.start_beat)
            .collect::<Vec<_>>();
        assert_eq!(beats, vec![0.0, 12.0, 24.0, 36.0]);
    }

    #[test]
    fn random_segments_add_measure_start_bpm_changes_once() {
        let segments = TimingSegments {
            bpms: vec![(0.0, 120.0), (8.0, 140.0), (16.0, 160.0)],
            ..TimingSegments::default()
        };
        let mut out = Vec::new();
        let mut cycle = MovieCycle::new(vec![path("a.avi"), path("b.avi")], "song");
        push_segment(
            &mut out,
            0.0,
            32.0,
            &segments,
            &mut cycle,
            &random_template(),
        );
        sort_background_changes(&mut out);
        let beats = out
            .iter()
            .map(|change| change.start_beat)
            .collect::<Vec<_>>();
        assert_eq!(beats, vec![0.0, 8.0, 16.0]);
    }

    #[test]
    fn crc32_matches_itg_hash_for_string() {
        assert_eq!(crc32(b"RandomMovies"), 0x67B4_79F8);
    }

    #[test]
    fn non_random_changes_skip_expansion_work() {
        let changes = [
            SongBackgroundChange::new(0.0, SongBackgroundChangeTarget::NoSongBg),
            SongBackgroundChange::new(
                16.0,
                SongBackgroundChangeTarget::File(path("background.png")),
            ),
        ];
        assert!(random_expansion_unneeded(&changes));
        assert!(!random_expansion_unneeded(&[]));
        assert!(!random_expansion_unneeded(&[SongBackgroundChange::new(
            0.0,
            SongBackgroundChangeTarget::Random
        ),]));
    }

    #[test]
    fn row_tracker_preserves_dense_and_sparse_membership() {
        for mut rows in [
            UsedRows::new(-64, 4_096, 8),
            UsedRows::new(i32::MIN, i32::MAX, 8),
        ] {
            assert!(rows.insert(-32));
            assert!(!rows.insert(-32));
            assert!(rows.insert(2_048));
            rows.remove(-32);
            assert!(rows.insert(-32));
        }
    }
}
