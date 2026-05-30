use crate::config::{RandomBackgroundMode, dirs};
use deadsync_chart::{SongBackgroundChange, SongBackgroundChangeTarget, SongData};
use deadsync_rules::timing::{
    ROWS_PER_BEAT, TimeSignatureSegment, TimingData, TimingSegments, beat_to_note_row,
    default_time_signature, note_row_to_beat,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const RANDOM_MOVIES_DIR: &str = "RandomMovies";
const BACKGROUND_MAPPING_FILE: &str = "BackgroundMapping.ini";
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

pub fn build_background_changes(
    song: &SongData,
    timing: &TimingData,
    timing_segments: &TimingSegments,
    mode: RandomBackgroundMode,
) -> Vec<SongBackgroundChange> {
    if mode != RandomBackgroundMode::RandomMovies {
        return song.background_changes.clone();
    }
    let paths = random_movie_paths_for_song(song);
    if paths.is_empty() {
        return song.background_changes.clone();
    }
    let seed_text = song
        .simfile_path
        .parent()
        .map(|path| path.to_string_lossy())
        .unwrap_or_else(|| song.simfile_path.to_string_lossy());
    let mut cycle = MovieCycle::new(paths, seed_text.as_ref());
    let last_beat =
        timing.get_beat_for_time(song.precise_last_second().max(song.music_length_seconds));

    if song.background_changes.is_empty() {
        let mut out = Vec::new();
        push_random_segment(&mut out, 0.0, last_beat, timing_segments, &mut cycle);
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
    out.push(SongBackgroundChange { start_beat, target });
}

fn push_random_segment(
    out: &mut Vec<SongBackgroundChange>,
    start_beat: f32,
    end_beat: f32,
    timing_segments: &TimingSegments,
    cycle: &mut MovieCycle,
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
            push_random_change(out, row, cycle);
            row += step_rows;
        }
    }

    for &(beat, _) in &timing_segments.bpms {
        let row = beat_to_note_row(beat);
        if row < start_row || row >= end_row || !row_starts_measure(row, &time_sigs) {
            continue;
        }
        push_random_change(out, row, cycle);
    }
}

fn push_random_change(out: &mut Vec<SongBackgroundChange>, row: i32, cycle: &mut MovieCycle) {
    if out
        .iter()
        .any(|change| beat_to_note_row(change.start_beat) == row)
    {
        return;
    }
    let Some(path) = cycle.next_path() else {
        return;
    };
    out.push(SongBackgroundChange {
        start_beat: note_row_to_beat(row),
        target: SongBackgroundChangeTarget::File(path),
    });
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

fn random_movie_paths_for_song(song: &SongData) -> Vec<PathBuf> {
    let group = song_group_name(song);
    let genre_whitelist = group
        .as_deref()
        .filter(|_| !song.genre.trim().is_empty())
        .and_then(|group| {
            random_movie_roots()
                .into_iter()
                .find_map(|root| genre_movie_whitelist(&root.join(group), &song.genre))
        });

    for root in random_movie_roots() {
        if let Some(group) = group.as_deref() {
            let paths = filtered_movie_paths(&root.join(group), genre_whitelist.as_ref());
            if !paths.is_empty() {
                return paths;
            }
        }
        let paths = filtered_movie_paths(&root, genre_whitelist.as_ref());
        if !paths.is_empty() {
            return paths;
        }
    }
    Vec::new()
}

fn filtered_movie_paths(dir: &Path, whitelist: Option<&HashSet<String>>) -> Vec<PathBuf> {
    let paths = list_movie_paths(dir);
    let Some(whitelist) = whitelist else {
        return paths;
    };
    let filtered = paths
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| whitelist.contains(name))
        })
        .cloned()
        .collect::<Vec<_>>();
    if filtered.is_empty() { paths } else { filtered }
}

fn random_movie_roots() -> Vec<PathBuf> {
    let dirs = dirs::app_dirs();
    let mut roots = Vec::with_capacity(4);
    push_unique_dir(&mut roots, dirs.data_dir.join(RANDOM_MOVIES_DIR));
    push_unique_dir(&mut roots, dirs.exe_dir.join(RANDOM_MOVIES_DIR));
    if let Ok(cwd) = std::env::current_dir() {
        push_unique_dir(&mut roots, cwd.join(RANDOM_MOVIES_DIR));
        push_unique_dir(&mut roots, cwd.join("deadsync").join(RANDOM_MOVIES_DIR));
    }
    roots
}

fn push_unique_dir(out: &mut Vec<PathBuf>, path: PathBuf) {
    if !path.is_dir() || out.iter().any(|existing| existing == &path) {
        return;
    }
    out.push(path);
}

fn list_movie_paths(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && !path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("._"))
                && is_random_movie_path(path)
        })
        .collect::<Vec<_>>();
    paths.sort_by(|a, b| {
        a.file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .cmp(
                &b.file_name()
                    .map(|name| name.to_string_lossy().to_ascii_lowercase()),
            )
    });
    paths
}

fn is_random_movie_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "ogv"
                    | "avi"
                    | "f4v"
                    | "flv"
                    | "mpg"
                    | "mpeg"
                    | "mp4"
                    | "m4v"
                    | "mov"
                    | "webm"
                    | "mkv"
                    | "wmv"
            )
        })
}

fn song_group_name(song: &SongData) -> Option<String> {
    song.simfile_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
}

fn genre_movie_whitelist(group_dir: &Path, genre: &str) -> Option<HashSet<String>> {
    let path = group_dir.join(BACKGROUND_MAPPING_FILE);
    let sections = parse_ini_sections(&fs::read_to_string(path).ok()?);
    let genre_section = sections
        .get("GenreToSection")?
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(genre.trim()))?
        .1
        .trim()
        .to_owned();
    let section = sections.get(genre_section.as_str())?;
    let out = section
        .iter()
        .map(|(key, _)| key.trim().to_owned())
        .filter(|key| !key.is_empty())
        .collect::<HashSet<_>>();
    (!out.is_empty()).then_some(out)
}

fn parse_ini_sections(text: &str) -> HashMap<String, Vec<(String, String)>> {
    let mut sections = HashMap::<String, Vec<(String, String)>>::new();
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            current = line[1..line.len() - 1].trim().to_owned();
            sections.entry(current.clone()).or_default();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        sections
            .entry(current.clone())
            .or_default()
            .push((key.trim().to_owned(), value.trim().to_owned()));
    }
    sections
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

    #[test]
    fn random_segments_change_every_four_measures_in_four_four() {
        let mut out = Vec::new();
        let mut cycle = MovieCycle::new(vec![path("a.avi"), path("b.avi")], "song");
        push_random_segment(&mut out, 0.0, 64.0, &TimingSegments::default(), &mut cycle);
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
        push_random_segment(&mut out, 0.0, 48.0, &segments, &mut cycle);
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
        push_random_segment(&mut out, 0.0, 32.0, &segments, &mut cycle);
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
