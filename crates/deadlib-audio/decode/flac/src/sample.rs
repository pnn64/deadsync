//! Private sample storage. Reconstruction arithmetic always uses i64.
pub(crate) trait Sample: Copy {
    const BITS: u32;
    fn wide(self) -> i64;
    /// The caller checks the reconstructed width before storing, or reads a
    /// signed field whose width is bounded by this storage type.
    fn store(value: i64) -> Self;
}
impl Sample for i32 {
    const BITS: u32 = 32;
    #[inline(always)]
    fn wide(self) -> i64 {
        i64::from(self)
    }
    #[inline(always)]
    fn store(value: i64) -> Self {
        value as i32
    }
}
impl Sample for i64 {
    const BITS: u32 = 64;
    #[inline(always)]
    fn wide(self) -> i64 {
        self
    }
    #[inline(always)]
    fn store(value: i64) -> Self {
        value
    }
}
