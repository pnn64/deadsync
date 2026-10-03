use super::*;
#[path = "artwork_baseline.rs"]
mod baseline;
use crate::entry_types_perf_fixtures as fixtures;

#[test]
fn artwork_entry_types_preserve_paths_order_and_links() {
    fixtures::verify(
        fixtures::Kind::Artwork,
        baseline::list_song_art_images,
        list_song_art_images,
    );
}

#[test]
#[ignore = "manual original/current artwork discovery benchmark; run in release"]
fn artwork_entry_types_benchmark() {
    fixtures::benchmark(
        "artwork",
        fixtures::Kind::Artwork,
        baseline::list_song_art_images,
        list_song_art_images,
    );
}
