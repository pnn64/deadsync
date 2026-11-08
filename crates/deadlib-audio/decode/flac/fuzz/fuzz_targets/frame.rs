#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| deadlib_audio_decode_flac_fuzz::frame(data));
libfuzzer_sys::fuzz_mutator!(|data: &mut [u8], size: usize, max_size: usize, seed: u32| {
    let size = libfuzzer_sys::fuzzer_mutate(data, size, max_size);
    if size > 34 && seed & 3 != 0 {
        deadlib_audio_decode_flac_fuzz::repair_frame(&mut data[34..size]);
    }
    size
});
