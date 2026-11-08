#![no_main]
libfuzzer_sys::fuzz_target!(|data: &[u8]| deadlib_audio_decode_fuzz::vorbis_packets(data));
