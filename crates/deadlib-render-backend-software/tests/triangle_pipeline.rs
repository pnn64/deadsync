//! Headless triangle pipeline checks and opt-in CPU measurements.
#[allow(dead_code)]
mod backend {
    include!("../src/lib.rs");

    mod triangle_pipeline {
        include!("../src/triangle_pipeline.rs");
    }
}
