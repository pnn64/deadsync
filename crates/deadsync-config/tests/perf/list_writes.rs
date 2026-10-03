use crate::cache::push_never_cache_list_option_line;
use crate::folders::{
    AdditionalSongFolder, additional_song_folder_paths, push_additional_song_folder_option_lines,
};
use crate::perf::{assert_reduced_churn, measure_sampled};
use std::hint::black_box;

mod baseline {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/perf/list_writes/baseline.rs"
    ));
}

fn write_lists(content: &mut String, folders: &[AdditionalSongFolder], list: &[String]) {
    push_additional_song_folder_option_lines(content, folders);
    push_never_cache_list_option_line(content, list);
}

fn original_write_lists(content: &mut String, folders: &[AdditionalSongFolder], list: &[String]) {
    baseline::push_additional_song_folder_option_lines(content, folders);
    baseline::push_never_cache_list_option_line(content, list);
}

fn fixture(count: usize, width: usize) -> (Vec<AdditionalSongFolder>, Vec<String>) {
    let folders = (0..count)
        .map(|i| AdditionalSongFolder {
            path: format!("D:/songs/日本語/{i:04}/{}", "a".repeat(width)),
            writable: i % 2 == 0,
        })
        .collect();
    let list = (0..count)
        .map(|i| format!("Pack {i:04} {}", "x".repeat(width)))
        .collect();
    (folders, list)
}

#[test]
fn list_writes_preserve_bytes_and_empty_entry_rules() {
    let tokens = [
        "",
        " ",
        "Pack",
        "日本語/Été",
        "a,b",
        "line\nnext",
        "x=y",
        "\r\n",
    ];
    for a in tokens {
        for b in tokens {
            for flags in 0..8 {
                let folders = [a, b, ""]
                    .into_iter()
                    .enumerate()
                    .map(|(i, path)| AdditionalSongFolder {
                        path: path.to_owned(),
                        writable: flags & (1 << i) != 0,
                    })
                    .collect::<Vec<_>>();
                let list = vec![a.to_owned(), b.to_owned(), String::new()];
                for prefix in ["", "[Options]\nExisting=1\n"] {
                    let mut old = prefix.to_owned();
                    let mut new = prefix.to_owned();
                    original_write_lists(&mut old, &folders, &list);
                    write_lists(&mut new, &folders, &list);
                    assert_eq!(old, new, "{a:?} {b:?} flags={flags}");
                }
                for writable in [false, true] {
                    assert_eq!(
                        additional_song_folder_paths(&folders, writable),
                        baseline::additional_song_folder_paths(&folders, writable)
                    );
                }
            }
        }
    }
    let mut old = String::new();
    let mut new = String::new();
    original_write_lists(&mut old, &[], &[]);
    write_lists(&mut new, &[], &[]);
    assert_eq!(old, new);
}

#[test]
fn list_writes_remove_temporary_allocations() {
    for (count, width) in [(1, 0), (8, 24), (64, 256)] {
        let (folders, list) = fixture(count, width);
        let capacity = 256
            + folders.iter().map(|f| f.path.len() + 1).sum::<usize>()
            + list.iter().map(|s| s.len() + 1).sum::<usize>();
        let mut old = String::with_capacity(capacity);
        let mut new = String::with_capacity(capacity);
        assert_reduced_churn(
            || original_write_lists(black_box(&mut old), black_box(&folders), black_box(&list)),
            || write_lists(black_box(&mut new), black_box(&folders), black_box(&list)),
        );
        assert_eq!(old, new);
    }
}

#[test]
#[ignore = "manual release CPU/allocation benchmark"]
fn list_writes_benchmark() {
    let original =
        black_box(original_write_lists as fn(&mut String, &[AdditionalSongFolder], &[String]));
    let current = black_box(write_lists as fn(&mut String, &[AdditionalSongFolder], &[String]));
    for (name, count, width, capacity) in [
        ("empty", 0, 0, 4096),
        ("one", 1, 0, 4096),
        ("eight", 8, 24, 4096),
        ("sixty-four", 64, 32, 16384),
        ("long", 64, 256, 65536),
        ("growing", 64, 256, 0),
    ] {
        let (folders, list) = fixture(count, width);
        let run = |variant: &str, f: fn(&mut String, &[AdditionalSongFolder], &[String])| {
            let mut content = String::with_capacity(capacity);
            measure_sampled(&format!("list-writes/{name}/{variant}"), 4096, 1, || {
                content.clear();
                if capacity == 0 {
                    content = String::new();
                }
                f(
                    black_box(&mut content),
                    black_box(&folders),
                    black_box(&list),
                );
                let len = black_box(content.len());
                if capacity == 0 {
                    drop(std::mem::take(&mut content));
                }
                len
            });
        };
        if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            run("current", current);
            run("original", original);
        } else {
            run("original", original);
            run("current", current);
        }
    }
}
