pub mod app_runtime;
pub mod artwork;
pub mod bgchanges;
pub mod bpm;
pub mod cache;
pub mod changes;
pub mod course;
pub mod event_intro;
pub mod matrix;
pub mod media;
pub mod playlist;
pub mod runtime;
pub mod runtime_cache;
pub mod scan;
pub mod song;
pub mod song_search;
pub mod song_sort;
pub mod stats;
pub mod sync_offset;
pub mod tags;
pub mod timing;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[cfg(test)]
#[path = "../tests/perf/processing.rs"]
mod processing_perf;
