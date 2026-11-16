use super::*;
use crate::{ErrorKind, StringId, memory::tests::with_memory};

#[test]
fn pattern_cache_and_compilation_fail_before_publication_and_preserve_old_entries() {
    fn run(nth: usize, filled: usize) -> usize {
        let mut points = 0;
        with_memory(|memory, attempts, reject| {
            let mut cache = Cache::new_in(memory.clone());
            let mut work = Work::new_in(memory.clone());
            for n in 0..filled {
                cache.try_reserve(1).unwrap();
                cache.insert(
                    (StringId(n as u64), false),
                    Pattern::compile(b"z", false, memory.clone()).unwrap(),
                );
            }
            let source = b"[a-z][a-z][a-z][a-z][a-z][a-z][a-z][a-z][a-z]";
            let key = (StringId(1000), true);
            let start = attempts.get();
            reject.set(if nth == 0 { 0 } else { start + nth });
            let result = cache
                .try_reserve(1)
                .and_then(|()| Pattern::compile(source, true, memory.clone()));
            points = attempts.get() - start;
            assert_eq!(cache.values().count(), filled);
            assert!(!cache.contains_key(&key));
            if nth != 0 {
                assert_eq!(result.as_ref().unwrap_err().kind(), ErrorKind::Memory);
            }
            for n in 0..filled {
                let found = cache[&(StringId(n as u64), false)]
                    .find(b"az", 0, &mut work)
                    .unwrap()
                    .unwrap();
                assert_eq!((found.start, found.end), (1, 2));
            }
            reject.set(0);
            let pattern = result.unwrap_or_else(|_| {
                cache.try_reserve(1).unwrap();
                Pattern::compile(source, true, memory.clone()).unwrap()
            });
            cache.insert(key, pattern);
            assert_eq!(cache.values().count(), filled + 1);
            let found = cache[&key]
                .find(b"abcdefghi", 0, &mut work)
                .unwrap()
                .unwrap();
            assert_eq!((found.start, found.end), (0, 9));
        });
        points
    }
    let mut total = 0;
    for filled in [0, 6] {
        let points = run(0, filled);
        assert!(points >= 5, "points={points}");
        for nth in 1..=points {
            run(nth, filled);
        }
        total += points;
    }
    eprintln!("PATTERN_REJECTION_POINTS {total}");
}

#[test]
fn pattern_backtracking_retries_after_each_failed_growth_and_reuses_owned_storage() {
    fn run(nth: usize) -> usize {
        let mut points = 0;
        with_memory(|memory, attempts, reject| {
            let mut bytes = b"a?".repeat(64);
            bytes.push(b'b');
            let mut source = vec![b'a'; 64];
            source.push(b'b');
            let pattern = Pattern::compile(&bytes, true, memory.clone()).unwrap();
            let mut work = Work::new_in(memory.clone());
            let start = attempts.get();
            reject.set(if nth == 0 { 0 } else { start + nth });
            let result = pattern.find(&source, 0, &mut work);
            points = attempts.get() - start;
            if nth != 0 {
                assert_eq!(result.unwrap_err().kind(), ErrorKind::Memory);
            }
            reject.set(0);
            let found = pattern.find(&source, 0, &mut work).unwrap().unwrap();
            assert_eq!((found.start, found.end), (0, 65));
            let start = attempts.get();
            reject.set(start + 1);
            let (result, stats) =
                crate::allocation_probe::probe(1, || pattern.find(&source, 0, &mut work));
            assert_eq!(result.unwrap().unwrap().end, 65);
            assert_eq!(stats.attempts, 0);
            assert_eq!(attempts.get(), start);
            reject.set(0);
        });
        points
    }
    let points = run(0);
    assert!(points >= 5, "points={points}");
    for nth in 1..=points {
        run(nth);
    }
    eprintln!("PATTERN_REJECTION_POINTS {points}");
}

#[test]
fn malformed_pattern_diagnostics_need_no_allocation() {
    with_memory(|memory, attempts, reject| {
        let pattern = Pattern::compile(b"%1", true, memory.clone()).unwrap();
        let mut work = Work::new_in(memory.clone());
        let start = attempts.get();
        reject.set(start + 1);
        let (result, stats) =
            crate::allocation_probe::probe(1, || pattern.find(b"a", 0, &mut work));
        let error = result.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Runtime);
        assert_eq!(error.message(), "invalid capture index");
        assert_eq!(attempts.get(), start);
        assert_eq!(stats.attempts, 0);
        reject.set(0);
    });
}
