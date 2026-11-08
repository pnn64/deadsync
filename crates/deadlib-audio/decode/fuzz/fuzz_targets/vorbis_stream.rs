#![no_main]
use deadlib_audio_decode_fuzz::{repair_checksums, vorbis_stream};
libfuzzer_sys::fuzz_target!(|data: &[u8]| vorbis_stream(data));
libfuzzer_sys::fuzz_mutator!(|data: &mut [u8], size: usize, max_size: usize, seed: u32| {
    let size = libfuzzer_sys::fuzzer_mutate(data, size, max_size);
    if size > 1 && seed & 3 != 0 {
        repair_checksums(&mut data[1..size]);
    }
    size
});
