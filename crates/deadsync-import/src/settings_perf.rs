use super::*;
use crate::{paired_bench, perf_alloc};
use std::{
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
};

#[path = "settings_original.rs"]
mod original;

struct Fixture(PathBuf);

impl Fixture {
    fn new(content: Option<&[u8]>) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "deadsync-settings-perf-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create unique fixture directory");
        if let Some(content) = content {
            fs::write(path.join("Simply Love UserPrefs.ini"), content).unwrap();
        }
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove owned fixture directory");
    }
}

fn preferences(count: usize) -> String {
    let mut text = "[Other]\nIgnored=retained in parser only\n[Simply Love]\n".to_owned();
    for i in 0..count {
        use std::fmt::Write;
        writeln!(&mut text, "Preference{i}=Value with spaces {i}").unwrap();
    }
    text
}

#[test]
fn moved_settings_match_original_files_and_duplicate_section_semantics() {
    for content in [None, Some(&b""[..]), Some(&b"[Other]\na=b\n"[..]), Some(&b"[Simply Love]\n"[..]), Some(&b"bad utf8: \xff"[..]), Some(&b"root=keep\n[Simply Love]\nX=first\nEmpty=\n[Other]\nX=other\n[Simply Love]\nX=last\n=empty-key\n[simply love]\nX=lower\n"[..]), Some("[Simply Love]\nUnicode=\u{00e4}\u{6771}\nSpeedModType=C\nSpeedMod=400\n".as_bytes())] {
        let fixture = Fixture::new(content);
        assert_eq!(read_simply_love(&fixture.0), original::read_simply_love(&fixture.0));
    }
}

#[test]
fn settings_transfer_removes_key_and_value_clone_allocations() {
    let text = preferences(128);
    let fixture = Fixture::new(Some(text.as_bytes()));
    let (before, old_churn) = perf_alloc::measure(|| original::read_simply_love(&fixture.0));
    let (after, new_churn) = perf_alloc::measure(|| read_simply_love(&fixture.0));
    assert_eq!(before, after);
    assert_eq!(
        old_churn.allocs - new_churn.allocs,
        256,
        "{old_churn:?} -> {new_churn:?}"
    );
    let copied_bytes: usize = before
        .iter()
        .map(|(key, value)| key.len() + value.len())
        .sum();
    assert_eq!(
        old_churn.allocated_bytes - new_churn.allocated_bytes,
        copied_bytes
    );
}

#[test]
#[ignore = "paired release benchmark; warm filesystem reads"]
fn benchmark_settings_import() {
    for count in [32, 128] {
        let text = preferences(count);
        let fixture = Fixture::new(Some(text.as_bytes()));
        for current in [false, true] {
            let (_, churn) = perf_alloc::measure(|| {
                if current {
                    read_simply_love(&fixture.0)
                } else {
                    original::read_simply_love(&fixture.0)
                }
            });
            println!("settings-{count} current={current}: {churn:?}");
        }
        paired_bench::compare(&format!("settings-{count}"), 1024, |current| {
            black_box(if current {
                read_simply_love(black_box(&fixture.0))
            } else {
                original::read_simply_love(black_box(&fixture.0))
            });
        });
    }
}
