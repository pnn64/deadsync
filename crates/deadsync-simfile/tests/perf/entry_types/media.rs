use super::*;
#[path = "media_baseline.rs"]
mod baseline;
use crate::entry_types_perf_fixtures as fixtures;

#[test]
fn random_movie_entry_types_preserve_paths_order_and_links() {
    fixtures::verify(
        fixtures::Kind::Movies,
        baseline::list_random_movie_paths,
        list_random_movie_paths,
    );
}

#[test]
fn foreground_entry_types_preserve_ranks_names_and_links() {
    fixtures::verify(
        fixtures::Kind::Foreground,
        baseline::resolve_foreground_media_dir,
        resolve_foreground_media_dir,
    );
}

#[test]
fn ordinary_entry_type_checks_match_metadata_without_heap_churn() {
    let fixture = fixtures::fixture(12, 12, fixtures::Kind::Artwork);
    for entry in fs::read_dir(&fixture.path).unwrap().map(Result::unwrap) {
        let path = entry.path();
        assert_eq!(song_asset_entry_is_file(&entry, &path), path.is_file());
        #[cfg(windows)]
        crate::perf::assert_no_churn(|| {
            assert!(song_asset_entry_is_file(&entry, &path));
        });
    }
}

#[test]
#[ignore = "manual original/current random and foreground discovery benchmark; run in release"]
fn media_entry_types_benchmark() {
    fixtures::benchmark(
        "random-movies",
        fixtures::Kind::Movies,
        baseline::list_random_movie_paths,
        list_random_movie_paths,
    );
    fixtures::benchmark(
        "foreground",
        fixtures::Kind::Foreground,
        baseline::resolve_foreground_media_dir,
        resolve_foreground_media_dir,
    );
}
