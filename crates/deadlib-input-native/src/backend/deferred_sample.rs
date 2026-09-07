pub(super) struct DeferredSample<T> {
    value: Option<T>,
}

impl<T: Copy> DeferredSample<T> {
    #[inline(always)]
    pub(super) const fn new() -> Self {
        Self { value: None }
    }

    #[inline(always)]
    pub(super) fn get_or_init(&mut self, sample: impl FnOnce() -> T) -> T {
        *self.value.get_or_insert_with(sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn sampling_is_lazy_and_shared_for_the_scope() {
        let calls = Cell::new(0);
        let mut sample = DeferredSample::new();
        assert_eq!(calls.get(), 0);

        let first = sample.get_or_init(|| {
            calls.set(calls.get() + 1);
            42
        });
        let second = sample.get_or_init(|| {
            calls.set(calls.get() + 1);
            99
        });

        assert_eq!(first, 42);
        assert_eq!(second, first);
        assert_eq!(calls.get(), 1);
    }
}
