use super::*;
use crate::{paired_bench, perf};
use std::hint::black_box;

#[path = "smo_search_original.rs"]
mod original;

// Only these tests touch the search runtime; keep the snapshot owned by each
// fixture and restore it without starting any network workers.
static RUNTIME_TEST: Mutex<()> = Mutex::new(());

struct RuntimeFixture {
    saved: Option<RuntimeState>,
    _serial: MutexGuard<'static, ()>,
}

impl RuntimeFixture {
    fn new() -> Self {
        let serial = RUNTIME_TEST.lock().unwrap();
        let saved = std::mem::take(&mut *lock_runtime());
        Self {
            saved: Some(saved),
            _serial: serial,
        }
    }

    fn install(&self, query: &str, phase: SearchPhase) {
        *lock_runtime() = RuntimeState {
            generation: u64::MAX,
            snapshot: Arc::new(SearchSnapshot {
                phase,
                query: query.to_owned(),
                hits: Arc::from([SearchHit {
                    pack_id: 19,
                    score: 80,
                    why: "pack name".to_owned(),
                }]),
                capped: true,
                revision: u64::MAX,
                message: Some("previous status".to_owned()),
            }),
        };
    }
}

impl Drop for RuntimeFixture {
    fn drop(&mut self) {
        *lock_runtime() = self.saved.take().unwrap();
    }
}

#[test]
fn repeated_queries_preserve_the_snapshot_without_allocation() {
    let fixture = RuntimeFixture::new();
    for phase in [SearchPhase::Loading, SearchPhase::Ready] {
        for query in ["", "a", "rosewood", "caf\u{e9}", "\u{97f3}\u{697d}"] {
            fixture.install(query, phase);
            let before = runtime_snapshot();
            for input in [query.to_owned(), format!("\u{2003}\t{query}\r\n\u{3000}")] {
                let (_, old) = perf::measure(|| original::runtime_search(&input));
                assert_eq!(old.allocs, usize::from(!query.is_empty()));
                perf::assert_no_churn(|| runtime_search(&input));
                assert!(Arc::ptr_eq(&before, &runtime_snapshot()));
                assert_eq!(lock_runtime().generation, u64::MAX);
            }
        }
    }
}

#[test]
fn short_query_transitions_match_the_original_including_revision_wrap() {
    let fixture = RuntimeFixture::new();
    for phase in [
        SearchPhase::Idle,
        SearchPhase::Loading,
        SearchPhase::Ready,
        SearchPhase::Error,
    ] {
        for query in ["", " \t\r\n", "a", "\u{2003}\u{e9}\u{3000}", "\u{1f9e9}"] {
            fixture.install("previous", phase);
            original::runtime_search(query);
            let expected = runtime_snapshot();
            let expected_generation = lock_runtime().generation;
            fixture.install("previous", phase);
            runtime_search(query);
            let actual = runtime_snapshot();
            assert_eq!(actual.phase, expected.phase);
            assert_eq!(actual.query, expected.query);
            assert_eq!(actual.hits, expected.hits);
            assert_eq!(actual.capped, expected.capped);
            assert_eq!(actual.revision, expected.revision);
            assert_eq!(actual.message, expected.message);
            assert_eq!(lock_runtime().generation, expected_generation);
            assert_eq!(actual.phase, SearchPhase::Ready);
            assert_eq!(actual.query, query.trim());
            assert!(actual.hits.is_empty());
            assert!(!actual.capped);
            assert_eq!(actual.revision, 1);
            assert_eq!(expected_generation, 0);
        }
    }
}

fn search_fixture(
    count: usize,
    scores: usize,
    dates: usize,
) -> (Accumulator, smo_details::DetailsSnapshot) {
    let mut by_id = std::collections::HashMap::with_capacity(count);
    let mut hits = Vec::with_capacity(count);
    for i in 0..count {
        let pack_id = i as u64;
        hits.push(SearchHit {
            pack_id,
            score: match scores {
                0 => 100,
                1 => (i % 7) as i32 * 20,
                _ => i as i32,
            },
            why: format!("reason {i}"),
        });
        if dates != 0 && (dates != 3 || i % 5 != 0) {
            let date_added = match dates {
                1 => None,
                3 if i % 5 == 1 => Some(String::new()),
                3 if i % 5 == 2 => Some("\u{00e9}\u{65e5}".to_owned()),
                _ => Some(format!(
                    "20{:02}-{:02}-{:02}",
                    i % 26,
                    i % 12 + 1,
                    i % 28 + 1
                )),
            };
            by_id.insert(
                pack_id,
                smo_details::PackDetails {
                    date_added,
                    ..smo_details::PackDetails::default()
                },
            );
        }
    }
    // Deterministic shuffle includes both ascending and descending runs.
    let mut seed = 13_u64;
    for i in (1..count).rev() {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        hits.swap(i, seed as usize % (i + 1));
    }
    (
        Accumulator { hits, capped: true },
        smo_details::DetailsSnapshot {
            by_id: Arc::new(by_id),
            ..smo_details::DetailsSnapshot::default()
        },
    )
}

fn copy_accumulator(acc: &Accumulator) -> Accumulator {
    Accumulator {
        hits: acc.hits.clone(),
        capped: acc.capped,
    }
}

#[test]
fn ordering_and_caps_match_for_missing_empty_unicode_and_tied_dates() {
    for count in [0, 1, 2, 7, 20, 200, 201, 1200] {
        for scores in 0..3 {
            for dates in 0..4 {
                let (mut actual, details) = search_fixture(count, scores, dates);
                let mut expected = copy_accumulator(&actual);
                original::sort_and_cap(&mut expected, &details);
                actual.sort_and_cap(&details);
                assert_eq!(actual.hits, expected.hits, "{count}/{scores}/{dates}");
                assert_eq!(actual.capped, count > MAX_ROWS);
                assert_eq!(actual.capped, expected.capped);
                assert_eq!(actual.hits.len(), count.min(MAX_ROWS));
            }
        }
    }
    let (mut actual, details) = search_fixture(7, 0, 2);
    let mut duplicates = actual.hits.clone();
    for hit in &mut duplicates {
        hit.why = "later equal key".to_owned();
    }
    actual.hits.extend(duplicates);
    let mut expected = copy_accumulator(&actual);
    original::sort_and_cap(&mut expected, &details);
    actual.sort_and_cap(&details);
    assert_eq!(
        actual.hits, expected.hits,
        "equal keys retain stable input order"
    );
}

#[test]
fn ordering_borrows_dates_without_per_comparison_allocations() {
    let (mut current, details) = search_fixture(200, 0, 2);
    let mut before = copy_accumulator(&current);
    let (_, old) = perf::measure(|| original::sort_and_cap(&mut before, &details));
    let (_, new) = perf::measure(|| current.sort_and_cap(&details));
    assert_eq!(current.hits, before.hits);
    assert!(old.allocs > 200, "{old:?}");
    assert!(
        new.allocs <= 1,
        "only the stable-sort scratch buffer remains: {new:?}"
    );
    assert_eq!(new.reallocs, 0);
}

#[test]
fn encoded_queries_match_all_ascii_and_utf8_boundaries() {
    for byte in 0_u8..=127 {
        let input = char::from(byte).to_string();
        assert_eq!(percent_encode(&input), original::percent_encode(&input));
    }
    for codepoint in [
        0x80, 0x7ff, 0x800, 0xd7ff, 0xe000, 0xffff, 0x10000, 0x10ffff,
    ] {
        let input = char::from_u32(codepoint).unwrap().to_string();
        assert_eq!(percent_encode(&input), original::percent_encode(&input));
    }
    let ascii: String = (0_u8..=127).map(char::from).collect();
    for input in [
        String::new(),
        ascii,
        "A_z-09.~".repeat(50),
        "\u{97f3}\u{697d} & caf\u{e9}? #1 +%/".repeat(40),
    ] {
        let expected = original::percent_encode(&input);
        let (actual, churn) = perf::measure(|| percent_encode(&input));
        assert_eq!(actual, expected);
        assert_eq!(churn.allocs, usize::from(!input.is_empty()));
        assert!(actual.is_ascii());
    }
    assert_eq!(percent_encode("+%/"), "%2B%25%2F");
    assert_eq!(percent_encode("\u{97f3}\u{697d}"), "%E9%9F%B3%E6%A5%BD");
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_repeated_search_requests() {
    let fixture = RuntimeFixture::new();
    for (name, phase, query, input) in [
        ("empty", SearchPhase::Ready, "", ""),
        ("ready", SearchPhase::Ready, "rosewood", "rosewood"),
        ("loading", SearchPhase::Loading, "rosewood", "rosewood"),
        (
            "trimmed",
            SearchPhase::Ready,
            "rosewood",
            "\u{2003} rosewood \u{3000}",
        ),
        (
            "unicode",
            SearchPhase::Ready,
            "\u{97f3}\u{697d}",
            "\u{97f3}\u{697d}",
        ),
    ] {
        fixture.install(query, phase);
        let (_, old) = perf::measure(|| original::runtime_search(black_box(input)));
        let (_, new) = perf::measure(|| runtime_search(black_box(input)));
        println!("repeat/{name} churn: original {old:?}; current {new:?}");
        paired_bench::compare(&format!("repeat/{name}"), 100_000, |current| {
            if current {
                runtime_search(black_box(input));
            } else {
                original::runtime_search(black_box(input));
            }
        });
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_search_ordering() {
    for (name, count, scores, dates, iterations) in [
        ("empty", 0, 0, 2, 100_000),
        ("single", 1, 0, 2, 20_000),
        ("visible-dated", 7, 0, 2, 5_000),
        ("page-dated", 200, 0, 2, 500),
        ("page-mixed", 200, 1, 3, 500),
        ("page-undated", 200, 0, 0, 500),
        ("page-distinct-scores", 200, 2, 2, 500),
        ("broad-dated", 1200, 0, 2, 100),
        ("catalogue-dated", 9000, 0, 2, 10),
    ] {
        let (acc, details) = search_fixture(count, scores, dates);
        let mut before = copy_accumulator(&acc);
        let mut after = copy_accumulator(&acc);
        let (_, old) = perf::measure(|| original::sort_and_cap(&mut before, &details));
        let (_, new) = perf::measure(|| after.sort_and_cap(&details));
        assert_eq!(before.hits, after.hits);
        println!("ordering/{name} churn: original {old:?}; current {new:?}");
        paired_bench::compare_prepared(
            &format!("ordering/{name}"),
            iterations,
            || copy_accumulator(&acc),
            |mut input, current| {
                if current {
                    black_box(&mut input).sort_and_cap(black_box(&details));
                } else {
                    original::sort_and_cap(black_box(&mut input), black_box(&details));
                }
                black_box(input);
            },
        );
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_percent_encoding() {
    for (name, input) in [
        ("empty", String::new()),
        ("unreserved", "rosewood-09_~.pack".to_owned()),
        ("spaces", "ITG Tournament Pack 2026".to_owned()),
        ("punctuation", "Tom & Jerry's #1 Pack? +%/".to_owned()),
        ("unicode", "\u{97f3}\u{697d} caf\u{e9} \u{1f3b5}".to_owned()),
        ("long", "\u{97f3}\u{697d} & caf\u{e9}? #1 +%/".repeat(32)),
    ] {
        let (expected, old) = perf::measure(|| original::percent_encode(black_box(&input)));
        let (actual, new) = perf::measure(|| percent_encode(black_box(&input)));
        assert_eq!(actual, expected);
        println!("encoding/{name} churn: original {old:?}; current {new:?}");
        paired_bench::compare(&format!("encoding/{name}"), 20_000, |current| {
            if current {
                black_box(percent_encode(black_box(&input)));
            } else {
                black_box(original::percent_encode(black_box(&input)));
            }
        });
    }
}
