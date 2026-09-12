use super::*;
use std::hint::black_box;

#[path = "asset_discovery/cache_baseline.rs"]
mod baseline;
#[path = "asset_discovery/fixtures.rs"]
mod fixtures;
use fixtures::{Fixture, pair};

#[test]
fn directory_hash_matches_old_across_files_directories_and_edits() {
    for files in [0, 1, 16, 256] {
        let fixture = Fixture::new("hash", files, 2);
        let path = fixture.path.join("song.ssc");
        std::fs::write(&path, b"#TITLE:Song;").unwrap();
        assert_eq!(
            get_song_directory_hash(&path).unwrap(),
            baseline::get_song_directory_hash(&path).unwrap()
        );
        let before = get_song_directory_hash(&path).unwrap();
        std::fs::write(&path, b"#TITLE:Changed and longer song;").unwrap();
        let after = get_song_directory_hash(&path).unwrap();
        assert_ne!(before, after);
        assert_eq!(after, baseline::get_song_directory_hash(&path).unwrap());
        std::fs::write(fixture.path.join("._ignored.png"), b"ignored").unwrap();
        assert_eq!(
            get_song_directory_hash(&path).unwrap(),
            baseline::get_song_directory_hash(&path).unwrap()
        );
        std::fs::write(fixture.path.join("._ignored.png"), vec![0; 2048]).unwrap();
        assert_eq!(get_song_directory_hash(&path).unwrap(), after);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            get_song_directory_hash(&path).unwrap(),
            baseline::get_song_directory_hash(&path).unwrap()
        );
    }
}

#[test]
fn directory_hash_follows_links_and_preserves_errors() {
    let fixture = Fixture::new("hash-links", 1, 1);
    let path = fixture.path.join("song.ssc");
    let target = fixture.path.join("Layer0/target");
    std::fs::write(&target, b"original").unwrap();
    let file_link = fixtures::link(&target, &fixture.path.join("file-link"), false);
    fixtures::link(
        &fixture.path.join("Layer0"),
        &fixture.path.join("dir-link"),
        true,
    );
    let before = get_song_directory_hash(&path).unwrap();
    assert_eq!(before, baseline::get_song_directory_hash(&path).unwrap());
    std::fs::write(&target, vec![1; 8192]).unwrap();
    let after = get_song_directory_hash(&path).unwrap();
    if file_link {
        assert_ne!(before, after);
    }
    assert_eq!(after, baseline::get_song_directory_hash(&path).unwrap());
    let missing = fixture.path.join("temporary-target");
    std::fs::create_dir(&missing).unwrap();
    fixtures::link(&missing, &fixture.path.join("broken"), true);
    std::fs::remove_dir(&missing).unwrap();
    for path in [&path, &fixture.path.join("missing/song.ssc"), Path::new("")] {
        assert_eq!(
            get_song_directory_hash(path).unwrap_err().kind(),
            baseline::get_song_directory_hash(path).unwrap_err().kind()
        );
    }
}

#[test]
fn directory_hash_preserves_timestamp_rounding() {
    let fixture = Fixture::new("hash-times", 0, 0);
    let path = fixture.path.join("song.ssc");
    let file = std::fs::File::create(&path).unwrap();
    for time in [
        UNIX_EPOCH - std::time::Duration::from_secs(1),
        UNIX_EPOCH + std::time::Duration::new(1, 999_999_999),
        UNIX_EPOCH + std::time::Duration::new(1_700_000_000, 123_456_789),
    ] {
        file.set_times(std::fs::FileTimes::new().set_modified(time))
            .unwrap();
        assert_eq!(
            get_song_directory_hash(&path).unwrap(),
            baseline::get_song_directory_hash(&path).unwrap()
        );
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn asset_discovery_bench() {
    for (label, files, depth, links) in [
        ("empty", 0, 0, false),
        ("small", 8, 0, false),
        ("flat128", 128, 0, false),
        ("flat512", 512, 0, false),
        ("mixed", 32, 4, true),
    ] {
        let fixture = Fixture::new("hash-bench", files, depth);
        let path = fixture.path.join("song.ssc");
        if links {
            std::fs::write(fixture.path.join("target"), b"target").unwrap();
            fixtures::link(
                &fixture.path.join("target"),
                &fixture.path.join("link"),
                false,
            );
            fixtures::link(
                &fixture.path.join("Layer0"),
                &fixture.path.join("dir-link"),
                true,
            );
        }
        assert_eq!(
            get_song_directory_hash(&path).unwrap(),
            baseline::get_song_directory_hash(&path).unwrap()
        );
        pair(
            &format!("hash_{label}"),
            32,
            std::fs::read_dir(&fixture.path).unwrap().count().max(1),
            || baseline::get_song_directory_hash(black_box(&path)).unwrap(),
            || get_song_directory_hash(black_box(&path)).unwrap(),
        );
    }
}
