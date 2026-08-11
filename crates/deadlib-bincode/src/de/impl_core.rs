//! Fallible, allocation-free array collection.

use core::mem::{self, MaybeUninit};

/// Pulls `N` items from `iter` and returns them as an array. If the iterator
/// yields fewer than `N` items, `None` is returned and all already yielded
/// items are dropped.
///
/// Since the iterator is passed as a mutable reference and this function calls
/// `next` at most `N` times, the iterator can still be used afterwards to
/// retrieve the remaining items.
///
/// If `iter.next()` panics, all items already yielded by the iterator are
/// dropped.
pub fn collect_into_array<E, I, T, const N: usize>(iter: &mut I) -> Option<Result<[T; N], E>>
where
    I: Iterator<Item = Result<T, E>>,
{
    struct Guard<'a, T, const N: usize> {
        array_mut: &'a mut [MaybeUninit<T>; N],
        initialized: usize,
    }

    impl<T, const N: usize> Drop for Guard<'_, T, N> {
        fn drop(&mut self) {
            debug_assert!(self.initialized <= N);

            for item in &mut self.array_mut[..self.initialized] {
                // SAFETY: `initialized` counts exactly the elements written below.
                unsafe { item.assume_init_drop() };
            }
        }
    }

    let mut array = [const { MaybeUninit::<T>::uninit() }; N];
    let mut guard = Guard {
        array_mut: &mut array,
        initialized: 0,
    };

    for slot in guard.array_mut.iter_mut() {
        let item_rslt = iter.next()?;
        let item = match item_rslt {
            Err(err) => {
                return Some(Err(err));
            }
            Ok(elem) => elem,
        };

        slot.write(item);
        guard.initialized += 1;
    }

    mem::forget(guard);
    // SAFETY:
    // * the loop wrote every element before the guard was forgotten
    // * `MaybeUninit<T>` and T are guaranteed to have the same layout
    // * `MaybeUninit` does not drop, so there are no double-frees
    let out = unsafe { (&array as *const _ as *const [T; N]).read() };
    Some(Ok(out))
}

#[cfg(test)]
mod tests {
    use super::collect_into_array;
    use std::{
        cell::Cell,
        panic::{catch_unwind, AssertUnwindSafe},
    };

    struct DropCount<'a>(&'a Cell<usize>);

    impl Drop for DropCount<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn empty_array_does_not_advance() {
        let mut iter = core::iter::from_fn(|| -> Option<Result<u8, ()>> {
            panic!("zero-length collection advanced the iterator")
        });
        let array = collect_into_array::<(), _, _, 0>(&mut iter)
            .expect("zero-length iterator has enough items")
            .expect("iterator cannot fail");
        assert_eq!(array, []);
    }

    #[test]
    fn exhausted_array_drops_initialized_items() {
        let drops = Cell::new(0);
        let mut iter = (0..2).map(|_| Ok::<_, ()>(DropCount(&drops)));
        let result = collect_into_array::<(), _, _, 3>(&mut iter);
        assert!(result.is_none());
        assert_eq!(drops.get(), 2);
    }

    #[test]
    fn error_drops_initialized_items() {
        let drops = Cell::new(0);
        let mut iter = [Ok(DropCount(&drops)), Err(())].into_iter();
        let result = collect_into_array::<(), _, _, 3>(&mut iter);
        assert!(matches!(result, Some(Err(()))));
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn panic_drops_initialized_items() {
        let drops = Cell::new(0);
        let mut first = Some(DropCount(&drops));
        let mut iter = core::iter::from_fn(|| match first.take() {
            Some(item) => Some(Ok::<_, ()>(item)),
            None => panic!("iterator failure"),
        });
        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _ = collect_into_array::<(), _, _, 3>(&mut iter);
        }));
        assert!(panic.is_err());
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn successful_array_owns_initialized_items() {
        let drops = Cell::new(0);
        let mut iter = (0..3).map(|_| Ok::<_, ()>(DropCount(&drops)));
        let array = collect_into_array::<(), _, _, 3>(&mut iter)
            .expect("iterator has enough items")
            .expect("iterator cannot fail");
        assert_eq!(drops.get(), 0);
        drop(array);
        assert_eq!(drops.get(), 3);
    }
}
