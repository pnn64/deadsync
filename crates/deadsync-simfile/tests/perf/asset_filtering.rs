use super::*;
use std::hint::black_box;

#[path = "asset_filtering/baseline.rs"]
mod baseline;
#[allow(dead_code)]
#[path = "asset_discovery/fixtures.rs"]
mod fixtures;

fn fixture(files: usize, images: usize) -> fixtures::Fixture {
    let fixture = fixtures::Fixture::new("asset-filtering", 0, 0);
    for index in 0..files {
        fs::write(
            fixture.path.join(format!(
                "{index:04}.{}",
                if index < images { "PNG" } else { "ssc" }
            )),
            [],
        )
        .unwrap();
    }
    fixture
}

#[test]
fn filename_filtering_preserves_files_directories_and_links() {
    let fixture = fixture(48, 8);
    for name in [
        "._resource.png",
        "movie.mp4",
        "日本.jpeg",
        "cover.JPG",
        "notes.sm",
    ] {
        fs::write(fixture.path.join(name), []).unwrap();
    }
    let directory = fixture.path.join("folder.png");
    fs::create_dir(&directory).unwrap();
    fixtures::link(
        &fixture.path.join("0000.PNG"),
        &fixture.path.join("linked.png"),
        false,
    );
    fixtures::link(&directory, &fixture.path.join("junction.png"), true);
    assert_eq!(
        list_song_art_images(&fixture.path),
        baseline::list_song_art_images(&fixture.path)
    );
    let missing = fixture.path.join("absent");
    assert_eq!(
        list_song_art_images(&missing),
        baseline::list_song_art_images(&missing)
    );
}

#[test]
#[ignore = "manual release benchmark"]
fn asset_filtering_benchmark() {
    for (files, images) in [(32, 32), (128, 8), (1024, 8)] {
        let fixture = fixture(files, images);
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!(
                    "assets/files={files}/images={images}/{}",
                    if old { "original" } else { "current" }
                ),
                16,
                1,
                || {
                    black_box(if old {
                        baseline::list_song_art_images(black_box(&fixture.path))
                    } else {
                        list_song_art_images(black_box(&fixture.path))
                    });
                },
            );
        }
    }
}
