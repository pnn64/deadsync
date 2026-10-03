use super::*;
use crate::perf::{assert_reduced_churn, measure_sampled};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/simfile_titles/baseline.rs"
    ));
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "deadsync-titles-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, bytes: &[u8]) {
        fs::write(self.0.join(name), bytes).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn reused_title_lines_preserve_limits_newlines_utf8_errors_and_file_selection() {
    let fixture = Fixture::new();
    for ending in ["\n", "\r\n", ""] {
        for index in [0, 1, 31, 63, 64, 65] {
            let prefix = "#ARTIST: Artist;\n".repeat(index);
            fixture.write(
                "chart.SSC",
                format!("{prefix}  #TITLE:[14] 日本語 (SX);{ending}").as_bytes(),
            );
            fixture.write("audio.ogg", b"#TITLE:ignored;");
            let mut old = HashSet::from(["existing".to_owned()]);
            let mut new = old.clone();
            baseline::collect_simfile_titles(&fixture.0, &mut old);
            collect_simfile_titles(&fixture.0, &mut new);
            assert_eq!(old, new);
            assert_eq!(new.contains("日本語"), index < 64);
        }
    }
    for bytes in [
        &b"#TITLE:First;\n#TITLE:Second;\n"[..],
        &b"#ARTIST:\xff\n#TITLE:After error;\n"[..],
        &b"#TITLE:Before error;\n\xff"[..],
        &b"#TITLE:Trailing CR;\r"[..],
        &b"#title:lowercase;\n"[..],
        &b""[..],
    ] {
        fixture.write("chart.SSC", bytes);
        let mut old = HashSet::new();
        let mut new = HashSet::new();
        baseline::collect_simfile_titles(&fixture.0, &mut old);
        collect_simfile_titles(&fixture.0, &mut new);
        assert_eq!(old, new);
    }
    fs::create_dir(fixture.0.join("directory.sm")).unwrap();
    fixture.write("other.sm", b"#TITLE:Other;\n");
    let mut old = HashSet::new();
    let mut new = HashSet::new();
    baseline::collect_simfile_titles(&fixture.0, &mut old);
    collect_simfile_titles(&fixture.0, &mut new);
    assert_eq!(old, new);
    assert!(new.contains("other"));
    collect_simfile_titles(&fixture.0.join("absent"), &mut new);
    assert_eq!(old, new);
}

#[test]
fn title_scans_reuse_one_buffer_across_header_lines() {
    let fixture = Fixture::new();
    fixture.write(
        "chart.sm",
        format!("{}#TITLE:Title;\n", "#ARTIST: Artist;\n".repeat(63)).as_bytes(),
    );
    assert_reduced_churn(
        || {
            let mut keys = HashSet::new();
            baseline::collect_simfile_titles(black_box(&fixture.0), &mut keys);
            drop(black_box(keys));
        },
        || {
            let mut keys = HashSet::new();
            collect_simfile_titles(black_box(&fixture.0), &mut keys);
            drop(black_box(keys));
        },
    );
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn simfile_titles_benchmark() {
    let old = black_box(baseline::collect_simfile_titles as fn(&Path, &mut HashSet<String>));
    let new = black_box(collect_simfile_titles as fn(&Path, &mut HashSet<String>));
    for (name, prefix, title) in [
        ("first", String::new(), "#TITLE:Title;\n"),
        ("middle", "#ARTIST: Artist;\n".repeat(31), "#TITLE:Title;\n"),
        ("last", "#ARTIST: Artist;\n".repeat(63), "#TITLE:Title;\n"),
        ("missing", "#ARTIST: Artist;\n".repeat(64), ""),
        (
            "long-unicode",
            format!("#ARTIST:{};\n", "日本語".repeat(128)).repeat(63),
            "#TITLE:日本語;\n",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.write("chart.ssc", format!("{prefix}{title}").as_bytes());
        let original = || {
            measure_sampled(&format!("title-lines/{name}/original"), 128, 1, || {
                let mut keys = HashSet::new();
                old(black_box(&fixture.0), &mut keys);
                keys
            })
        };
        let current = || {
            measure_sampled(&format!("title-lines/{name}/current"), 128, 1, || {
                let mut keys = HashSet::new();
                new(black_box(&fixture.0), &mut keys);
                keys
            })
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            current();
            original();
        } else {
            original();
            current();
        }
    }
}
