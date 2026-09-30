//! Paired short-query matcher measurements against 0.5.1649.
#[allow(dead_code)]
#[path = "one_edit/baseline.rs"]
mod baseline;
#[allow(dead_code)]
#[path = "../src/screens/components/shared/fuzzy.rs"]
mod current;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;
use std::hint::black_box;

fn compare(query: &str, label: &str, aliases: &[&str]) {
    let old = baseline::prepare_query(query);
    let new = current::prepare_query(query);
    assert_eq!(
        baseline::best_match_score(&old, label, aliases),
        current::best_match_score(&new, label, aliases),
        "{query:?} / {label:?}"
    );
    if query.chars().count() <= 80 && label.chars().count() <= 96 {
        perf::assert_no_churn(|| {
            black_box(current::best_match_score(&new, label, aliases));
        });
    }
}

#[test]
fn short_query_typo_scores_preserve_edits_words_aliases_and_unicode() {
    let mut words = vec![String::new()];
    for len in 1..=5 {
        for bits in 0..1usize << len {
            words.push(
                (0..len)
                    .map(|i| if bits & (1 << i) == 0 { 'a' } else { 'b' })
                    .collect(),
            );
        }
    }
    for query in &words {
        for label in &words {
            compare(query, label, &[]);
        }
    }
    for label in [
        "Speed",
        "Skin",
        "Glow",
        "deja",
        "日本語",
        "Σtep",
        "İab",
        "a\0b",
        "éabc",
    ] {
        let chars: Vec<_> = label.chars().collect();
        for i in 0..=chars.len() {
            for ch in ['x', ' ', '\u{2003}', '日', '\u{301}'] {
                let mut edited = chars.clone();
                edited.insert(i, ch);
                let query: String = edited.iter().collect();
                compare(&query, label, &[]);
                compare(label, &query, &["alias", "abc"]);
                if i < chars.len() {
                    edited = chars.clone();
                    edited[i] = ch;
                    compare(&edited.iter().collect::<String>(), label, &[]);
                    edited.remove(i);
                    compare(&edited.iter().collect::<String>(), label, &[]);
                }
            }
        }
    }
    for query in ["abx", "abxd", "abcxe", "abcxefgy", ""] {
        for label in [
            "abc abcxe",
            "axc abc",
            "long unrelated title abcde",
            "abx alias",
        ] {
            for aliases in [&[][..], &["abc", "abx"][..]] {
                compare(query, label, aliases);
            }
        }
    }
}

#[test]
#[ignore = "manual release benchmark"]
fn benchmark_one_edit() {
    let labels: Vec<_> = (0..4096)
        .map(|i| {
            format!(
                "{} {:04} {}",
                [
                    "Speed", "Skin", "Glow", "Step", "Song", "Wave", "Dance", "Night"
                ][i % 8],
                i,
                ["Mix", "Remix", "Edit"][i % 3]
            )
        })
        .collect();
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for text in ["spxed", "glxw", "zzzz", "spe", "perspextive"] {
        let old_query = baseline::prepare_query(text);
        let new_query = current::prepare_query(text);
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            perf::measure_sampled(
                &format!("matcher_{text}/{}", if old { "old" } else { "new" }),
                64,
                labels.len(),
                || {
                    let mut checksum = 0i64;
                    for label in black_box(&labels) {
                        let score = if old {
                            baseline::best_match_score(black_box(&old_query), black_box(label), &[])
                        } else {
                            current::best_match_score(black_box(&new_query), black_box(label), &[])
                        };
                        checksum += i64::from(score.unwrap_or(0));
                    }
                    checksum
                },
            );
        }
    }
}
