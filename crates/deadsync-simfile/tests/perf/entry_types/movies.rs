use super::*;
#[path = "movies_baseline.rs"]
mod baseline;
use crate::entry_types_perf_fixtures as fixtures;

#[test]
fn movie_entry_types_preserve_paths_order_and_links() {
    fixtures::verify(
        fixtures::Kind::Movies,
        baseline::list_bgchange_song_movies,
        list_bgchange_song_movies,
    );
}

#[test]
#[ignore = "manual original/current background movie discovery benchmark; run in release"]
fn movie_entry_types_benchmark() {
    fixtures::benchmark(
        "song-movies",
        fixtures::Kind::Movies,
        baseline::list_bgchange_song_movies,
        list_bgchange_song_movies,
    );
}
