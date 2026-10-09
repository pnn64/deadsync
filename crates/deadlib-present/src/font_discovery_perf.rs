use super::*;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "font_discovery_original.rs"]
mod original;
#[path = "../../../tests/perf/core_lookup_support.rs"]
mod support;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir().canonicalize().unwrap();
        let path = base.join(format!(
            "deadsync-font-discovery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert_eq!(path.parent(), Some(base.as_path()));
        // create_dir deliberately fails rather than reusing an existing directory.
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn file(&self, name: &str) {
        fs::write(self.0.join(name), []).unwrap();
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn font_page_discovery_preserves_filtering_sorting_and_errors() {
    let dir = Directory::new();
    for name in [
        "font [main].png",
        "font [z].PNG",
        "font-stroke.png",
        "font [other].png",
        "Font.png",
        "font.png.txt",
        "font.ini",
        "unrelated.png",
        "font 雪.png",
        "font.PNG",
        "not-a-font.txt",
    ] {
        dir.file(name);
    }
    fs::create_dir(dir.0.join("font [directory].png")).unwrap();
    for prefix in ["font", "Font", "", "missing", "font 雪"] {
        let expected = original::list_texture_pages(&dir.0, prefix).unwrap();
        assert_eq!(list_texture_pages(&dir.0, prefix).unwrap(), expected);
    }
    for path in [dir.0.join("absent"), dir.0.join("font.ini")] {
        assert_eq!(
            list_texture_pages(&path, "font").unwrap_err().kind(),
            original::list_texture_pages(&path, "font")
                .unwrap_err()
                .kind()
        );
    }
}

#[test]
#[cfg_attr(windows, ignore = "requires Windows symlink privilege")]
fn font_page_discovery_follows_file_symlinks() {
    let dir = Directory::new();
    dir.file("target.bin");
    fs::create_dir(dir.0.join("target-dir")).unwrap();
    #[cfg(windows)]
    let link = std::os::windows::fs::symlink_file;
    #[cfg(unix)]
    let link = std::os::unix::fs::symlink;
    link(dir.0.join("target.bin"), dir.0.join("font-link.png")).unwrap();
    link(dir.0.join("missing"), dir.0.join("font-broken.png")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(dir.0.join("target-dir"), dir.0.join("font-directory.png"))
        .unwrap();
    #[cfg(unix)]
    link(dir.0.join("target-dir"), dir.0.join("font-directory.png")).unwrap();
    let actual = list_texture_pages(&dir.0, "font").unwrap();
    assert_eq!(
        actual,
        original::list_texture_pages(&dir.0, "font").unwrap()
    );
    assert_eq!(actual, [dir.0.join("font-link.png")]);
}

#[test]
#[ignore = "paired release benchmark with warm filesystem cache"]
fn benchmark_core_lookups() {
    for count in [8, 64, 512, 2048] {
        let dir = Directory::new();
        for i in 0..count {
            dir.file(&format!(
                "{} [{i}].{}",
                if i < 8 { "font" } else { "unrelated" },
                if i % 3 == 0 { "txt" } else { "png" }
            ));
        }
        assert_eq!(
            list_texture_pages(&dir.0, "font").unwrap(),
            original::list_texture_pages(&dir.0, "font").unwrap()
        );
        for prefix in ["font", "", "missing"] {
            support::compare(
                &format!("font/{count}/prefix={prefix:?}"),
                1,
                || {
                    black_box(
                        original::list_texture_pages(black_box(&dir.0), black_box(prefix)).unwrap(),
                    );
                },
                || {
                    black_box(list_texture_pages(black_box(&dir.0), black_box(prefix)).unwrap());
                },
            );
        }
    }
}
