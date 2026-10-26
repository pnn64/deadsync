pub mod combo;
pub mod golf;
pub mod judgment;
pub mod life;
pub mod note;
pub mod scroll;
pub mod stream;
pub mod timing;

#[cfg(test)]
#[path = "../../../tests/perf/stream_alloc.rs"]
mod perf_alloc;

#[cfg(test)]
#[path = "../../../tests/support/kernel_bench.rs"]
mod perf_bench;
