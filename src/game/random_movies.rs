use crate::config::{RandomBackgroundMode, dirs};
use deadsync_chart::{SongBackgroundChange, SongData, expand_random_background_changes};
use deadsync_rules::timing::{TimingData, TimingSegments};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const RANDOM_MOVIES_DIR: &str = "RandomMovies";
const BACKGROUND_MAPPING_FILE: &str = "BackgroundMapping.ini";

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
    expand_random_background_changes(song, timing, timing_segments, paths, seed_text.as_ref())
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
