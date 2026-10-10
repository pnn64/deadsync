use super::*;
use crate::{paired_bench, perf};
use std::hint::black_box;

mod original {
    use super::*;
    include!("targets_original.rs");
    include!("update_original.rs");
}

fn context(rivals: usize) -> ArrowCloudUserContext {
    ArrowCloudUserContext {
        self_user_id: Some("self-player-00000000".into()),
        rival_user_ids: (0..rivals)
            .map(|i| format!("rival-player-{i:06}"))
            .collect(),
    }
}

fn entry(id: &str, is_self: bool, is_rival: bool) -> ArrowCloudLeaderboardEntry {
    ArrowCloudLeaderboardEntry {
        rank: 1,
        score: 99.0,
        alias: "Player".into(),
        date: String::new(),
        user_id: id.into(),
        is_self,
        is_rival,
    }
}

#[test]
fn borrowed_targets_preserve_identity_and_deduplication() {
    for self_id in [
        None,
        Some(""),
        Some("a"),
        Some("A"),
        Some(" a "),
        Some("\u{00e9}"),
    ] {
        let context = ArrowCloudUserContext {
            self_user_id: self_id.map(str::to_owned),
            rival_user_ids: ["", "a", "A", " a ", "\u{00e9}"].map(str::to_owned).into(),
        };
        for input in [None, Some(&context)] {
            let old = original::arrowcloud_target_user_ids(input);
            let new = arrowcloud_target_user_ids(input);
            assert_eq!(old.len(), new.len());
            assert!(old.iter().all(|id| new.contains(id.as_str())));
            for id in new {
                assert!(
                    context
                        .self_user_id
                        .as_deref()
                        .is_some_and(|s| std::ptr::eq(s, id))
                        || context
                            .rival_user_ids
                            .iter()
                            .any(|s| std::ptr::eq(s.as_str(), id))
                );
            }
        }
    }
}

#[test]
fn borrowed_pagination_preserves_each_page_and_early_stop() {
    let context = context(3);
    let pages = [
        vec![
            entry(" stranger ", true, true),
            entry("\u{2003}", true, true),
        ],
        vec![
            entry(" rival-player-000000 ", false, false),
            entry("RIVAL-PLAYER-000001", false, false),
        ],
        vec![
            entry("self-player-00000000", false, false),
            entry("rival-player-000000", true, true),
        ],
        vec![
            entry("rival-player-000001", false, false),
            entry("rival-player-000002", false, false),
        ],
        vec![entry("ignored-after-completion", false, false)],
    ];
    for input in [None, Some(&context)] {
        let mut old = original::arrowcloud_target_user_ids(input);
        let mut new = arrowcloud_target_user_ids(input);
        for page in &pages {
            original::update_remaining_targets(page, input, &mut old);
            update_remaining_targets(page, input, &mut new);
            assert_eq!(old.len(), new.len());
            assert!(old.iter().all(|id| new.contains(id.as_str())));
        }
        assert!(new.is_empty());
    }
    // Public callers can also supply targets absent from the context.
    for flags in [(false, false), (true, false), (false, true)] {
        let mut old: HashSet<String> = ["external".into()].into();
        let mut new: HashSet<&str> = ["external"].into();
        let page = [entry(" external ", flags.0, flags.1)];
        original::update_remaining_targets(&page, None, &mut old);
        update_remaining_targets(&page, None, &mut new);
        assert_eq!(old.is_empty(), new.is_empty());
    }
}

#[test]
fn borrowed_targets_allocate_only_the_set() {
    let context = context(512);
    let (_, old) = perf::measure(|| original::arrowcloud_target_user_ids(Some(&context)));
    let (targets, new) = perf::measure(|| arrowcloud_target_user_ids(Some(&context)));
    assert_eq!(old.allocs, 514);
    assert_eq!(new.allocs, 1);
    assert_eq!(new.reallocs, 0);
    assert!(new.allocated_bytes < old.allocated_bytes);
    let page = [entry("self-player-00000000", false, false)];
    let mut targets = targets;
    perf::assert_no_churn(|| update_remaining_targets(&page, Some(&context), &mut targets));
}

#[test]
#[ignore = "paired performance benchmark"]
fn benchmark_borrowed_pagination() {
    for count in [0, 8, 64, 512] {
        let context = context(count);
        let page: Vec<_> = (0..count)
            .step_by(2)
            .map(|i| entry(&format!("rival-player-{i:06}"), false, false))
            .collect();
        for paginate in [false, true] {
            let label = format!(
                "{}-{count}",
                if paginate { "pagination" } else { "targets" }
            );
            let work = |current| {
                if current {
                    let mut targets = arrowcloud_target_user_ids(black_box(Some(&context)));
                    if paginate {
                        update_remaining_targets(black_box(&page), Some(&context), &mut targets);
                    }
                    black_box(targets);
                } else {
                    let mut targets =
                        original::arrowcloud_target_user_ids(black_box(Some(&context)));
                    if paginate {
                        original::update_remaining_targets(
                            black_box(&page),
                            Some(&context),
                            &mut targets,
                        );
                    }
                    black_box(targets);
                }
            };
            let (_, old) = perf::measure(|| work(false));
            let (_, new) = perf::measure(|| work(true));
            println!("{label} allocations: original {old:?}, current {new:?}");
            paired_bench::compare(&label, 5_000, work);
        }
    }
}
