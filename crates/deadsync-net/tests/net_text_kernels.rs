use deadsync_net::{is_timeout_message, log_body_snippet};
use std::hint::black_box;

#[path = "../../../tests/perf/core_alloc.rs"]
mod alloc;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/text_original.rs"]
mod original;

#[test]
fn timeout_search_preserves_case_controls_boundaries_and_unicode() {
    for base in ["timeout", "timed out"] {
        for mask in 0..1usize << base.len() {
            let text: String = base
                .bytes()
                .enumerate()
                .map(|(i, b)| {
                    char::from(if mask & (1 << i) != 0 {
                        b.to_ascii_uppercase()
                    } else {
                        b
                    })
                })
                .collect();
            for text in [
                text.clone(),
                format!("\u{96ea}prefix{text}suffix"),
                format!("ttttt{text}"),
            ] {
                assert_eq!(
                    is_timeout_message(&text),
                    original::is_timeout_message(&text),
                    "{text:?}"
                );
            }
        }
        for i in 0..base.len() {
            for byte in 0..=127 {
                let mut bytes = base.as_bytes().to_vec();
                bytes[i] = byte;
                let text = String::from_utf8(bytes).unwrap();
                assert_eq!(
                    is_timeout_message(&text),
                    original::is_timeout_message(&text),
                    "{text:?}"
                );
            }
            for ch in ["\u{130}", "\u{96ea}", "\u{2003}", "\u{e9}"] {
                let text = format!("{}{ch}{}", &base[..i], &base[i + 1..]);
                assert_eq!(
                    is_timeout_message(&text),
                    original::is_timeout_message(&text),
                    "{text:?}"
                );
            }
        }
    }
    for text in [
        "",
        "t",
        "ti",
        "time",
        "timed",
        "time out",
        "timed outside",
        "ttimeout",
        "t\0imeout",
    ] {
        assert_eq!(is_timeout_message(text), original::is_timeout_message(text));
    }
}

#[test]
fn snippet_preserves_exact_characters_and_owned_capacity() {
    for unit in [
        "a",
        "\0",
        "\u{e9}",
        "\u{96ea}",
        "\u{1f600}",
        "a\u{301}",
        "a\r\n",
    ] {
        for len in [0, 1, 63, 64, 127, 128, 255, 256, 257, 511, 1024] {
            for prefix in ["", "text-prefix ", "\u{96ea}"] {
                let text = format!("{prefix}{}", unit.repeat(len));
                let mut owned = String::with_capacity(text.len() + 1024);
                owned.push_str(&text);
                let expected = original::log_body_snippet(owned.clone());
                let capacity = owned.capacity();
                let actual = log_body_snippet(owned);
                assert_eq!(actual, expected);
                assert_eq!(actual.capacity(), capacity);
                assert!(actual.chars().count() <= 256);
            }
        }
    }
    for first_non_ascii in 250..=258 {
        let text = format!(
            "{}\u{1f600}{}",
            "a".repeat(first_non_ascii),
            "tail".repeat(100)
        );
        assert_eq!(
            log_body_snippet(text.clone()),
            original::log_body_snippet(text)
        );
    }
}

fn compare(label: &str, mut work: impl FnMut(bool)) {
    for current in [false, true] {
        let (_, counts) = alloc::measure(|| work(current));
        assert_eq!(counts, alloc::Churn::default());
        println!(
            "{label}/{}: allocation {counts:?}",
            if current { "current" } else { "original" }
        );
    }
    bench::compare(label, 300, work);
}

// Give both implementations the same non-inlined call boundary.
#[inline(never)]
fn current_timeout(text: &str) -> bool {
    is_timeout_message(text)
}

#[inline(never)]
fn original_timeout(text: &str) -> bool {
    original::is_timeout_message(text)
}

#[inline(never)]
fn current_snippet(text: String) -> String {
    log_body_snippet(text)
}

#[inline(never)]
fn original_snippet(text: String) -> String {
    original::log_body_snippet(text)
}

#[test]
#[ignore = "release paired CPU/allocation benchmark"]
fn benchmark_net_text_kernels() {
    for (kind, choices) in [
        (
            "matches",
            vec![
                "request timed out",
                "Timeout reading body",
                "prefix TiMeD OuT suffix",
                "HTTP TIMEOUT",
            ],
        ),
        (
            "missing",
            vec![
                "connection refused",
                "HTTP status 404",
                "unexpected connection reset",
                "time out",
            ],
        ),
        (
            "dense",
            vec![
                "tttttttttttttttttttttttttttttttt",
                "this text takes thirty test tokens to try",
            ],
        ),
    ] {
        let inputs: Vec<_> = (0..256).map(|i| choices[i % choices.len()]).collect();
        compare(&format!("net/timeout/{kind}/256"), |current| {
            let check: fn(&str) -> bool = black_box(if current {
                current_timeout
            } else {
                original_timeout
            });
            for text in black_box(&inputs) {
                black_box(check(black_box(text)));
            }
        });
    }
    for (kind, text) in [
        (
            "short",
            "short response with a brief error message".repeat(2),
        ),
        ("ascii", "server response content ".repeat(100)),
        ("unicode_short", "\u{96ea}\u{1f600}\u{e9}".repeat(64)),
        ("unicode_long", "\u{96ea}\u{1f600}\u{e9}".repeat(300)),
    ] {
        let mut scratch = String::with_capacity(text.len());
        compare(&format!("net/snippet/{kind}/256"), |current| {
            let clip: fn(String) -> String = black_box(if current {
                current_snippet
            } else {
                original_snippet
            });
            for _ in 0..256 {
                scratch.clear();
                scratch.push_str(black_box(&text));
                scratch = clip(black_box(std::mem::take(&mut scratch)));
                black_box(&scratch);
            }
        });
    }
}
