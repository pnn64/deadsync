use super::*;
use crate::perf;
use std::{
    hint::black_box,
    io::Cursor,
    time::{Duration, SystemTime},
};

mod baseline;

const HEX: &str = "0123456789abcdef";
const OPTIONS: BannerCacheOptions = BannerCacheOptions { enabled: true };
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Tree {
    path: PathBuf,
    root: PathBuf,
}
impl Tree {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let path = root.join(format!(
            "banner-cache-{:010}-{:010}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path, root }
    }
    fn cache(&self) -> PathBuf {
        self.path.join(format!("{HEX}-current.rgba"))
    }
    fn source(&self) -> PathBuf {
        self.path.join("source.png")
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        assert_eq!(self.path.parent(), Some(self.root.as_path()));
        assert!(
            self.path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("banner-cache-")
        );
        fs::remove_dir_all(&self.path).unwrap();
    }
}

fn raw(width: u32, height: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = b"DSBNR02\0".to_vec();
    bytes.extend(width.to_le_bytes());
    bytes.extend(height.to_le_bytes());
    bytes.extend(payload);
    bytes
}
fn stamp(path: &Path, seconds: u64) {
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(seconds);
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(time))
        .unwrap();
}
fn cache_fixture(width: u32, height: u32) -> Tree {
    let tree = Tree::new();
    let image = RgbaImage::from_pixel(width, height, image::Rgba([17, 39, 81, 255]));
    fs::write(
        tree.source(),
        b"cached hits must not decode this invalid PNG",
    )
    .unwrap();
    assert!(save_raw_cached_banner_image(&tree.cache(), &image));
    stamp(&tree.source(), 1_600_000_000);
    stamp(&tree.cache(), 1_600_000_100);
    tree
}

#[test]
fn raw_validation_matches_pixel_loading_and_rejects_malformed_caches() {
    let tree = Tree::new();
    let mut cases = vec![
        (raw(0, 0, &[]), true),
        (raw(0, 7, &[]), true),
        (raw(1, 1, &[1, 2, 3, 4]), true),
        (raw(7, 3, &(0..84).collect::<Vec<_>>()), true),
        (raw(1, 1, &[1, 2, 3]), false),
        (raw(1, 1, &[1, 2, 3, 4, 5]), false),
        (raw(u32::MAX, 2, &[]), false),
        (raw(1 << 30, 1, &[]), false),
        (raw(u32::MAX, 0, &[]), usize::BITS > 32),
        (raw(0, u32::MAX, &[]), true),
    ];
    let valid = raw(1, 1, &[1, 2, 3, 4]);
    for len in 0..16 {
        cases.push((valid[..len].to_vec(), false));
    }
    let mut bad_magic = valid.clone();
    bad_magic[5] = b'9';
    cases.push((bad_magic, false));
    for (bytes, expected) in cases {
        fs::write(tree.cache(), &bytes).unwrap();
        let old = baseline::load_raw_cached_banner_image(&tree.cache());
        assert_eq!(old.is_some(), expected);
        assert_eq!(load_raw_cached_banner_image(&tree.cache()), old);
        assert_eq!(
            validate_raw_cached_banner(&tree.cache()).is_some(),
            expected
        );
        assert_eq!(
            validate_raw_banner(&mut Cursor::new(&bytes), bytes.len()).is_some(),
            expected
        );
    }
    let bytes = raw(320, 80, &vec![123; 320 * 80 * 4]);
    perf::assert_no_churn(|| {
        assert_eq!(
            validate_raw_banner(&mut Cursor::new(&bytes), bytes.len()),
            Some(())
        );
    });
}

struct ShortReader<'a> {
    bytes: &'a [u8],
    position: usize,
    fail_at: usize,
    interrupted: bool,
}
impl Read for ShortReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.interrupted {
            self.interrupted = false;
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        if self.position >= self.fail_at {
            return Err(std::io::Error::other("injected payload read error"));
        }
        let count = out
            .len()
            .min(3)
            .min(self.bytes.len() - self.position)
            .min(self.fail_at - self.position);
        out[..count].copy_from_slice(&self.bytes[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

#[test]
fn streaming_validation_reads_every_byte_and_propagates_payload_errors() {
    let bytes = raw(4, 2, &[17; 32]);
    let mut good = ShortReader {
        bytes: &bytes,
        position: 0,
        fail_at: usize::MAX,
        interrupted: true,
    };
    assert_eq!(validate_raw_banner(&mut good, bytes.len()), Some(()));
    assert_eq!(good.position, bytes.len());
    for fail_at in [0, 15, 16, 17, bytes.len() - 1] {
        let mut reader = ShortReader {
            bytes: &bytes,
            position: 0,
            fail_at,
            interrupted: false,
        };
        assert_eq!(validate_raw_banner(&mut reader, bytes.len()), None);
    }
    // A file shrinking after its metadata was read must also be rejected.
    for len in [16, 17, bytes.len() - 1] {
        assert_eq!(
            validate_raw_banner(&mut Cursor::new(&bytes[..len]), bytes.len()),
            None
        );
    }
    let mut extended = bytes.clone();
    extended.push(99);
    let mut reader = Cursor::new(&extended);
    assert_eq!(validate_raw_banner(&mut reader, bytes.len()), Some(()));
    assert_eq!(reader.position(), bytes.len() as u64);

    for pixels in [16_383, 16_384, 16_385, 32_769] {
        let bytes = raw(pixels, 1, &vec![71; pixels as usize * 4]);
        let mut reader = Cursor::new(&bytes);
        perf::assert_no_churn(|| {
            assert_eq!(validate_raw_banner(&mut reader, bytes.len()), Some(()));
        });
        assert_eq!(reader.position(), bytes.len() as u64);
        for missing in [1, 4, 16] {
            assert_eq!(
                validate_raw_banner(
                    &mut Cursor::new(&bytes[..bytes.len() - missing]),
                    bytes.len(),
                ),
                None,
            );
        }
    }
}

#[test]
fn cache_load_preserves_freshness_pixels_and_corrupt_file_removal() {
    let tree = Tree::new();
    let source = tree.source();
    let cache = tree.cache();
    for source_time in [
        None,
        Some(1_600_000_000),
        Some(1_600_000_100),
        Some(1_600_000_200),
    ] {
        for valid in [false, true] {
            let bytes = if valid {
                raw(1, 1, &[7, 11, 19, 255])
            } else {
                b"broken".to_vec()
            };
            let reset = || {
                fs::write(&cache, &bytes).unwrap();
                stamp(&cache, 1_600_000_100);
                if let Some(time) = source_time {
                    fs::write(&source, b"unneeded").unwrap();
                    stamp(&source, time);
                } else {
                    let _ = fs::remove_file(&source);
                }
            };
            reset();
            let old = baseline::load_cached_banner_image(&cache, &source);
            let old_disk = fs::read(&cache).ok();
            reset();
            let new = load_cached_banner_image(&cache, &source);
            assert_eq!(new, old);
            assert_eq!(fs::read(&cache).ok(), old_disk);
            let stale = source_time.is_some_and(|time| time > 1_600_000_100);
            assert_eq!(new.is_some(), valid && !stale);
            assert_eq!(cache.exists(), valid || stale);
        }
    }
    for path in [tree.path.join("missing"), tree.path.clone()] {
        assert_eq!(
            load_cached_banner_image(&path, &source),
            baseline::load_cached_banner_image(&path, &source)
        );
    }
    #[cfg(windows)]
    {
        let fixture = cache_fixture(320, 80);
        perf::assert_reduced_churn(
            || {
                black_box(baseline::load_cached_banner_image(
                    &fixture.cache(),
                    &fixture.source(),
                ));
            },
            || {
                black_box(load_cached_banner_image(
                    &fixture.cache(),
                    &fixture.source(),
                ));
            },
        );
    }
}

#[test]
fn prewarm_preserves_reused_rebuilt_failed_and_disk_outcomes() {
    let tree = Tree::new();
    let source = tree.source();
    let cache = tree.cache();
    let stale_variant = tree.path.join(format!("{HEX}-stale.png"));
    let source_image = RgbaImage::from_pixel(3, 2, image::Rgba([6, 5, 4, 255]));
    for state in [
        "fresh",
        "equal",
        "stale",
        "corrupt",
        "missing",
        "source-missing",
        "source-invalid",
        "cache-directory",
    ] {
        let reset = || {
            if cache.is_dir() {
                fs::remove_dir(&cache).unwrap();
            }
            source_image.save(&source).unwrap();
            fs::write(&cache, raw(1, 1, &[1, 2, 3, 255])).unwrap();
            fs::write(&stale_variant, b"keep until rebuilding").unwrap();
            stamp(&source, 1_600_000_000);
            stamp(&cache, 1_600_000_100);
            match state {
                "equal" => stamp(&source, 1_600_000_100),
                "stale" => stamp(&source, 1_600_000_200),
                "corrupt" => {
                    fs::write(&cache, b"invalid").unwrap();
                }
                "missing" => {
                    fs::remove_file(&cache).unwrap();
                }
                "source-missing" => {
                    fs::remove_file(&source).unwrap();
                }
                "source-invalid" => {
                    fs::write(&source, b"invalid PNG").unwrap();
                    fs::write(&cache, b"invalid").unwrap();
                }
                "cache-directory" => {
                    fs::remove_file(&cache).unwrap();
                    fs::create_dir(&cache).unwrap();
                }
                _ => {}
            }
        };
        reset();
        let old = baseline::ensure_cached_dynamic_image_at(&source, OPTIONS, &cache, HEX)
            .map_err(|e| e.to_string());
        let old_disk = (fs::read(&cache).ok(), stale_variant.exists());
        reset();
        let new = ensure_cached_dynamic_image_at(&source, OPTIONS, &cache, HEX)
            .map_err(|e| e.to_string());
        assert_eq!(new, old, "{state}");
        assert_eq!(
            (fs::read(&cache).ok(), stale_variant.exists()),
            old_disk,
            "{state}"
        );
        match state {
            "fresh" | "equal" | "source-missing" => assert_eq!(new, Ok(false)),
            "source-invalid" => assert!(new.is_err()),
            _ => assert_eq!(new, Ok(true)),
        }
    }
    let fixture = cache_fixture(1024, 512);
    let cache = fixture.cache();
    let source = fixture.source();
    perf::assert_reduced_churn(
        || {
            black_box(
                baseline::ensure_cached_dynamic_image_at(&source, OPTIONS, &cache, HEX).unwrap(),
            );
        },
        || {
            black_box(ensure_cached_dynamic_image_at(&source, OPTIONS, &cache, HEX).unwrap());
        },
    );
}

fn remaining(dir: &Path) -> Vec<std::ffi::OsString> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    names.sort();
    names
}

#[test]
fn prune_preserves_current_unrelated_directories_and_exact_name_rules() {
    let tree = Tree::new();
    for hex in [HEX, "", "é"] {
        let current = tree.path.join(format!("{hex}-current.rgba"));
        let names = [
            format!("{hex}-current.rgba"),
            format!("{hex}-old.rgba"),
            format!("{hex}-old.png"),
            format!("{hex}-upper.PNG"),
            format!("{hex}-old.rgba.tmp"),
            format!("{hex}X-old.rgba"),
            format!(".{hex}-old.rgba"),
            format!("{hex}-.png"),
        ];
        let reset = || {
            for name in &names {
                fs::write(tree.path.join(name), b"fixture").unwrap();
            }
        };
        let directory = tree.path.join(format!("{hex}-directory.rgba"));
        fs::create_dir(&directory).unwrap();
        reset();
        baseline::prune_stale_banner_cache_variants(&current, hex);
        let old = remaining(&tree.path);
        reset();
        prune_stale_banner_cache_variants(&current, hex);
        assert_eq!(remaining(&tree.path), old);
        assert!(current.is_file() && directory.is_dir());
        assert!(!tree.path.join(format!("{hex}-old.rgba")).exists());
        assert!(tree.path.join(format!("{hex}-upper.PNG")).is_file());
        for name in remaining(&tree.path) {
            let path = tree.path.join(name);
            if path.is_dir() {
                fs::remove_dir(path).unwrap();
            } else {
                fs::remove_file(path).unwrap();
            }
        }
    }
    let non_utf = {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            std::ffi::OsString::from_wide(&[
                0xd800,
                b'.' as u16,
                b'p' as u16,
                b'n' as u16,
                b'g' as u16,
            ])
        }
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            std::ffi::OsString::from_vec(vec![0xff, b'.', b'p', b'n', b'g'])
        }
        #[cfg(not(any(windows, unix)))]
        {
            std::ffi::OsString::from("unrelated.png")
        }
    };
    fs::write(tree.path.join(non_utf), b"preserved").unwrap();
    let before = remaining(&tree.path);
    prune_stale_banner_cache_variants(&tree.cache(), HEX);
    assert_eq!(remaining(&tree.path), before);
    perf::assert_no_churn(|| prune_stale_banner_cache_variants(Path::new("/"), HEX));
    for i in 0..128 {
        fs::write(tree.path.join(format!("unrelated-{i:04}.rgba")), b"fixture").unwrap();
    }
    let cache = tree.cache();
    perf::assert_reduced_churn(
        || baseline::prune_stale_banner_cache_variants(&cache, HEX),
        || prune_stale_banner_cache_variants(&cache, HEX),
    );
}

#[cfg(any(windows, unix))]
#[test]
fn cache_links_preserve_loading_validation_and_pruning() {
    let tree = cache_fixture(7, 3);
    let target = tree.cache();
    let link = tree.path.join(format!("{HEX}-link.png"));
    #[cfg(windows)]
    let result = std::os::windows::fs::symlink_file(&target, &link);
    #[cfg(unix)]
    let result = std::os::unix::fs::symlink(&target, &link);
    if let Err(error) = result {
        #[cfg(windows)]
        if error.raw_os_error() == Some(1314) {
            eprintln!("File symlink coverage unavailable: Windows privilege 1314");
            return;
        }
        panic!("create file symlink: {error}");
    }
    assert_eq!(
        load_cached_banner_image(&link, &tree.source()),
        baseline::load_cached_banner_image(&link, &tree.source())
    );
    assert_eq!(validate_raw_cached_banner(&link), Some(()));
    baseline::prune_stale_banner_cache_variants(&target, HEX);
    assert!(!link.try_exists().unwrap());
    assert!(target.is_file());
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&target, &link).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    prune_stale_banner_cache_variants(&target, HEX);
    assert!(!link.try_exists().unwrap());
    assert!(target.is_file());

    let missing = tree.path.join("missing");
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&missing, &link).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&missing, &link).unwrap();
    baseline::prune_stale_banner_cache_variants(&target, HEX);
    assert!(fs::symlink_metadata(&link).is_ok());
    prune_stale_banner_cache_variants(&target, HEX);
    assert!(fs::symlink_metadata(&link).is_ok());
}

#[cfg(windows)]
#[test]
fn pruning_preserves_directory_junctions() {
    let tree = Tree::new();
    let directory = tree.path.join("directory");
    fs::create_dir(&directory).unwrap();
    let sentinel = directory.join("keep.txt");
    fs::write(&sentinel, b"preserved").unwrap();
    let link = tree.path.join(format!("{HEX}-junction.rgba"));
    let shell_path = |path: &Path| {
        path.to_str()
            .unwrap()
            .trim_start_matches(r"\\?\")
            .to_owned()
    };
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command",
            "New-Item -ItemType Junction -Path $env:DEADSYNC_TEST_LINK -Target $env:DEADSYNC_TEST_TARGET -ErrorAction Stop | Out-Null"])
        .env("DEADSYNC_TEST_LINK", shell_path(&link))
        .env("DEADSYNC_TEST_TARGET", shell_path(&directory))
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    baseline::prune_stale_banner_cache_variants(&tree.cache(), HEX);
    assert!(link.is_dir());
    prune_stale_banner_cache_variants(&tree.cache(), HEX);
    assert!(link.is_dir());
    assert_eq!(fs::read(&sentinel).unwrap(), b"preserved");
    fs::remove_dir(&link).unwrap();
}

fn compare<T>(name: &str, units: usize, old: impl FnMut() -> T, new: impl FnMut() -> T) {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        perf::measure_sampled(&format!("{name}/new"), 16, units, new);
        perf::measure_sampled(&format!("{name}/old"), 16, units, old);
    } else {
        perf::measure_sampled(&format!("{name}/old"), 16, units, old);
        perf::measure_sampled(&format!("{name}/new"), 16, units, new);
    }
}

#[test]
#[ignore = "manual release before/after CPU and allocation benchmark"]
fn benchmark_banner_cache() {
    let old_load =
        black_box(baseline::load_cached_banner_image as fn(&Path, &Path) -> Option<RgbaImage>);
    let new_load = black_box(load_cached_banner_image as fn(&Path, &Path) -> Option<RgbaImage>);
    let old_ensure = black_box(
        baseline::ensure_cached_dynamic_image_at
            as fn(&Path, BannerCacheOptions, &Path, &str) -> image::ImageResult<bool>,
    );
    let new_ensure = black_box(
        ensure_cached_dynamic_image_at
            as fn(&Path, BannerCacheOptions, &Path, &str) -> image::ImageResult<bool>,
    );
    for (width, height) in [(0, 0), (1, 1), (320, 80), (1024, 512), (2048, 2048)] {
        let tree = cache_fixture(width, height);
        let source = tree.source();
        let cache = tree.cache();
        assert_eq!(old_load(&cache, &source), new_load(&cache, &source));
        compare(
            &format!("load/{width}x{height}"),
            1,
            || old_load(black_box(&cache), black_box(&source)),
            || new_load(black_box(&cache), black_box(&source)),
        );
        compare(
            &format!("prewarm/{width}x{height}"),
            1,
            || old_ensure(black_box(&source), OPTIONS, black_box(&cache), HEX).unwrap(),
            || new_ensure(black_box(&source), OPTIONS, black_box(&cache), HEX).unwrap(),
        );
        compare(
            &format!("validate/{width}x{height}"),
            1,
            || baseline::load_raw_cached_banner_image(black_box(&cache)).map(drop),
            || validate_raw_cached_banner(black_box(&cache)),
        );
    }
    let old_prune = black_box(baseline::prune_stale_banner_cache_variants as fn(&Path, &str));
    let new_prune = black_box(prune_stale_banner_cache_variants as fn(&Path, &str));
    for count in [0, 1, 128, 512] {
        let tree = Tree::new();
        for i in 0..count {
            fs::write(tree.path.join(format!("unrelated-{i:04}.rgba")), b"fixture").unwrap();
        }
        let cache = tree.cache();
        compare(
            &format!("prune/unrelated{count}"),
            count.max(1),
            || old_prune(black_box(&cache), black_box(HEX)),
            || new_prune(black_box(&cache), black_box(HEX)),
        );
        if count == 128 {
            #[cfg(windows)]
            perf::assert_reduced_churn(|| old_prune(&cache, HEX), || new_prune(&cache, HEX));
        }
    }
    for count in [0, 128] {
        let tree = Tree::new();
        let cache = tree.cache();
        for i in 0..count {
            fs::write(tree.path.join(format!("unrelated-{i:04}.rgba")), b"fixture").unwrap();
        }
        let stale: Vec<_> = (0..4)
            .map(|i| tree.path.join(format!("{HEX}-{i}.rgba")))
            .collect();
        let reset = || {
            for path in &stale {
                fs::write(path, b"stale").unwrap();
            }
        };
        let name = format!("prune/delete4-mixed{count}");
        let run = |variant: &str, function: fn(&Path, &str)| {
            perf::measure_sampled_with_setup(
                &format!("{name}/{variant}"),
                16,
                count + 4,
                reset,
                |_| function(black_box(&cache), black_box(HEX)),
            );
        };
        if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            run("new", new_prune);
            run("old", old_prune);
        } else {
            run("old", old_prune);
            run("new", new_prune);
        }
    }
}
