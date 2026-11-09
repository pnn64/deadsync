//! Fallible storage for VM-independent compilation, using Rust's allocator.
use crate::{Error, ErrorKind};
use std::{
    alloc::{Layout, alloc},
    fmt::{self, Write},
    ptr,
};
mod map;
pub(crate) use map::Map;

/// Reserve growth before allocating. In a bounded compiler workspace the exact
/// requested layout is charged, including the old storage until the job ends.
pub(crate) fn reserve<T>(values: &mut Vec<T>, additional: usize) -> Result<(), Error> {
    if additional <= values.capacity() - values.len() {
        return Ok(());
    }
    if !super::compilation::active() {
        return values.try_reserve(additional).map_err(|_| Error::memory());
    }
    let needed = values
        .len()
        .checked_add(additional)
        .ok_or_else(Error::memory)?;
    let minimum = if size_of::<T>() == 1 {
        8
    } else if size_of::<T>() <= 1024 {
        4
    } else {
        1
    };
    let capacity = needed.max(values.capacity().saturating_mul(2)).max(minimum);
    reserve_exact(values, capacity)
}

fn reserve_exact<T>(values: &mut Vec<T>, capacity: usize) -> Result<(), Error> {
    if capacity <= values.capacity() {
        return Ok(());
    }
    if !super::compilation::active() {
        return values
            .try_reserve_exact(capacity - values.len())
            .map_err(|_| Error::memory());
    }
    let layout = Layout::array::<T>(capacity).map_err(|_| Error::memory())?;
    super::compilation::charge(layout.size())?;
    // SAFETY: the nonempty non-ZST layout is exactly the eventual Vec layout.
    let pointer = unsafe { alloc(layout) }.cast::<T>();
    if pointer.is_null() {
        return Err(Error::memory());
    }
    let length = values.len();
    // SAFETY: move each initialized T into disjoint aligned storage. Clearing
    // the old length transfers ownership without running any element twice.
    unsafe {
        ptr::copy_nonoverlapping(values.as_ptr(), pointer, length);
        values.set_len(0);
        *values = Vec::from_raw_parts(pointer, length, capacity);
    }
    Ok(())
}

pub(crate) trait VecExt<T> {
    fn try_push(&mut self, value: T) -> Result<(), Error>;
    fn try_extend(&mut self, values: impl IntoIterator<Item = T>) -> Result<(), Error>;
    fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), Error>
    where
        T: Copy;
    fn try_resize(&mut self, length: usize, value: T) -> Result<(), Error>
    where
        T: Copy;
    fn try_into_boxed_slice(self) -> Result<Box<[T]>, Error>;
}
impl<T> VecExt<T> for Vec<T> {
    fn try_push(&mut self, value: T) -> Result<(), Error> {
        reserve(self, 1)?;
        self.push(value);
        Ok(())
    }
    fn try_extend(&mut self, values: impl IntoIterator<Item = T>) -> Result<(), Error> {
        for value in values {
            self.try_push(value)?;
        }
        Ok(())
    }
    fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), Error>
    where
        T: Copy,
    {
        reserve(self, values.len())?;
        self.extend_from_slice(values);
        Ok(())
    }
    fn try_resize(&mut self, length: usize, value: T) -> Result<(), Error>
    where
        T: Copy,
    {
        reserve(self, length.saturating_sub(self.len()))?;
        self.resize(length, value);
        Ok(())
    }
    fn try_into_boxed_slice(mut self) -> Result<Box<[T]>, Error> {
        let length = self.len();
        if length == 0 {
            return Ok(Box::default());
        }
        let pointer = if self.capacity() == length || size_of::<T>() == 0 {
            let pointer = self.as_mut_ptr();
            std::mem::forget(self);
            pointer
        } else {
            let layout = Layout::array::<T>(length).map_err(|_| Error::memory())?;
            super::compilation::charge(layout.size())?;
            // SAFETY: nonempty non-ZST layout, matching the eventual Box's
            // deallocation layout and alignment. A null result leaves self owned.
            let pointer = unsafe { alloc(layout) }.cast::<T>();
            if pointer.is_null() {
                return Err(Error::memory());
            }
            // SAFETY: copy initialized values to disjoint, sufficiently large
            // storage, then remove them from the old owner without dropping them.
            unsafe {
                ptr::copy_nonoverlapping(self.as_ptr(), pointer, length);
                self.set_len(0);
            }
            pointer
        };
        // SAFETY: one exact-size allocation (or an aligned dangling ZST pointer)
        // owns length initialized T values; ownership has left the source Vec.
        Ok(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(pointer, length)) })
    }
}
pub(crate) trait SliceExt<T: Copy> {
    fn try_to_vec(&self) -> Result<Vec<T>, Error>;
    fn try_boxed(&self) -> Result<Box<[T]>, Error>;
}
impl<T: Copy> SliceExt<T> for [T] {
    fn try_to_vec(&self) -> Result<Vec<T>, Error> {
        let mut values = with_capacity(self.len())?;
        values.extend_from_slice(self);
        Ok(values)
    }
    fn try_boxed(&self) -> Result<Box<[T]>, Error> {
        self.try_to_vec()?.try_into_boxed_slice()
    }
}
pub(crate) fn with_capacity<T>(capacity: usize) -> Result<Vec<T>, Error> {
    let mut values = Vec::new();
    reserve_exact(&mut values, capacity)?;
    Ok(values)
}
pub(crate) fn vec<T, const N: usize>(values: [T; N]) -> Result<Vec<T>, Error> {
    let mut out = with_capacity(N)?;
    out.extend(values);
    Ok(out)
}
pub(crate) fn repeat<T: Copy>(value: T, count: usize) -> Result<Vec<T>, Error> {
    let mut out = with_capacity(count)?;
    out.resize(count, value);
    Ok(out)
}
pub(crate) fn boxed<T>(value: T) -> Result<Box<T>, Error> {
    let slice = vec([value])?.try_into_boxed_slice()?;
    // SAFETY: the slice owns exactly one initialized T, with the same layout as
    // Box<T>, including aligned zero-size types. Ownership leaves the slice.
    Ok(unsafe { Box::from_raw(Box::into_raw(slice).cast::<T>()) })
}
pub(crate) fn format(arguments: fmt::Arguments<'_>) -> Result<String, Error> {
    struct Writer(String);
    impl Write for Writer {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            // SAFETY: reserve changes capacity only; the initialized bytes and
            // their UTF-8 validity are unchanged before push_str appends text.
            reserve(unsafe { self.0.as_mut_vec() }, text.len()).map_err(|_| fmt::Error)?;
            self.0.push_str(text);
            Ok(())
        }
    }
    let mut out = Writer(String::new());
    out.write_fmt(arguments).map_err(|_| Error::memory())?;
    Ok(out.0)
}
pub(crate) fn diagnostic(kind: ErrorKind, line: u32, arguments: fmt::Arguments<'_>) -> Error {
    match format(arguments) {
        Ok(text) => Error::from_string(kind, line, text),
        Err(error) => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allocation_probe::probe;
    use std::cell::Cell;

    struct Owned<'a>(&'a Cell<usize>);
    impl Drop for Owned<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    #[test]
    fn boxed_value_is_dropped_on_both_acceptance_and_rejection() {
        for deny in 0..=1 {
            let drops = Cell::new(0);
            let (failed, stats) = probe(deny, || boxed(Owned(&drops)).is_err());
            assert_eq!(failed, deny == 1);
            assert_eq!(drops.get(), 1);
            assert_eq!(stats.allocated_bytes, stats.freed_bytes);
        }
    }

    #[test]
    fn boxed_slice_transfers_or_drops_every_element_once() {
        for (length, capacity) in [(0, 0), (0, 8), (4, 4), (4, 8)] {
            for deny in 0..=2 {
                let drops = Cell::new(0);
                let (result, stats) = probe(deny, || -> Result<(), ErrorKind> {
                    let run = || -> Result<(), Error> {
                        let mut values = with_capacity(capacity)?;
                        for _ in 0..length {
                            values.push(Owned(&drops));
                        }
                        drop(values.try_into_boxed_slice()?);
                        Ok(())
                    };
                    run().map_err(|error| error.kind())
                });
                assert_eq!(result.is_err(), stats.rejected != 0);
                assert_eq!(
                    drops.get(),
                    if deny == 1 && capacity != 0 {
                        0
                    } else {
                        length
                    }
                );
                assert_eq!(stats.allocated_bytes, stats.freed_bytes);
            }
        }
    }

    #[test]
    fn boxed_slice_handles_aligned_zero_size_elements_without_allocating() {
        thread_local! { static DROPS: Cell<usize> = const { Cell::new(0) }; }
        #[repr(align(64))]
        struct Zst;
        impl Drop for Zst {
            fn drop(&mut self) {
                DROPS.with(|n| n.set(n.get() + 1));
            }
        }
        DROPS.with(|n| n.set(0));
        let ((), stats) = probe(1, || {
            let mut values = Vec::new();
            for _ in 0..5 {
                values.push(Zst);
            }
            let boxed = values.try_into_boxed_slice().unwrap();
            assert_eq!(boxed.len(), 5);
            assert_eq!(boxed.as_ptr().addr() % 64, 0);
            drop(boxed);
            drop(super::boxed(Zst).unwrap());
        });
        assert_eq!(stats.attempts, 0);
        assert_eq!(DROPS.with(Cell::get), 6);
    }
}
