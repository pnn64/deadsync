use deadsync_chart::{SongData, SongPack};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::course::CourseFile;

pub type CourseData = (PathBuf, CourseFile);

static SONG_CACHE: std::sync::LazyLock<Mutex<Vec<SongPack>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));
static SONG_CACHE_GENERATION: AtomicU64 = AtomicU64::new(1);

static COURSE_CACHE: std::sync::LazyLock<Mutex<Vec<CourseData>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

/// Provides safe, read-only access to the global song cache.
pub fn get_song_cache() -> std::sync::MutexGuard<'static, Vec<SongPack>> {
    SONG_CACHE.lock().unwrap()
}

pub fn song_cache_generation() -> u64 {
    SONG_CACHE_GENERATION.load(Ordering::Relaxed)
}

/// A public function to allow the parser to populate the cache.
pub fn set_song_cache(packs: Vec<SongPack>) {
    let mut cache = SONG_CACHE.lock().unwrap();
    *cache = packs;
    SONG_CACHE_GENERATION.fetch_add(1, Ordering::Relaxed);
}

pub fn reload_song_in_cache_with<F>(
    simfile_path: &std::path::Path,
    parse_song: F,
) -> Result<Arc<SongData>, String>
where
    F: FnOnce(&std::path::Path) -> Result<SongData, String>,
{
    let updated = Arc::new(parse_song(simfile_path)?);
    let mut song_cache = get_song_cache();
    let mut replaced = false;
    for pack in song_cache.iter_mut() {
        for song in &mut pack.songs {
            if song.simfile_path == simfile_path {
                *song = updated.clone();
                replaced = true;
            }
        }
    }
    if !replaced {
        return Err(format!(
            "Song '{}' not found in song cache",
            simfile_path.display()
        ));
    }
    Ok(updated)
}

pub fn get_course_cache() -> std::sync::MutexGuard<'static, Vec<CourseData>> {
    COURSE_CACHE.lock().unwrap()
}

pub fn set_course_cache(courses: Vec<CourseData>) {
    *COURSE_CACHE.lock().unwrap() = courses;
}
