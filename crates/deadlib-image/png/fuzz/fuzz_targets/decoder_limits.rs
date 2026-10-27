#![no_main]
#[path = "../../tests/support/resource.rs"]
mod resource;
libfuzzer_sys::fuzz_target!(|bytes: &[u8]| resource::check(bytes, crc32fast::hash));
