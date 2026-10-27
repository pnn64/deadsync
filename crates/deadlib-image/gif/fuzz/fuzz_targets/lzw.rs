#![no_main]
use deadlib_image_gif::lzw;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if bytes.len() < 3 {
        return;
    }
    let minimum = bytes[0] % 7 + 2;
    let limit = u16::from_le_bytes([bytes[1], bytes[2]]) as usize;
    let data = &bytes[3..];
    let mut decoder = lzw::Decoder::new();
    decoder.reset(minimum).unwrap();
    let mut output = Vec::new();
    let mut incremental = lzw::Decoder::new();
    incremental.reset(minimum).unwrap();
    let mut fragmented = Vec::new();
    let full = decoder.update(data, &mut output, limit);
    let mut result = Ok(());
    for b in data.chunks(1 + minimum as usize) {
        result = incremental.update(b, &mut fragmented, limit);
        if result.is_err() {
            break;
        }
    }
    assert_eq!(full.is_ok(), result.is_ok());
    assert_eq!(fragmented, output);
    if full.is_ok() && decoder.ended() {
        assert_eq!(lzw::decode(data, minimum, output.len()).unwrap(), output);
        let encoded = lzw::encode(&output, minimum).unwrap();
        assert_eq!(
            lzw::decode(&encoded, minimum, output.len()).unwrap(),
            output
        );
    }
});
