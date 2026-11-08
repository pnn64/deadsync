#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| deadlib_audio_decode_flac_fuzz::encode(data));
