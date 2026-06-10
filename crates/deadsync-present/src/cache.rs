use std::cell::RefCell;
use std::collections::{HashMap, hash_map::RandomState};
use std::hash::{BuildHasher, Hash};
use std::sync::Arc;
use std::thread::LocalKey;

pub type TextCache<K, S = RandomState> = HashMap<K, Arc<str>, S>;
pub type SharedStrCache<S = RandomState> = HashMap<Box<str>, Arc<str>, S>;

#[inline(always)]
pub fn cached_text<K, S, F>(
    cache: &'static LocalKey<RefCell<TextCache<K, S>>>,
    key: K,
    limit: usize,
    build: F,
) -> Arc<str>
where
    K: Copy + Eq + Hash,
    S: BuildHasher,
    F: FnOnce() -> String,
{
    cache.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(text) = cache.get(&key) {
            return text.clone();
        }
        let text: Arc<str> = Arc::<str>::from(build());
        if cache.len() < limit {
            cache.insert(key, text.clone());
        }
        text
    })
}

#[inline(always)]
pub fn cached_shared_str<S>(
    cache: &'static LocalKey<RefCell<SharedStrCache<S>>>,
    text: &str,
    limit: usize,
) -> Arc<str>
where
    S: BuildHasher,
{
    cache.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(shared) = cache.get(text) {
            return shared.clone();
        }
        let shared: Arc<str> = Arc::<str>::from(text);
        if cache.len() < limit {
            cache.insert(text.into(), shared.clone());
        }
        shared
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    thread_local! {
        static TEST_CACHE: RefCell<TextCache<u32>> = RefCell::new(HashMap::with_capacity(4));
        static TEST_STR_CACHE: RefCell<SharedStrCache> = RefCell::new(HashMap::with_capacity(4));
    }

    fn clear_test_cache() {
        TEST_CACHE.with(|cache| cache.borrow_mut().clear());
    }

    fn clear_test_str_cache() {
        TEST_STR_CACHE.with(|cache| cache.borrow_mut().clear());
    }

    #[test]
    fn cached_text_reuses_cached_arc() {
        clear_test_cache();
        let first = cached_text(&TEST_CACHE, 42, 4, || "forty-two".to_owned());
        let second = cached_text(&TEST_CACHE, 42, 4, || "different".to_owned());
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(second.as_ref(), "forty-two");
    }

    #[test]
    fn cached_text_saturates_without_inserting_new_keys() {
        clear_test_cache();
        let first = cached_text(&TEST_CACHE, 1, 1, || "one".to_owned());
        let missed_a = cached_text(&TEST_CACHE, 2, 1, || "two".to_owned());
        let missed_b = cached_text(&TEST_CACHE, 2, 1, || "two".to_owned());
        assert_eq!(first.as_ref(), "one");
        assert!(!Arc::ptr_eq(&missed_a, &missed_b));
        TEST_CACHE.with(|cache| {
            let cache = cache.borrow();
            assert_eq!(cache.len(), 1);
            assert!(cache.contains_key(&1));
            assert!(!cache.contains_key(&2));
        });
    }

    #[test]
    fn cached_shared_str_reuses_by_content() {
        clear_test_str_cache();
        let first_input = "alpha".to_string();
        let second_input = String::from("alpha");
        let first = cached_shared_str(&TEST_STR_CACHE, first_input.as_str(), 4);
        let second = cached_shared_str(&TEST_STR_CACHE, second_input.as_str(), 4);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(second.as_ref(), "alpha");
    }

    #[test]
    fn cached_shared_str_separates_different_content() {
        clear_test_str_cache();
        let first = cached_shared_str(&TEST_STR_CACHE, "alpha", 4);
        let second = cached_shared_str(&TEST_STR_CACHE, "bravo", 4);
        assert_eq!(first.as_ref(), "alpha");
        assert_eq!(second.as_ref(), "bravo");
        assert!(!Arc::ptr_eq(&first, &second));
        TEST_STR_CACHE.with(|cache| {
            let cache = cache.borrow();
            assert_eq!(cache.len(), 2);
            assert!(cache.contains_key("alpha"));
            assert!(cache.contains_key("bravo"));
        });
    }
}
