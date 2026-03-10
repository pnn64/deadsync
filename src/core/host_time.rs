use std::sync::LazyLock;
use std::time::Instant;

static HOST_EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

#[inline(always)]
pub(crate) fn init() {
    let _ = *HOST_EPOCH;
}

#[inline(always)]
pub(crate) fn instant_nanos(at: Instant) -> u64 {
    at.checked_duration_since(*HOST_EPOCH)
        .map(|delta| delta.as_nanos().min((u64::MAX - 1) as u128) as u64)
        .unwrap_or(0)
}

#[cfg(unix)]
#[inline(always)]
pub(crate) fn now_nanos() -> u64 {
    instant_nanos(Instant::now())
}
