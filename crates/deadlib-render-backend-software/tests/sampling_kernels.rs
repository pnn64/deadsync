//! Headless production sampling and clipping checks, with one scoped allocator.
#[allow(dead_code)]
mod backend {
    include!("../src/lib.rs");

    mod sampling_performance {
        include!("../src/sampling_performance.rs");
    }
}
