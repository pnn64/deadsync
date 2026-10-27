#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| deadlib_audio_decode_flac_fuzz::ogg(data));
libfuzzer_sys::fuzz_mutator!(|data: &mut [u8], size: usize, max_size: usize, seed: u32| {
    let size = libfuzzer_sys::fuzzer_mutate(data, size, max_size);
    if seed & 3 != 0 {
        deadlib_audio_decode_flac_fuzz::repair_ogg(&mut data[..size]);
    }
    size
});
