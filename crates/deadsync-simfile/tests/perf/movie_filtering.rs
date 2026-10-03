use super::*;
use std::hint::black_box;

#[path = "movie_filtering/baseline.rs"]
mod baseline;
#[allow(dead_code)]
#[path = "asset_discovery/fixtures.rs"]
mod fixtures;

fn fixture(files: usize, movies: usize) -> fixtures::Fixture {
    let fixture = fixtures::Fixture::new("movie-filtering", 0, 0);
    for index in 0..files {
        std::fs::write(
            fixture.path.join(format!(
                "{index:04}.{}",
                if index < movies { "MP4" } else { "ssc" }
            )),
            [],
        )
        .unwrap();
    }
    fixture
}

#[test]
fn movie_metadata_filtering_preserves_files_links_and_sorted_order() {
    let fixture = fixture(48, 8);
    for name in ["movie.avi", "日本.mpg", "cover.png", "._ignored.mp4"] {
        std::fs::write(fixture.path.join(name), []).unwrap();
    }
    let directory = fixture.path.join("folder.mp4");
    std::fs::create_dir(&directory).unwrap();
    fixtures::link(
        &fixture.path.join("0000.MP4"),
        &fixture.path.join("linked.mp4"),
        false,
    );
    fixtures::link(&directory, &fixture.path.join("junction.mp4"), true);
    assert_eq!(
        list_bgchange_song_movies(&fixture.path),
        baseline::list_bgchange_song_movies(&fixture.path)
    );
    let missing = fixture.path.join("absent");
    assert_eq!(
        list_bgchange_song_movies(&missing),
        baseline::list_bgchange_song_movies(&missing)
    );
}

#[test]
#[ignore = "manual release benchmark"]
fn movie_filtering_benchmark() {
    for (files, movies) in [(32, 32), (128, 8), (1024, 8)] {
        let fixture = fixture(files, movies);
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!(
                    "movies/files={files}/movies={movies}/{}",
                    if old { "original" } else { "current" }
                ),
                16,
                1,
                || {
                    black_box(if old {
                        baseline::list_bgchange_song_movies(black_box(&fixture.path))
                    } else {
                        list_bgchange_song_movies(black_box(&fixture.path))
                    });
                },
            );
        }
    }
}
