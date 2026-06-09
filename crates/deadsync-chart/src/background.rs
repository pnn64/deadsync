use crate::song::{SongBackgroundChange, SongBackgroundChangeTarget, SongData};
use deadsync_core::timing::{ROWS_PER_BEAT, beat_to_note_row, note_row_to_beat};
use deadsync_rules::timing::{
    TimeSignatureSegment, TimingData, TimingSegments, default_time_signature,
};
use std::path::PathBuf;

const RANDOM_BG_CHANGE_MEASURES: i32 = 4;
const RANDOM_MOVIE_LIMIT: usize = 10;

#[derive(Clone, Debug)]
struct MovieCycle {
    paths: Vec<PathBuf>,
    next: usize,
}

impl MovieCycle {
    fn new(mut paths: Vec<PathBuf>, seed_text: &str) -> Self {
        shuffle_paths(&mut paths, crc32(seed_text.as_bytes()) as u64);
        paths.truncate(RANDOM_MOVIE_LIMIT);
        Self { paths, next: 0 }
    }

    fn next_path(&mut self) -> Option<PathBuf> {
        let path = self.paths.get(self.next)?.clone();
        self.next = (self.next + 1) % self.paths.len();
        Some(path)
    }
}

pub fn expand_random_background_changes(
    song: &SongData,
    timing: &TimingData,
    timing_segments: &TimingSegments,
    paths: Vec<PathBuf>,
    seed_text: &str,
) -> Vec<SongBackgroundChange> {
    if paths.is_empty() {
        return song.background_changes.clone();
    }
    let mut cycle = MovieCycle::new(paths, seed_text);
    let last_beat =
        timing.get_beat_for_time(song.precise_last_second().max(song.music_length_seconds));

    if song.background_changes.is_empty() {
        let mut out = Vec::new();
        let template = SongBackgroundChange::new(0.0, SongBackgroundChangeTarget::Random);
        push_random_segment(
            &mut out,
            0.0,
            last_beat,
            timing_segments,
            &mut cycle,
            &template,
        );
        push_static_song_background(song, last_beat, &mut out);
        sort_background_changes(&mut out);
        return out;
    }

    let mut out = Vec::with_capacity(song.background_changes.len());
    let mut expanded_random = false;
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
                    timing_segments,
                    &mut cycle,
                    change,
                );
                expanded_random = true;
            }
            _ => out.push(change.clone()),
        }
    }
    if !expanded_random {
        return song.background_changes.clone();
    }
    sort_background_changes(&mut out);
    out
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
    timing_segments: &TimingSegments,
    cycle: &mut MovieCycle,
    template: &SongBackgroundChange,
) {
    let start_row = beat_to_note_row(start_beat);
    let end_row = beat_to_note_row(end_beat);
    if end_row <= start_row {
        return;
    }
    let time_sigs = normalized_time_signatures(timing_segments);
    for (ix, sig) in time_sigs.iter().enumerate() {
        let sig_start_row = beat_to_note_row(sig.beat);
        let sig_end_row = time_sigs
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
            push_random_change(out, row, cycle, template);
            row += step_rows;
        }
    }

    for &(beat, _) in &timing_segments.bpms {
        let row = beat_to_note_row(beat);
        if row < start_row || row >= end_row || !row_starts_measure(row, &time_sigs) {
            continue;
        }
        push_random_change(out, row, cycle, template);
    }
}

fn push_random_change(
    out: &mut Vec<SongBackgroundChange>,
    row: i32,
    cycle: &mut MovieCycle,
    template: &SongBackgroundChange,
) {
    if out
        .iter()
        .any(|change| beat_to_note_row(change.start_beat) == row)
    {
        return;
    }
    let Some(path) = cycle.next_path() else {
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
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        (x >> 32) as u32
    }

    fn gen_range(&mut self, upper_exclusive: usize) -> usize {
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

    #[test]
    fn random_segments_change_every_four_measures_in_four_four() {
        let mut out = Vec::new();
        let mut cycle = MovieCycle::new(vec![path("a.avi"), path("b.avi")], "song");
        push_random_segment(
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
        let mut segments = TimingSegments::default();
        segments.time_signatures = vec![TimeSignatureSegment {
            beat: 0.0,
            numerator: 3,
            denominator: 4,
        }];
        let mut out = Vec::new();
        let mut cycle = MovieCycle::new(vec![path("a.avi"), path("b.avi")], "song");
        push_random_segment(
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
        let mut segments = TimingSegments::default();
        segments.bpms = vec![(0.0, 120.0), (8.0, 140.0), (16.0, 160.0)];
        let mut out = Vec::new();
        let mut cycle = MovieCycle::new(vec![path("a.avi"), path("b.avi")], "song");
        push_random_segment(
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
}
