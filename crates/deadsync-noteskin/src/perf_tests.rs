use super::*;
use crate::perf as allocations;
use std::hint::black_box;


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

#[test]
fn owned_fallback_merge_matches_original_precedence_and_adopts_allocations() {
    let corpus = metrics_corpus();
    let mut current = IniData::default();
    for input in [
        "[Global]\nFallbackNoteSkin=default\n[A]\nx=first\n[Empty]",
        "[GLOBAL]\nfallbacknoteskin=common\n[a]\nX=second\ny=new\n[Other]\nz=third",
        "[Global]\nFallbackNoteSkin=ignored\n[Empty]\nx=from-common",
    ] {
        current.merge_missing_owned(IniData::parse(input));
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
    exercise_cache!(ItgSkinRuntimeCache::<String>::default());
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
