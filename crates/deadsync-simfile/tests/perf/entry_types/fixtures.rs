use std::fmt::Debug;
use std::fs;
use std::hint::black_box;
use std::path::Path;

#[allow(dead_code)]
#[path = "../asset_discovery/fixtures.rs"]
mod filesystem;

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Artwork,
    Movies,
    Foreground,
}

pub(super) fn fixture(files: usize, supported: usize, kind: Kind) -> filesystem::Fixture {
    let fixture = filesystem::Fixture::new("entry-types", 0, 0);
    for index in 0..files {
        let extensions = match kind {
            Kind::Artwork => ["PNG", "jpg", "GIF", "jpeg"],
            Kind::Movies => ["MP4", "avi", "WMV", "mpg"],
            Kind::Foreground => ["PNG", "MP4", "jpg", "AVI"],
        };
        let extension = if index < supported {
            extensions[index % 4]
        } else {
            "ssc"
        };
        fs::write(fixture.path.join(format!("{index:04}.{extension}")), []).unwrap();
    }
    fixture
}

pub(super) fn verify<T: PartialEq + Debug>(
    kind: Kind,
    original: fn(&Path) -> T,
    current: fn(&Path) -> T,
) {
    for (files, supported) in [(0, 0), (12, 0), (12, 12), (48, 8), (256, 256)] {
        let fixture = fixture(files, supported, kind);
        assert_eq!(current(&fixture.path), original(&fixture.path));
        #[cfg(windows)]
        if supported > 0 {
            crate::perf::assert_reduced_churn(
                || {
                    black_box(original(&fixture.path));
                },
                || {
                    black_box(current(&fixture.path));
                },
            );
        }
    }
    let fixture = fixture(40, 8, kind);
    for name in [
        "._ignored.PNG",
        "._ignored.MP4",
        "alpha.mp4",
        "Alpha.MP4",
        "日本.jpeg",
        "日本.MPG",
        "cover.bmp",
        "legacy.m2v",
        "no-extension",
    ] {
        fs::write(fixture.path.join(name), []).unwrap();
    }
    let extension = match kind {
        Kind::Artwork => "png",
        _ => "mp4",
    };
    let source = fixture.path.join(format!("source.{extension}"));
    fs::write(&source, []).unwrap();
    let hard_link = fixture.path.join(format!("hard.{extension}"));
    fs::hard_link(&source, &hard_link).unwrap();
    filesystem::link(
        &source,
        &fixture.path.join(format!("linked.{extension}")),
        false,
    );
    let directory = fixture.path.join(format!("directory.{extension}"));
    fs::create_dir(&directory).unwrap();
    filesystem::link(
        &directory,
        &fixture.path.join(format!("junction.{extension}")),
        true,
    );
    assert_eq!(current(&fixture.path), original(&fixture.path));
    fs::remove_file(&source).unwrap();
    fs::remove_dir(&directory).unwrap();
    // The hard link stays a regular file; symbolic links/junctions now dangle.
    assert_eq!(current(&fixture.path), original(&fixture.path));
    for path in [fixture.path.join("absent"), hard_link] {
        assert_eq!(current(&path), original(&path));
    }
}

pub(super) fn benchmark<T>(
    name: &str,
    kind: Kind,
    original: fn(&Path) -> T,
    current: fn(&Path) -> T,
) {
    for (files, supported) in [(32, 32), (128, 128), (1024, 1024), (1024, 8), (128, 0)] {
        let fixture = fixture(files, supported, kind);
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [true, false]
        } else {
            [false, true]
        };
        for new in order {
            let variant = if new { "new" } else { "old" };
            let work = if new { current } else { original };
            crate::perf::measure_sampled(
                &format!("{name}/files={files}/supported={supported}/{variant}"),
                16,
                1,
                || work(black_box(&fixture.path)),
            );
        }
    }
}
