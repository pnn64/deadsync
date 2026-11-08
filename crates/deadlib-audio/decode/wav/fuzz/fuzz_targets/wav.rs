#![no_main]
#[path = "../../tests/fuzz_support/mod.rs"]
mod fuzz_support;
libfuzzer_sys::fuzz_target!(|input: &[u8]| {
    std::hint::black_box(fuzz_support::PATHS);
    std::hint::black_box(fuzz_support::exercise(input));
});
