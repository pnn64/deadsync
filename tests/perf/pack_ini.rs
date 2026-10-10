use super::*;
use std::hint::black_box;

// Frozen from committed main 466815487; only the function name changes.
fn original_pack_ini_with_sync(existing: &str, pack_name: &str, itg: bool) -> String {
    let wanted = if itg { "ITG" } else { "NULL" };
    if existing.trim().is_empty() {
        // A minimal file the parser will accept. `Version` is not decoration:
        // without it the whole file is discarded and the sync value with it.
        return format!("[Group]\nVersion=1\nDisplayTitle={pack_name}\nSyncOffset={wanted}\n");
    }
    let newline = if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let sync_line = format!("SyncOffset={wanted}");

    let mut out: Vec<String> = Vec::with_capacity(existing.lines().count() + 3);
    let mut in_group = false;
    // Where the last `[Group]` section's last line is, to add keys after.
    let mut group_end: Option<usize> = None;
    let mut has_sync = false;
    let mut version_counts = false;
    for raw in existing.lines() {
        let line = raw.strip_prefix('\u{feff}').unwrap_or(raw).trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_group = line[1..line.len() - 1].trim().eq_ignore_ascii_case("group");
            out.push(raw.to_owned());
            if in_group {
                group_end = Some(out.len() - 1);
            }
            continue;
        }
        if in_group && !line.starts_with(';') && !line.starts_with('#') {
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                if key.eq_ignore_ascii_case("SyncOffset") {
                    out.push(sync_line.clone());
                    has_sync = true;
                    group_end = Some(out.len() - 1);
                    continue;
                }
                if key.eq_ignore_ascii_case("Version") {
                    version_counts = !value.trim().is_empty();
                }
            }
            out.push(raw.to_owned());
            if !line.is_empty() {
                group_end = Some(out.len() - 1);
            }
            continue;
        }
        out.push(raw.to_owned());
    }

    let mut missing = Vec::with_capacity(2);
    if !version_counts {
        missing.push("Version=1".to_owned());
    }
    if !has_sync {
        missing.push(sync_line);
    }
    match group_end {
        Some(at) => {
            out.splice(at + 1..at + 1, missing);
        }
        None => {
            out.push("[Group]".to_owned());
            out.extend(missing);
        }
    }
    let mut text = out.join(newline);
    text.push_str(newline);
    text
}

#[test]
fn pack_ini_borrowed_lines_match_original_edge_cases() {
    let cases = [
        "",
        " \t\r\n",
        "[Group]",
        "[Group]\nVersion=1\nSyncOffset=NULL",
        "\u{feff}[gRoUp]\r\nVersion=\r\n; comment\r\n# comment\r\n\r\n[Other]\r\nSyncOffset=ITG\r\n",
        "[Group]\nVersion=1\nSyncOffset=ITG\nSyncOffset=NULL\n[Group]\nVersion=\n\n;tail",
        "[Group]\nVersion=\n[Other]\nVersion=1\n[GROUP]\nVersion=2\n#tail\n",
        "key=outside\n[Other]\nSyncOffset=ITG\n",
        "[Group]\nVersion=1\r\n\nSyncOffset =NULL\r\n",
        "[Group]\rVersion=1\rSyncOffset=NULL",
        "[Group]\nTitle=é界🎵\n\u{feff}SyncOffset=NULL\n",
    ];
    for input in cases {
        for itg in [false, true] {
            assert_eq!(
                pack_ini_with_sync(input, "é界🎵", itg),
                original_pack_ini_with_sync(input, "é界🎵", itg),
                "{input:?}"
            );
        }
    }
}

#[test]
fn pack_ini_borrowed_lines_match_generated_sections() {
    let pieces = [
        "[Group]",
        "[Other]",
        "[ group ]",
        "Version=",
        "Version=1",
        "SyncOffset=custom",
        " ;note",
        "#note",
        "",
        "Title=é界🎵",
    ];
    for seed in 0..512usize {
        let mut rng = seed;
        let newline = if seed % 2 == 0 { "\n" } else { "\r\n" };
        let mut input = String::new();
        for _ in 0..48 {
            rng = rng.wrapping_mul(1664525).wrapping_add(1013904223);
            input.push_str(pieces[(rng >> 16) % pieces.len()]);
            input.push_str(newline);
        }
        for itg in [false, true] {
            assert_eq!(
                pack_ini_with_sync(&input, "Pack", itg),
                original_pack_ini_with_sync(&input, "Pack", itg),
                "seed {seed}"
            );
        }
    }
}

#[test]
fn pack_ini_line_count_does_not_multiply_allocations() {
    let input = format!("[Group]\nVersion=1\n{}", "DisplayTitle=Pack\n".repeat(1000));
    let (expected, before) =
        crate::perf::measure(|| original_pack_ini_with_sync(&input, "Pack", true));
    let (actual, after) = crate::perf::measure(|| pack_ini_with_sync(&input, "Pack", true));
    assert_eq!(actual, expected);
    assert_eq!(after.allocs, 2);
    assert_eq!(after.reallocs, 0);
    assert!(before.allocs > 1000);
    assert!(after.peak_added_bytes < before.peak_added_bytes);
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_runtime_traversal_pack_ini() {
    for (name, input) in [
        ("empty", String::new()),
        (
            "typical",
            "[Group]\nVersion=1\nDisplayTitle=Pack\nSeries=2026\nSyncOffset=NULL\n".into(),
        ),
        (
            "insert",
            "\u{feff}[Group]\r\nVersion=\r\nDisplayTitle=é界🎵\r\n\r\n[Other]\r\nKey=Kept\r\n"
                .into(),
        ),
        (
            "large",
            format!(
                "[Group]\nVersion=1\n{}",
                "Credit=Original pack contributor\n".repeat(1000)
            ),
        ),
    ] {
        assert_eq!(
            pack_ini_with_sync(&input, "Pack", true),
            original_pack_ini_with_sync(&input, "Pack", true)
        );
        crate::traversal_perf::compare(
            &format!("pack/{name}"),
            || {
                black_box(original_pack_ini_with_sync(
                    black_box(&input),
                    black_box("Pack"),
                    black_box(true),
                ));
            },
            || {
                black_box(pack_ini_with_sync(
                    black_box(&input),
                    black_box("Pack"),
                    black_box(true),
                ));
            },
        );
    }
}
