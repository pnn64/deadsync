#[path = "fuzz_support/mod.rs"]
mod fuzz_support;

#[test]
fn malformed_container_mutations_preserve_reader_and_stream_agreement() {
    use deadlib_audio_decode_wav::{SampleFormat, Spec, WriteOptions, Writer};
    use std::io::Cursor;
    let mut writer = Writer::new(
        Cursor::new(Vec::new()),
        Spec::new(SampleFormat::I16, 2, 48000),
        WriteOptions::default(),
        &[],
    )
    .unwrap();
    writer
        .write_i16(&[-32768, 32767, -1, 0, 12345, -23456])
        .unwrap();
    let mut seed = vec![0];
    seed.extend(writer.finish().unwrap().into_inner());
    assert_eq!(fuzz_support::exercise(&seed) & 3, 3);
    for pos in 1..seed.len() {
        for value in [0, 1, 127, 128, 254, 255] {
            let mut input = seed.clone();
            input[pos] = value;
            fuzz_support::exercise(&input);
        }
        fuzz_support::exercise(&seed[..pos]);
    }
    assert_eq!(fuzz_support::PATHS.len(), 15);
}
