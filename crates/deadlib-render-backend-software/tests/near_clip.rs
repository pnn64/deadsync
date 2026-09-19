//! Compare the production projection path with its pre-change implementation.
#[path = "../../../tests/support/perf.rs"]
#[allow(dead_code)]
mod perf;

#[allow(dead_code)]
mod backend {
    include!("../src/lib.rs");

    mod near_clip {
        include!("near_clip/cases.rs");
    }
}
