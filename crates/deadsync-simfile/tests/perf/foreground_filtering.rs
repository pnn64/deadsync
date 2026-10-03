use super::*;
use std::hint::black_box;

#[path = "foreground_filtering/baseline.rs"]
mod baseline;
#[allow(dead_code)]
#[path = "asset_discovery/fixtures.rs"]
mod fixtures;

fn fixture(files: usize, supported: usize) -> fixtures::Fixture {
    let fixture = fixtures::Fixture::new("foreground-filtering", 0, 0);
    for index in 0..files {
        let extension = if index < supported {
            ["PNG", "MP4", "jpg", "AVI"][index % 4]
        } else {
            "ssc"
        };
        fs::write(fixture.path.join(format!("{index:04}.{extension}")), []).unwrap();
    }
    fixture
}

#[test]
fn foreground_filter_preserves_ranks_names_directories_and_links() {
    let fixture = fixture(48, 8);
    for name in [
        "_image.png",
        "日本.jpeg",
        "_MOVIE.AVI",
        "notes.sm",
        "._resource.mp4",
        "Alpha.MP4",
        "alpha.mp4",
    ] {
        fs::write(fixture.path.join(name), []).unwrap();
    }
    let directory = fixture.path.join("folder.avi");
    fs::create_dir(&directory).unwrap();
    fixtures::link(
        &fixture.path.join("0001.MP4"),
        &fixture.path.join("linked.mp4"),
        false,
    );
    fixtures::link(&directory, &fixture.path.join("junction.mp4"), true);
    for removed in [
        None,
        Some("._resource.mp4"),
        Some("0001.MP4"),
        Some("0003.AVI"),
    ] {
        if let Some(name) = removed {
            fs::remove_file(fixture.path.join(name)).unwrap();
        }
        assert_eq!(
            resolve_foreground_media_dir(&fixture.path),
            baseline::resolve_foreground_media_dir(&fixture.path)
        );
    }
    for path in [fixture.path.join("absent"), fixture.path.join("empty")] {
        if path.file_name().unwrap() == "empty" {
            fs::create_dir(&path).unwrap();
        }
        assert_eq!(
            resolve_foreground_media_dir(&path),
            baseline::resolve_foreground_media_dir(&path)
        );
    }
}

#[test]
fn foreground_filter_preserves_each_extension_preference() {
    let fixture = fixture(0, 0);
    for extension in ["mp4", "png", "gif", "jpeg", "jpg", "bmp", "m2v", "txt"] {
        let path = fixture.path.join(format!("media.{extension}"));
        fs::write(&path, []).unwrap();
        assert_eq!(
            resolve_foreground_media_dir(&fixture.path),
            baseline::resolve_foreground_media_dir(&fixture.path)
        );
        fs::remove_file(path).unwrap();
    }
}

#[test]
#[ignore = "manual original/current foreground discovery benchmark; run in release"]
fn foreground_filtering_benchmark() {
    for (files, supported) in [(32, 32), (128, 8), (1024, 8)] {
        let fixture = fixture(files, supported);
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for new in order {
            crate::perf::measure_sampled(
                &format!(
                    "foreground/files={files}/supported={supported}/{}",
                    if new { "new" } else { "old" }
                ),
                16,
                1,
                || {
                    if new {
                        resolve_foreground_media_dir(black_box(&fixture.path))
                    } else {
                        baseline::resolve_foreground_media_dir(black_box(&fixture.path))
                    }
                },
            );
        }
    }
}
