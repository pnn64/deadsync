use deadsync_chart::notes::ParsedNote;
use deadsync_chart::{
    ArrowStats, ChartData, SongData, SongPack, StaminaCounts, SyncPref, TechCounts,
};
use deadsync_core::note::NoteType;
use deadsync_simfile::bgchanges::{
    bgchange_field_rejects_non_media, bgchange_field_rejects_non_media_legacy,
    parse_bgchange_color, parse_bgchange_color_legacy,
};
use deadsync_simfile::cache::{
    SerializableSongForegroundChange, SerializableSongForegroundLuaChange,
};
use deadsync_simfile::changes::{
    extract_foreground_change_sets, extract_foreground_changes, extract_foreground_lua_changes,
    simfile_uses_lua,
};
use deadsync_simfile::media::{
    foreground_media_ext_rank, foreground_media_ext_rank_legacy, is_bgchange_movie_path,
    is_bgchange_movie_path_legacy, is_song_art_image, is_song_art_image_legacy,
};
use deadsync_simfile::notes::{
    parse_chart_notes, parse_chart_notes_legacy, step_type_lanes, step_type_lanes_legacy,
};
use deadsync_simfile::scan::{
    sort_song_packs_for_bench, sort_song_packs_legacy, sort_songs_itgmania_for_bench,
    sort_songs_itgmania_legacy,
};
use deadsync_simfile::song_search::{
    SongSearchCandidate, SongSearchCatalogEntry, build_song_search_candidates,
    build_song_search_candidates_legacy, sort_song_search_candidates_for_bench,
    sort_song_search_candidates_legacy,
};
use deadsync_simfile::song_sort::{
    GroupedSongs, song_meters_for_sort, song_meters_for_sort_legacy, title_grouped_songs,
    title_grouped_songs_legacy,
};
use deadsync_simfile::tags::{latest_simfile_tag_value_legacy, latest_simfile_tag_values};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SONGS: usize = 8_192;
const TAG_BYTES: usize = 512 * 1024;
const TAG_ITERATIONS: usize = 256;
const SEARCH_ITERATIONS: usize = 64;
const SORT_ITERATIONS: usize = 12;
const NOTE_ROWS: usize = 65_536;
const NOTE_ITERATIONS: usize = 128;
const METER_ITERATIONS: usize = 64;
const FOREGROUND_ITERATIONS: usize = 64;
const STEP_TYPE_CALLS: usize = 65_536;
const STEP_TYPE_ITERATIONS: usize = 128;
const MEDIA_PATHS: usize = 32_768;
const MEDIA_ITERATIONS: usize = 128;
const SEARCH_SORT_ITERATIONS: usize = 32;
const CATALOG_SORT_ITERATIONS: usize = 24;
const PACKS: usize = 4_096;
const PACK_SORT_ITERATIONS: usize = 48;
const BGCHANGE_FIELD_CALLS: usize = 65_536;
const BGCHANGE_FIELD_ITERATIONS: usize = 64;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates to `System` with the caller-provided
// pointer and layout; the independent atomics only observe successful calls.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged to the system allocator.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocs.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller provides the allocation's original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    deallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let simfile = benchmark_simfile();
    benchmark_tag_batch(&simfile);

    benchmark_step_type_matching();
    benchmark_bgchange_fields();

    let media_paths = benchmark_media_paths();
    benchmark_media_extensions(&media_paths);

    let note_data = benchmark_note_data();
    benchmark_note_parsing(&note_data);

    let foreground = ForegroundFixture::new();
    benchmark_foreground_changes(&foreground);

    let songs: Vec<_> = (0..SONGS).map(benchmark_song).collect();
    benchmark_catalog_song_sort(&songs);
    benchmark_search(&songs);
    benchmark_search_sort(&songs);
    benchmark_sort(&songs);
    benchmark_meters(&songs);

    let packs: Vec<_> = (0..PACKS).map(benchmark_pack).collect();
    benchmark_pack_sort(&packs);
}

fn benchmark_tag_batch(simfile: &[u8]) {
    let old_values = [
        latest_simfile_tag_value_legacy(simfile, b"#CDIMAGE:"),
        latest_simfile_tag_value_legacy(simfile, b"#DISCIMAGE:"),
    ];
    let new_values = latest_simfile_tag_values(
        simfile,
        [b"#CDIMAGE:".as_slice(), b"#DISCIMAGE:".as_slice()],
    );
    assert_eq!(old_values, new_values, "tag extraction output changed");

    let old = measure(TAG_ITERATIONS, || {
        let cdimage = latest_simfile_tag_value_legacy(black_box(simfile), b"#CDIMAGE:");
        let discimage = latest_simfile_tag_value_legacy(black_box(simfile), b"#DISCIMAGE:");
        text_checksum(&cdimage) ^ text_checksum(&discimage).rotate_left(17)
    });
    let new = measure(TAG_ITERATIONS, || {
        let [cdimage, discimage] = latest_simfile_tag_values(
            black_box(simfile),
            [b"#CDIMAGE:".as_slice(), b"#DISCIMAGE:".as_slice()],
        );
        text_checksum(&cdimage) ^ text_checksum(&discimage).rotate_left(17)
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "batched artwork tags",
        simfile.len(),
        TAG_ITERATIONS,
        "bytes",
        &old,
        &new,
    );
}

fn benchmark_step_type_matching() {
    const STEP_TYPES: [&str; 8] = [
        "dance-single",
        " DANCE-DOUBLE ",
        "dance_double",
        "Dance_Double",
        "pump-double",
        "dance__double",
        "lights-cabinet",
        "",
    ];
    for step_type in STEP_TYPES {
        assert_eq!(
            step_type_lanes_legacy(step_type),
            step_type_lanes(step_type),
            "step-type lane count changed"
        );
    }

    let old = measure(STEP_TYPE_ITERATIONS, || {
        (0..STEP_TYPE_CALLS).fold(0_u64, |checksum, index| {
            let step_type = black_box(STEP_TYPES[index % STEP_TYPES.len()]);
            checksum.rotate_left(3) ^ step_type_lanes_legacy(step_type) as u64
        })
    });
    let new = measure(STEP_TYPE_ITERATIONS, || {
        (0..STEP_TYPE_CALLS).fold(0_u64, |checksum, index| {
            let step_type = black_box(STEP_TYPES[index % STEP_TYPES.len()]);
            checksum.rotate_left(3) ^ step_type_lanes(step_type) as u64
        })
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "allocation-free step-type matching",
        STEP_TYPE_CALLS,
        STEP_TYPE_ITERATIONS,
        "types",
        &old,
        &new,
    );
}

fn benchmark_bgchange_fields() {
    const TARGETS: [&str; 8] = [
        "movie.mp4",
        "Theme/Default.XML",
        "config.ini",
        "animation.PNG",
        "visual.INI.backup",
        "script.lua",
        "folder.with.dots/clip.webm",
        "",
    ];
    const COLORS: [&str; 8] = [
        "1,0.5,0",
        "1^0.5^0^0.75",
        "#FF8000",
        "#10203040",
        " 0.1, 0.2, 0.3, 0.4 ",
        "1,,0.5,,0",
        "invalid",
        "1,2,3,4,5",
    ];
    for index in 0..TARGETS.len() {
        assert_eq!(
            bgchange_field_rejects_non_media_legacy(TARGETS[index]),
            bgchange_field_rejects_non_media(TARGETS[index]),
            "BG-change target validation changed"
        );
        assert_eq!(
            parse_bgchange_color_legacy(COLORS[index]),
            parse_bgchange_color(COLORS[index]),
            "BG-change color parsing changed"
        );
    }

    let old = measure(BGCHANGE_FIELD_ITERATIONS, || {
        (0..BGCHANGE_FIELD_CALLS).fold(0_u64, |checksum, index| {
            let target = black_box(TARGETS[index % TARGETS.len()]);
            let color = black_box(COLORS[index % COLORS.len()]);
            checksum.rotate_left(5)
                ^ u64::from(bgchange_field_rejects_non_media_legacy(target))
                ^ color_checksum(parse_bgchange_color_legacy(color)).rotate_left(17)
        })
    });
    let new = measure(BGCHANGE_FIELD_ITERATIONS, || {
        (0..BGCHANGE_FIELD_CALLS).fold(0_u64, |checksum, index| {
            let target = black_box(TARGETS[index % TARGETS.len()]);
            let color = black_box(COLORS[index % COLORS.len()]);
            checksum.rotate_left(5)
                ^ u64::from(bgchange_field_rejects_non_media(target))
                ^ color_checksum(parse_bgchange_color(color)).rotate_left(17)
        })
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "allocation-free BG-change fields",
        BGCHANGE_FIELD_CALLS * 2,
        BGCHANGE_FIELD_ITERATIONS,
        "fields",
        &old,
        &new,
    );
}

fn benchmark_media_extensions(paths: &[PathBuf]) {
    for path in paths {
        assert_eq!(
            media_classification_legacy(path),
            media_classification(path),
            "media extension classification changed"
        );
    }

    let old = measure(MEDIA_ITERATIONS, || {
        black_box(paths).iter().fold(0_u64, |checksum, path| {
            checksum.rotate_left(5) ^ media_classification_legacy(path)
        })
    });
    let new = measure(MEDIA_ITERATIONS, || {
        black_box(paths).iter().fold(0_u64, |checksum, path| {
            checksum.rotate_left(5) ^ media_classification(path)
        })
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "allocation-free media extension checks",
        paths.len() * 3,
        MEDIA_ITERATIONS,
        "checks",
        &old,
        &new,
    );
}

fn benchmark_note_parsing(note_data: &[u8]) {
    let old_notes = parse_chart_notes_legacy(note_data, 4);
    let new_notes = parse_chart_notes(note_data, 4);
    assert_eq!(old_notes, new_notes, "parsed notes changed");

    let old = measure(NOTE_ITERATIONS, || {
        note_checksum(&parse_chart_notes_legacy(black_box(note_data), 4))
    });
    let new = measure(NOTE_ITERATIONS, || {
        note_checksum(&parse_chart_notes(black_box(note_data), 4))
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "pre-sized parsed-note buffer",
        NOTE_ROWS,
        NOTE_ITERATIONS,
        "rows",
        &old,
        &new,
    );
}

fn benchmark_foreground_changes(fixture: &ForegroundFixture) {
    let old_media = extract_foreground_changes(&fixture.song_dir, &fixture.simfile);
    let old_lua = extract_foreground_lua_changes(&fixture.song_dir, &fixture.simfile);
    let old_has_lua = simfile_uses_lua(&fixture.song_dir, &fixture.simfile, "");
    let new_changes = extract_foreground_change_sets(&fixture.song_dir, &fixture.simfile);
    assert_eq!(
        foreground_media_values(&old_media),
        foreground_media_values(&new_changes.media),
        "foreground media changes changed"
    );
    assert_eq!(
        foreground_lua_values(&old_lua),
        foreground_lua_values(&new_changes.lua),
        "foreground Lua changes changed"
    );
    assert_eq!(
        old_has_lua, new_changes.uses_lua,
        "foreground Lua detection changed"
    );

    let old = measure(FOREGROUND_ITERATIONS, || {
        let media =
            extract_foreground_changes(black_box(&fixture.song_dir), black_box(&fixture.simfile));
        let lua = extract_foreground_lua_changes(
            black_box(&fixture.song_dir),
            black_box(&fixture.simfile),
        );
        let has_lua = simfile_uses_lua(
            black_box(&fixture.song_dir),
            black_box(&fixture.simfile),
            "",
        );
        foreground_checksum(&media, &lua, has_lua)
    });
    let new = measure(FOREGROUND_ITERATIONS, || {
        let changes = extract_foreground_change_sets(
            black_box(&fixture.song_dir),
            black_box(&fixture.simfile),
        );
        let has_lua = changes.uses_lua;
        foreground_checksum(&changes.media, &changes.lua, has_lua)
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "combined foreground extraction",
        fixture.simfile.len(),
        FOREGROUND_ITERATIONS,
        "bytes",
        &old,
        &new,
    );
}

fn benchmark_search(songs: &[Arc<SongData>]) {
    const QUERY: &str = "PERFORMANCE PACK/song 04";
    let old_candidates =
        build_song_search_candidates_legacy(search_entries(songs), QUERY, "dance-single");
    let new_candidates = build_song_search_candidates(search_entries(songs), QUERY, "dance-single");
    assert_eq!(
        candidate_paths(&old_candidates),
        candidate_paths(&new_candidates),
        "search candidates changed"
    );

    let old = measure(SEARCH_ITERATIONS, || {
        candidate_checksum(&build_song_search_candidates_legacy(
            search_entries(black_box(songs)),
            QUERY,
            "dance-single",
        ))
    });
    let new = measure(SEARCH_ITERATIONS, || {
        candidate_checksum(&build_song_search_candidates(
            search_entries(black_box(songs)),
            QUERY,
            "dance-single",
        ))
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "allocation-free search matching",
        songs.len(),
        SEARCH_ITERATIONS,
        "songs",
        &old,
        &new,
    );
}

fn benchmark_catalog_song_sort(songs: &[Arc<SongData>]) {
    let mut old_songs = songs.to_vec();
    old_songs.reverse();
    sort_songs_itgmania_legacy(&mut old_songs);
    let mut new_songs = songs.to_vec();
    new_songs.reverse();
    sort_songs_itgmania_for_bench(&mut new_songs);
    assert_eq!(
        song_paths(&old_songs),
        song_paths(&new_songs),
        "catalog song sort order changed"
    );

    let mut old_songs = songs.to_vec();
    let old = measure(CATALOG_SORT_ITERATIONS, || {
        old_songs.reverse();
        sort_songs_itgmania_legacy(black_box(&mut old_songs));
        song_checksum(&old_songs)
    });
    let mut new_songs = songs.to_vec();
    let new = measure(CATALOG_SORT_ITERATIONS, || {
        new_songs.reverse();
        sort_songs_itgmania_for_bench(black_box(&mut new_songs));
        song_checksum(&new_songs)
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "borrowed catalog song sort keys",
        songs.len(),
        CATALOG_SORT_ITERATIONS,
        "songs",
        &old,
        &new,
    );
}

fn benchmark_search_sort(songs: &[Arc<SongData>]) {
    let candidates = build_song_search_candidates(search_entries(songs), "", "dance-single");
    let mut old_candidates = candidates.clone();
    old_candidates.reverse();
    sort_song_search_candidates_legacy(&mut old_candidates);
    let mut new_candidates = candidates.clone();
    new_candidates.reverse();
    sort_song_search_candidates_for_bench(&mut new_candidates);
    assert_eq!(
        candidate_paths(&old_candidates),
        candidate_paths(&new_candidates),
        "search result sort order changed"
    );

    let mut old_candidates = candidates.clone();
    let old = measure(SEARCH_SORT_ITERATIONS, || {
        old_candidates.reverse();
        sort_song_search_candidates_legacy(black_box(&mut old_candidates));
        candidate_checksum(&old_candidates)
    });
    let mut new_candidates = candidates;
    let new = measure(SEARCH_SORT_ITERATIONS, || {
        new_candidates.reverse();
        sort_song_search_candidates_for_bench(black_box(&mut new_candidates));
        candidate_checksum(&new_candidates)
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "allocation-free search sort keys",
        songs.len(),
        SEARCH_SORT_ITERATIONS,
        "songs",
        &old,
        &new,
    );
}

fn benchmark_sort(songs: &[Arc<SongData>]) {
    let old_groups = title_grouped_songs_legacy(songs.to_vec());
    let new_groups = title_grouped_songs(songs.to_vec());
    assert_eq!(
        grouped_paths(&old_groups),
        grouped_paths(&new_groups),
        "title sort order changed"
    );

    let old = measure(SORT_ITERATIONS, || {
        grouped_checksum(&title_grouped_songs_legacy(black_box(songs).to_vec()))
    });
    let new = measure(SORT_ITERATIONS, || {
        grouped_checksum(&title_grouped_songs(black_box(songs).to_vec()))
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "allocation-free title sort",
        songs.len(),
        SORT_ITERATIONS,
        "songs",
        &old,
        &new,
    );
}

fn benchmark_meters(songs: &[Arc<SongData>]) {
    for song in songs {
        assert_eq!(
            song_meters_for_sort_legacy(song, "dance-single"),
            song_meters_for_sort(song, "dance-single"),
            "song meter values changed"
        );
    }

    let old = measure(METER_ITERATIONS, || {
        black_box(songs).iter().fold(0_u64, |checksum, song| {
            meter_checksum(checksum, &song_meters_for_sort_legacy(song, "dance-single"))
        })
    });
    let new = measure(METER_ITERATIONS, || {
        black_box(songs).iter().fold(0_u64, |checksum, song| {
            meter_checksum(checksum, &song_meters_for_sort(song, "dance-single"))
        })
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "single-vector meter collection",
        songs.len(),
        METER_ITERATIONS,
        "songs",
        &old,
        &new,
    );
}

fn benchmark_pack_sort(packs: &[SongPack]) {
    let mut old_packs = packs.to_vec();
    old_packs.reverse();
    sort_song_packs_legacy(&mut old_packs);
    let mut new_packs = packs.to_vec();
    new_packs.reverse();
    sort_song_packs_for_bench(&mut new_packs);
    assert_eq!(
        pack_names(&old_packs),
        pack_names(&new_packs),
        "pack sort order changed"
    );

    let mut old_packs = packs.to_vec();
    let old = measure(PACK_SORT_ITERATIONS, || {
        old_packs.reverse();
        sort_song_packs_legacy(black_box(&mut old_packs));
        pack_checksum(&old_packs)
    });
    let mut new_packs = packs.to_vec();
    let new = measure(PACK_SORT_ITERATIONS, || {
        new_packs.reverse();
        sort_song_packs_for_bench(black_box(&mut new_packs));
        pack_checksum(&new_packs)
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "borrowed pack sort keys",
        packs.len(),
        PACK_SORT_ITERATIONS,
        "packs",
        &old,
        &new,
    );
}

fn search_entries(songs: &[Arc<SongData>]) -> impl Iterator<Item = SongSearchCatalogEntry<'_>> {
    std::iter::once(SongSearchCatalogEntry::PackHeader("Performance Pack"))
        .chain(songs.iter().map(SongSearchCatalogEntry::Song))
}

fn measure(iterations: usize, mut work: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..2 {
        black_box(work());
    }
    let alloc_before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for iteration in 0..iterations {
        checksum = checksum.rotate_left(7) ^ black_box(work()) ^ iteration as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(alloc_before),
        checksum,
    }
}

fn print_pair(
    name: &str,
    units_per_iteration: usize,
    iterations: usize,
    unit: &str,
    old: &BenchResult,
    new: &BenchResult,
) {
    println!("\n{name} ({units_per_iteration} {unit}/operation, {iterations} operations)");
    print_result("old", units_per_iteration, iterations, unit, old);
    print_result("new", units_per_iteration, iterations, unit, new);
    println!(
        "  speedup {:>6.2}x | cycles reduction {:>6.1}% | allocations reduction {:>6.1}% | bytes reduction {:>6.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(old.alloc.allocs, new.alloc.allocs),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn print_result(
    label: &str,
    units_per_iteration: usize,
    iterations: usize,
    unit: &str,
    result: &BenchResult,
) {
    let operations = iterations as f64;
    let units = units_per_iteration as f64 * operations;
    println!(
        "  {label:<4} {:>9.2} ms/op {:>12.0} {unit}/s {:>9.1} cycles/{unit}",
        result.elapsed.as_secs_f64() * 1_000.0 / operations,
        units / result.elapsed.as_secs_f64(),
        result.cycles as f64 / units,
    );
    println!(
        "       alloc/realloc/free={:.1}/{:.1}/{:.1} per op, {:.1} KiB allocated/op",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.deallocs as f64 / operations,
        result.alloc.bytes as f64 / operations / 1024.0,
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        return 0.0;
    }
    100.0 * (1.0 - new as f64 / old as f64)
}

fn candidate_paths(candidates: &[SongSearchCandidate]) -> Vec<&std::path::Path> {
    candidates
        .iter()
        .map(|candidate| candidate.song.simfile_path.as_path())
        .collect()
}

fn song_paths(songs: &[Arc<SongData>]) -> Vec<&Path> {
    songs
        .iter()
        .map(|song| song.simfile_path.as_path())
        .collect()
}

fn pack_names(packs: &[SongPack]) -> Vec<&str> {
    packs.iter().map(|pack| pack.group_name.as_str()).collect()
}

fn grouped_paths(groups: &[GroupedSongs]) -> Vec<&std::path::Path> {
    groups
        .iter()
        .flat_map(|group| &group.songs)
        .map(|song| song.simfile_path.as_path())
        .collect()
}

fn candidate_checksum(candidates: &[SongSearchCandidate]) -> u64 {
    candidates.iter().fold(0_u64, |checksum, candidate| {
        checksum.rotate_left(9)
            ^ text_checksum(&candidate.pack_name)
            ^ text_checksum(&candidate.song.title).rotate_left(23)
    })
}

fn song_checksum(songs: &[Arc<SongData>]) -> u64 {
    songs.iter().fold(0_u64, |checksum, song| {
        checksum.rotate_left(7)
            ^ text_checksum(&song.title)
            ^ path_checksum(&song.simfile_path).rotate_left(19)
    })
}

fn pack_checksum(packs: &[SongPack]) -> u64 {
    packs.iter().fold(0_u64, |checksum, pack| {
        checksum.rotate_left(11)
            ^ text_checksum(&pack.sort_title)
            ^ text_checksum(&pack.group_name).rotate_left(23)
    })
}

fn grouped_checksum(groups: &[GroupedSongs]) -> u64 {
    groups.iter().fold(0_u64, |checksum, group| {
        group
            .songs
            .iter()
            .fold(checksum.rotate_left(3), |sum, song| {
                sum.rotate_left(11) ^ text_checksum(&song.title)
            })
    })
}

fn note_checksum(notes: &[ParsedNote]) -> u64 {
    notes.iter().fold(0_u64, |checksum, note| {
        let note_type = match note.note_type {
            NoteType::Tap => 1,
            NoteType::Hold => 2,
            NoteType::Roll => 3,
            NoteType::Mine => 4,
            NoteType::Lift => 5,
            NoteType::Fake => 6,
        };
        checksum.rotate_left(11)
            ^ note.row_index as u64
            ^ (note.column as u64).rotate_left(17)
            ^ (note_type << 29)
            ^ note.tail_row_index.unwrap_or_default() as u64
    })
}

fn meter_checksum(mut checksum: u64, meters: &[u32]) -> u64 {
    for &meter in meters {
        checksum = checksum.rotate_left(7) ^ u64::from(meter);
    }
    checksum ^ meters.len() as u64
}

fn foreground_media_values(changes: &[SerializableSongForegroundChange]) -> Vec<(u32, &str)> {
    changes
        .iter()
        .map(|change| (change.start_beat.to_bits(), change.path.as_str()))
        .collect()
}

fn foreground_lua_values(changes: &[SerializableSongForegroundLuaChange]) -> Vec<(u32, &str)> {
    changes
        .iter()
        .map(|change| (change.start_beat.to_bits(), change.path.as_str()))
        .collect()
}

fn foreground_checksum(
    media: &[SerializableSongForegroundChange],
    lua: &[SerializableSongForegroundLuaChange],
    has_lua: bool,
) -> u64 {
    let media_sum = media.iter().fold(0_u64, |checksum, change| {
        checksum.rotate_left(5)
            ^ u64::from(change.start_beat.to_bits())
            ^ text_checksum(&change.path).rotate_left(19)
    });
    lua.iter().fold(media_sum, |checksum, change| {
        checksum.rotate_left(7)
            ^ u64::from(change.start_beat.to_bits())
            ^ text_checksum(&change.path).rotate_left(23)
    }) ^ u64::from(has_lua)
}

fn media_classification(path: &Path) -> u64 {
    u64::from(foreground_media_ext_rank(path).unwrap_or(u8::MAX))
        | (u64::from(is_bgchange_movie_path(path)) << 8)
        | (u64::from(is_song_art_image(path)) << 9)
}

fn media_classification_legacy(path: &Path) -> u64 {
    u64::from(foreground_media_ext_rank_legacy(path).unwrap_or(u8::MAX))
        | (u64::from(is_bgchange_movie_path_legacy(path)) << 8)
        | (u64::from(is_song_art_image_legacy(path)) << 9)
}

fn color_checksum(color: Option<[f32; 4]>) -> u64 {
    color.map_or(0, |color| {
        color.into_iter().fold(0_u64, |checksum, component| {
            checksum.rotate_left(13) ^ u64::from(component.to_bits())
        })
    })
}

fn path_checksum(path: &Path) -> u64 {
    text_checksum(path.to_string_lossy().as_ref())
}

fn text_checksum(text: &str) -> u64 {
    text.bytes().fold(text.len() as u64, |checksum, byte| {
        checksum.rotate_left(5) ^ u64::from(byte)
    })
}

fn benchmark_simfile() -> Vec<u8> {
    let mut data = Vec::with_capacity(TAG_BYTES + 128);
    data.extend_from_slice(b"#TITLE:Benchmark;#CDIMAGE:old.png;\n");
    while data.len() < TAG_BYTES / 2 {
        data.extend_from_slice(b"00000000\n00000000\n00001000\n00000000\n");
    }
    data.extend_from_slice(b"#DISCIMAGE:disc.png;\n");
    while data.len() < TAG_BYTES {
        data.extend_from_slice(b"00000000\n00000000\n00000000\n00000000\n");
    }
    data.extend_from_slice(b"#CDIMAGE:new\\;image.png;");
    data
}

fn benchmark_note_data() -> Vec<u8> {
    let mut data = Vec::with_capacity(NOTE_ROWS * 5);
    for row in 0..NOTE_ROWS {
        let line = match row % 64 {
            0 => b"2000\n".as_slice(),
            16 => b"3000\n".as_slice(),
            24 => b"0040\n".as_slice(),
            40 => b"0030\n".as_slice(),
            value if value.is_multiple_of(8) => b"1001\n".as_slice(),
            5 => b"0M00\n".as_slice(),
            13 => b"000L\n".as_slice(),
            21 => b"F000\n".as_slice(),
            _ => b"0000\n".as_slice(),
        };
        data.extend_from_slice(line);
    }
    data
}

fn benchmark_media_paths() -> Vec<PathBuf> {
    const EXTENSIONS: [&str; 24] = [
        "MP4", "avi", "F4V", "flv", "M4V", "mkv", "MOV", "mpeg", "MPG", "ogv", "WEBM", "wmv",
        "PNG", "jpg", "JPEG", "gif", "BMP", "txt", "INI", "xml", "m2v", "ogg", "", "mp3",
    ];
    (0..MEDIA_PATHS)
        .map(|index| {
            PathBuf::from(format!(
                "Songs/Performance/Song{index:05}/asset.{extension}",
                extension = EXTENSIONS[index % EXTENSIONS.len()]
            ))
        })
        .collect()
}

struct ForegroundFixture {
    root: PathBuf,
    song_dir: PathBuf,
    simfile: Vec<u8>,
}

impl ForegroundFixture {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "deadsync-foreground-benchmark-{}-{unique}",
            std::process::id()
        ));
        let song_dir = root.join("Performance Pack").join("Song");
        let media_dir = song_dir.join("animations");
        let lua_dir = song_dir.join("scripts");
        fs::create_dir_all(&media_dir).expect("create benchmark media directory");
        fs::create_dir_all(&lua_dir).expect("create benchmark Lua directory");
        fs::write(media_dir.join("clip.avi"), b"benchmark video").expect("write benchmark media");
        fs::write(lua_dir.join("default.lua"), b"return Def.ActorFrame {}")
            .expect("write benchmark Lua");

        let mut simfile = Vec::with_capacity(256 * 1024);
        simfile.extend_from_slice(b"#TITLE:Foreground Benchmark;\n");
        for index in 0..128 {
            simfile.extend_from_slice(
                format!(
                    "#FGCHANGES:{}=animations=1=0=0=0=0,{}=scripts=1=0=0=0=0;\n",
                    index * 4,
                    index * 4 + 2
                )
                .as_bytes(),
            );
            simfile.extend_from_slice(b"0000\n0000\n1000\n0000\n");
        }
        while simfile.len() < 256 * 1024 {
            simfile.extend_from_slice(b"0000\n0000\n0000\n0000\n");
        }

        Self {
            root,
            song_dir,
            simfile,
        }
    }
}

impl Drop for ForegroundFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn benchmark_song(index: usize) -> Arc<SongData> {
    let shuffled = index.wrapping_mul(2_654_435_761usize) % SONGS;
    let title = if index.is_multiple_of(17) {
        format!("SONG {shuffled:05}")
    } else {
        format!("Song {shuffled:05}")
    };
    let translit_title = if index.is_multiple_of(7) {
        format!("Translit {shuffled:05}")
    } else {
        String::new()
    };
    Arc::new(SongData {
        simfile_path: PathBuf::from(format!("Songs/Performance/Song{index:05}/song.ssc")),
        title,
        subtitle: "Tournament Mix".to_string(),
        translit_title,
        translit_subtitle: String::new(),
        artist: format!("Artist {:03}", index % 257),
        genre: format!("Genre {}", index % 12),
        banner_path: None,
        background_path: None,
        background_changes: Vec::new(),
        background_layer2_changes: Vec::new(),
        foreground_changes: Vec::new(),
        background_lua_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: false,
        cdtitle_path: None,
        music_path: None,
        display_bpm: "120:180".to_string(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 120.0,
        max_bpm: 180.0,
        normalized_bpms: "0=120,64=180".to_string(),
        music_length_seconds: 120.0 + (index % 240) as f32,
        first_second: 0.0,
        total_length_seconds: 120 + (index % 240) as i32,
        precise_last_second_seconds: 120.0,
        charts: (0..6)
            .map(|chart_index| benchmark_chart(index, chart_index))
            .collect(),
    })
}

fn benchmark_pack(index: usize) -> SongPack {
    let shuffled = index.wrapping_mul(2_654_435_761usize) % PACKS;
    let sort_title = if index.is_multiple_of(13) {
        format!("PACK {shuffled:05}")
    } else {
        format!("Pack {shuffled:05}")
    };
    SongPack {
        group_name: format!("Group {index:05}"),
        name: sort_title.clone(),
        sort_title,
        translit_title: String::new(),
        series: String::new(),
        year: 0,
        sync_pref: SyncPref::Default,
        directory: PathBuf::from(format!("Songs/Group{index:05}")),
        banner_path: None,
        songs: Vec::new(),
    }
}

fn benchmark_chart(index: usize, chart_index: usize) -> ChartData {
    let difficulty = match chart_index {
        0 => "Beginner",
        1 => "Easy",
        2 => "Medium",
        3 => "Hard",
        4 => "Challenge",
        _ => "Edit",
    };
    let meter = if chart_index == 4 {
        5 + (index % 20) as u32
    } else {
        5 + ((index + chart_index * 3) % 20) as u32
    };
    ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: difficulty.to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter,
        step_artist: String::new(),
        music_path: None,
        short_hash: format!("bench-{index:05}-{chart_index}"),
        stats: ArrowStats::default(),
        tech_counts: TechCounts::default(),
        mines_nonfake: 0,
        stamina_counts: StaminaCounts::default(),
        total_streams: 0,
        matrix_rating: 0.0,
        max_nps: 0.0,
        sn_detailed_breakdown: String::new(),
        sn_partial_breakdown: String::new(),
        sn_simple_breakdown: String::new(),
        detailed_breakdown: String::new(),
        partial_breakdown: String::new(),
        simple_breakdown: String::new(),
        total_measures: 0,
        measure_nps_vec: Vec::new(),
        measure_seconds_vec: Vec::new(),
        first_second: 0.0,
        has_note_data: true,
        has_chart_attacks: false,
        possible_grade_points: 0,
        holds_total: 0,
        rolls_total: 0,
        mines_total: 0,
        display_bpm: None,
        min_bpm: 120.0,
        max_bpm: 180.0,
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and the timestamp read do not dereference memory and only
    // serialize this thread's measured interval.
    unsafe {
        core::arch::x86_64::_mm_lfence();
        let cycles = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        cycles
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}
