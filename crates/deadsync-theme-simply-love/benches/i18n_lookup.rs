use deadsync_theme_simply_love::i18n::{self, tr};
use std::hint::black_box;
use std::time::Instant;

const BATCHES: usize = 200_000;
const SELECT_MUSIC_KEYS: [(&str, &str); 34] = [
    ("ScreenTitles", "SelectMusic"),
    ("SelectMusic", "PressStartForOptions"),
    ("SelectMusic", "EnteringOptions"),
    ("SelectMusic", "ExitGamePrompt"),
    ("SelectMusic", "KeepPlayingInfo"),
    ("SelectMusic", "FinishedInfo"),
    ("SelectMusic", "RecentlyPlayed"),
    ("SelectMusic", "MostPopular"),
    ("SelectMusic", "ArtistLabel"),
    ("SelectMusic", "BPMLabel"),
    ("SelectMusic", "LengthLabel"),
    ("SelectMusic", "StepsLabel"),
    ("SelectMusic", "ExScore"),
    ("SelectMusic", "ItgScore"),
    ("SelectMusic", "OptionsMenuLabel"),
    ("SelectMusic", "SortBy"),
    ("SelectMusic", "Genre"),
    ("SelectMusic", "MachineTopScores"),
    ("SelectMusic", "P1MostPlayed"),
    ("SelectMusic", "P2MostPlayed"),
    ("SelectMusic", "P1RecentSongs"),
    ("SelectMusic", "P2RecentSongs"),
    ("SelectMusic", "ChangeStyleTo"),
    ("SelectMusic", "TestInputPrompt"),
    ("SelectMusic", "SongSearchPrompt"),
    ("SelectMusic", "ReloadPrompt"),
    ("SelectMusic", "Favorites"),
    ("SelectMusic", "Unplayed"),
    ("SelectMusic", "UnknownGenre"),
    ("SelectMusic", "NotAvailable"),
    ("SelectMusic", "TotalLabel"),
    ("SelectMusic", "MusicRateSuffix"),
    ("Common", "Yes"),
    ("Common", "No"),
];

fn main() {
    i18n::init(deadsync_assets::language::load_for_tests("en"));
    for _ in 0..1_000 {
        black_box(lookup_batch());
    }

    let started = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..BATCHES {
        checksum = checksum.wrapping_add(black_box(lookup_batch()));
    }
    let elapsed = started.elapsed();
    let lookups = BATCHES * SELECT_MUSIC_KEYS.len();
    let ns_per_lookup = elapsed.as_secs_f64() * 1.0e9 / lookups as f64;

    println!("translation lookup microbenchmark");
    println!(
        "{lookups} Select Music translation hits in {:.3}s",
        elapsed.as_secs_f64()
    );
    println!(
        "{ns_per_lookup:>10.2} ns/lookup  {:>10.2} Mlookups/s  checksum={checksum}",
        lookups as f64 / elapsed.as_secs_f64() / 1.0e6,
    );
}

#[inline(never)]
fn lookup_batch() -> usize {
    let mut checksum = 0usize;
    for &(section, key) in &SELECT_MUSIC_KEYS {
        let value = black_box(tr(black_box(section), black_box(key)));
        checksum = checksum.wrapping_add(value.len());
    }
    checksum
}
