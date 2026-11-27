use std::ops::{Deref, DerefMut};

/// Inline storage for the small syntax lists common in HEVC slices. Larger
/// lists keep the same behavior using heap storage.
#[derive(Clone, Debug)]
pub(crate) enum SmallList<T, const N: usize> {
    Inline { values: [T; N], len: usize },
    Heap(Vec<T>),
}

impl<T: Default, const N: usize> Default for SmallList<T, N> {
    fn default() -> Self {
        Self::Inline {
            values: std::array::from_fn(|_| T::default()),
            len: 0,
        }
    }
}

impl<T: Clone + Default, const N: usize> SmallList<T, N> {
    pub(crate) fn push(&mut self, value: T) {
        match self {
            Self::Inline { values, len } if *len < N => {
                values[*len] = value;
                *len += 1;
            }
            Self::Inline { values, .. } => {
                let mut heap = Vec::with_capacity((2 * N).max(1));
                heap.extend_from_slice(values);
                heap.push(value);
                *self = Self::Heap(heap);
            }
            Self::Heap(values) => values.push(value),
        }
    }
}

impl<T: Copy, const N: usize> SmallList<T, N> {
    pub(crate) fn clear(&mut self) {
        match self {
            Self::Inline { len, .. } => *len = 0,
            Self::Heap(values) => values.clear(),
        }
    }
}

impl<T, const N: usize> Deref for SmallList<T, N> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        match self {
            Self::Inline { values, len } => &values[..*len],
            Self::Heap(values) => values,
        }
    }
}

impl<T, const N: usize> DerefMut for SmallList<T, N> {
    fn deref_mut(&mut self) -> &mut [T] {
        match self {
            Self::Inline { values, len } => &mut values[..*len],
            Self::Heap(values) => values,
        }
    }
}

impl<T: Clone + Default, const N: usize> FromIterator<T> for SmallList<T, N> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut list = Self::default();
        for item in iter {
            list.push(item);
        }
        list
    }
}

#[cfg(test)]
impl<T: PartialEq, const N: usize> PartialEq<Vec<T>> for SmallList<T, N> {
    fn eq(&self, other: &Vec<T>) -> bool {
        &self[..] == other.as_slice()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a SmallList<T, N> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
