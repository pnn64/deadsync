use super::*;
use std::hint::black_box;

#[path = "perf_original.rs"]
#[allow(dead_code)]
mod original;
use original::{OriginalIniData, OriginalRuntimeCache};

#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod allocations;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

fn metrics_corpus() -> Vec<(PathBuf, String)> {
    fn collect(dir: &Path, result: &mut Vec<(PathBuf, String)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(&path, result);
            } else if path.file_name().unwrap() == "metrics.ini" {
                result.push((path.clone(), fs::read_to_string(path).unwrap()));
            }
        }
    }
    let mut result = Vec::new();
    collect(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/noteskins"),
        &mut result,
    );
    result.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!result.is_empty());
    result
}

fn assert_parse_matches(content: &str) {
    assert_eq!(
        IniData::parse(content).sections,
        OriginalIniData::parse(content).sections,
        "input: {content:?}"
    );
}

#[test]
fn parser_matches_original_for_assets_and_generated_sections() {
    for (_, content) in metrics_corpus() {
        assert_parse_matches(&content);
    }
    for content in [
        "",
        "\n; comment\n# comment",
        "[Empty]",
        "[ ]",
        "[]=value",
        "A=first\n[ ]\na=last",
        "[Same]\nKey=one\n[sAME]\nKEY=two",
        "[Bad\nx=y\n=empty-key\n[]\nx=last",
        "[É]\nÜ=x\n[é]\nü=y",
        "\u{2003}[ Mixed ]\r\nKey= value = remainder ; # literal \u{2003}\r\n",
    ] {
        assert_parse_matches(content);
    }
    let lines = [
        "",
        "; ignored",
        "# ignored",
        "[Alpha]",
        "[ALPHA]",
        "[ ]",
        "[Beta]",
        "[É]",
        "[é]",
        "[]",
        " [ empty ] ",
        "key = first",
        "KEY=last",
        "ü=value",
        "Ü=other",
        "=skip",
        "bad header[",
        "nested=a=b",
        "before=1",
    ];
    let mut seed = 17_u64;
    for _ in 0..512 {
        let mut content = String::new();
        for _ in 0..80 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            content.push_str(lines[(seed >> 32) as usize % lines.len()]);
            content.push('\n');
        }
        assert_parse_matches(&content);
    }
}

#[test]
fn owned_fallback_merge_matches_original_precedence_and_adopts_allocations() {
    let corpus = metrics_corpus();
    for (_, first) in &corpus {
        for (_, second) in &corpus {
            let mut current = IniData::parse(first);
            let mut expected = OriginalIniData::parse(first);
            current.merge_missing_owned(IniData::parse(second));
            expected.merge_missing_from(&OriginalIniData::parse(second));
            assert_eq!(current.sections, expected.sections);
        }
    }
    let mut current = IniData::default();
    let mut expected = OriginalIniData::default();
    for input in [
        "[Global]\nFallbackNoteSkin=default\n[A]\nx=first\n[Empty]",
        "[GLOBAL]\nfallbacknoteskin=common\n[a]\nX=second\ny=new\n[Other]\nz=third",
        "[Global]\nFallbackNoteSkin=ignored\n[Empty]\nx=from-common",
    ] {
        current.merge_missing_owned(IniData::parse(input));
        expected.merge_missing_from(&OriginalIniData::parse(input));
        assert_eq!(current.sections, expected.sections);
    }
    assert_eq!(current.get("GLOBAL", "fallbackNOTESKIN"), Some("default"));
    assert_eq!(current.get("a", "x"), Some("first"));

    let source = IniData::parse(&corpus[0].1);
    let value_ptr = source
        .sections
        .values()
        .flat_map(|s| s.values())
        .next()
        .unwrap()
        .as_ptr();
    let mut destination = IniData::default();
    allocations::assert_no_churn(|| destination.merge_missing_owned(source));
    assert!(
        destination
            .sections
            .values()
            .flat_map(|s| s.values())
            .any(|value| value.as_ptr() == value_ptr)
    );
}

macro_rules! exercise_cache {
    ($cache:expr) => {{
        let cache = $cache;
        let mut results = Vec::new();
        let mut retained = Vec::new();
        for (num_cols, num_players) in [(4, 1), (4, 2), (8, 1), (5, 1), (10, 1), (0, 0)] {
            let style = Style {
                num_cols,
                num_players,
            };
            for (name, alias) in [
                (" CeL ", "cel"),
                (" \t", "DEFAULT"),
                ("ÉSkin", "ÉSKIN"),
                ("éSkin", "éSKIN"),
                ("pack?color=Blue&arrow=Metal", "PACK?COLOR=BLUE&ARROW=METAL"),
            ] {
                assert!(cache.get(&style, name).is_none());
                assert_eq!(
                    cache
                        .get_or_load(&style, name, || Err("load failed".to_owned()))
                        .unwrap_err(),
                    "load failed"
                );
                let value = format!("{num_cols}:{num_players}:{name}");
                let loaded = cache.get_or_load(&style, name, || Ok(value)).unwrap();
                assert!(Arc::ptr_eq(&loaded, &cache.get(&style, alias).unwrap()));
                assert!(Arc::ptr_eq(
                    &loaded,
                    &cache
                        .get_or_load(&style, alias, || panic!("resident cache must not load"))
                        .unwrap()
                ));
                results.push((*loaded).clone());
                retained.push(loaded);
            }
        }
        drop(retained);
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        assert!(cache.get(&style, "cel").is_none());
        let reloaded = cache
            .get_or_load(&style, "cel", || Ok("reloaded".to_owned()))
            .unwrap();
        results.push((*reloaded).clone());
        cache.clear();
        assert!(cache.get(&style, "cel").is_none());
        // The loader must remain outside the lock and may clear the cache.
        let loaded = cache
            .get_or_load(&style, "cel", || {
                cache.clear();
                Ok("after clear".to_owned())
            })
            .unwrap();
        results.push((*loaded).clone());
        results
    }};
}

#[test]
fn runtime_cache_matches_original_lifetime_keys_errors_and_reentry() {
    assert_eq!(
        exercise_cache!(ItgSkinRuntimeCache::<String>::default()),
        exercise_cache!(OriginalRuntimeCache::<String>::default())
    );
    let style = Style {
        num_cols: 4,
        num_players: 1,
    };
    let cache = ItgSkinRuntimeCache::default();
    let resident = cache.get_or_load(&style, "cel", || Ok(42)).unwrap();
    allocations::assert_no_churn(|| {
        for query in ["cel", " CeL ", "missing"] {
            black_box(cache.get(&style, query));
        }
        black_box(
            cache
                .get_or_load(&style, " CEL ", || panic!("resident"))
                .unwrap(),
        );
    });
    assert_eq!(*resident, 42);
}

macro_rules! exercise_race {
    ($cache:expr) => {{
        let cache = $cache;
        let barrier = std::sync::Barrier::new(2);
        let style = Style {
            num_cols: 4,
            num_players: 1,
        };
        std::thread::scope(|scope| {
            let one = scope.spawn(|| {
                cache
                    .get_or_load(&style, " CeL ", || {
                        barrier.wait();
                        Ok(1)
                    })
                    .unwrap()
            });
            let two = scope.spawn(|| {
                cache
                    .get_or_load(&style, "cel", || {
                        barrier.wait();
                        Ok(2)
                    })
                    .unwrap()
            });
            let one = one.join().unwrap();
            let two = two.join().unwrap();
            assert!(Arc::ptr_eq(&one, &two));
            assert!(Arc::ptr_eq(&one, &cache.get(&style, "CEL").unwrap()));
            assert!([1, 2].contains(&*one));
        });
        assert!(cache.get(&style, "cel").is_none());
    }};
}

#[test]
fn runtime_cache_retains_original_concurrent_winner_behavior() {
    exercise_race!(ItgSkinRuntimeCache::default());
    exercise_race!(OriginalRuntimeCache::default());
}

#[test]
fn runtime_cache_hashes_match_at_block_and_utf8_boundaries() {
    let style = Style {
        num_cols: 4,
        num_players: 1,
    };
    for length in [0, 1, 15, 31, 32, 33, 63, 64, 65, 127, 128, 129, 4096] {
        for suffix in ["", "É", "é", "雪"] {
            let name = format!("{}{}", "a".repeat(length), suffix);
            let query = format!(" \t{} \u{2003}", name.to_ascii_uppercase());
            let owned = itg_skin_cache_key(&style, &name);
            let borrowed = ItgSkinCacheKeyRef::new(&style, &query);
            assert!(borrowed.equivalent(&owned));
            let mut a = std::collections::hash_map::DefaultHasher::new();
            let mut b = std::collections::hash_map::DefaultHasher::new();
            owned.hash(&mut a);
            borrowed.hash(&mut b);
            assert_eq!(a.finish(), b.finish());
            let cache = ItgSkinRuntimeCache::default();
            let resident = cache.get_or_load(&style, &name, || Ok(length)).unwrap();
            allocations::assert_no_churn(|| {
                assert!(Arc::ptr_eq(&resident, &cache.get(&style, &query).unwrap()));
            });
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_ini_parse() {
    let corpus = metrics_corpus();
    let original_churn = allocations::measure(|| {
        for (_, content) in &corpus {
            black_box(OriginalIniData::parse(black_box(content)));
        }
    })
    .1;
    let current_churn = allocations::measure(|| {
        for (_, content) in &corpus {
            black_box(IniData::parse(black_box(content)));
        }
    })
    .1;
    println!(
        "parse {} shipped INIs: original {original_churn:?}, current {current_churn:?}",
        corpus.len()
    );
    assert!(current_churn.allocs < original_churn.allocs);
    paired::compare("parse shipped INI corpus", 100, |current| {
        for (_, content) in &corpus {
            if current {
                black_box(IniData::parse(black_box(content)));
            } else {
                black_box(OriginalIniData::parse(black_box(content)));
            }
        }
    });
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_owned_fallback_merge() {
    let requested = "[Global]\nFallbackNoteSkin=default\n[NoteDisplay]\nTapNoteAnimationLength=2";
    let default = include_str!("../../../assets/noteskins/dance/default/metrics.ini");
    let common = include_str!("../../../assets/noteskins/common/common/metrics.ini");
    let inputs = [requested, default, common];
    let sources: Vec<_> = inputs.iter().map(|s| IniData::parse(s)).collect();
    let originals: Vec<_> = inputs.iter().map(|s| OriginalIniData::parse(s)).collect();
    let owned_originals = originals.clone();
    let original_churn = allocations::measure(|| {
        let mut out = OriginalIniData::default();
        for input in owned_originals {
            out.merge_missing_from(&input);
        }
        black_box(out);
    })
    .1;
    let owned = sources.clone();
    let current_churn = allocations::measure(|| {
        let mut out = IniData::default();
        for input in owned {
            out.merge_missing_owned(input);
        }
        black_box(out);
    })
    .1;
    println!(
        "merge requested/default/common: original {original_churn:?}, current {current_churn:?}"
    );
    assert!(current_churn.allocs < original_churn.allocs);
    paired::compare_prepared(
        "merge requested/default/common",
        250,
        || sources.clone(),
        |sources, current| {
            if current {
                let mut out = IniData::default();
                for input in sources {
                    out.merge_missing_owned(black_box(input));
                }
                black_box(out);
            } else {
                let mut out = OriginalIniData::default();
                for input in sources {
                    let input = OriginalIniData {
                        sections: input.sections,
                    };
                    out.merge_missing_from(black_box(&input));
                }
                black_box(out);
            }
        },
    );
    paired::compare("parse and merge requested/default/common", 500, |current| {
        if current {
            let mut out = IniData::default();
            for input in inputs {
                out.merge_missing_owned(IniData::parse(black_box(input)));
            }
            black_box(out);
        } else {
            let mut out = OriginalIniData::default();
            for input in inputs {
                out.merge_missing_from(&OriginalIniData::parse(black_box(input)));
            }
            black_box(out);
        }
    });
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_runtime_cache() {
    let style = Style {
        num_cols: 4,
        num_players: 1,
    };
    let current = ItgSkinRuntimeCache::default();
    let original = OriginalRuntimeCache::default();
    let long =
        "workshop?arrow=metal&hold_active=bright&hold_inactive=blue&receptor=ring&mine=classic";
    let residents: Vec<_> = ["cel", "default", long]
        .iter()
        .map(|skin| {
            (
                current.get_or_load(&style, skin, || Ok(1)).unwrap(),
                original.get_or_load(&style, skin, || Ok(1)).unwrap(),
            )
        })
        .collect();
    for (label, query) in [
        ("resident lowercase", "cel"),
        ("resident mixed case", " CeL "),
        ("resident default", " \t"),
        ("resident long pack key", long),
        ("missing lookup", "not-installed"),
    ] {
        let old = allocations::measure(|| black_box(original.get(&style, query))).1;
        let new = allocations::measure(|| black_box(current.get(&style, query))).1;
        println!("{label}: original {old:?}, current {new:?}");
        assert_eq!(new.allocs, 0);
        paired::compare(label, 100_000, |use_current| {
            if use_current {
                black_box(current.get(black_box(&style), black_box(query)));
            } else {
                black_box(original.get(black_box(&style), black_box(query)));
            }
        });
    }
    paired::compare("resident get_or_load", 100_000, |use_current| {
        if use_current {
            black_box(
                current
                    .get_or_load(black_box(&style), black_box("cel"), || panic!("resident"))
                    .unwrap(),
            );
        } else {
            black_box(
                original
                    .get_or_load(black_box(&style), black_box("cel"), || panic!("resident"))
                    .unwrap(),
            );
        }
    });
    paired::compare("cold load and release", 10_000, |use_current| {
        if use_current {
            let cache = ItgSkinRuntimeCache::default();
            black_box(
                cache
                    .get_or_load(black_box(&style), black_box("cel"), || Ok(1))
                    .unwrap(),
            );
        } else {
            let cache = OriginalRuntimeCache::default();
            black_box(
                cache
                    .get_or_load(black_box(&style), black_box("cel"), || Ok(1))
                    .unwrap(),
            );
        }
    });
    black_box(residents);
}
