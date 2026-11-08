#![no_main]
use deadlib_audio_decode_mp3::*;
use libfuzzer_sys::fuzz_target;

fn pcm_hash(pcm: Pcm<'_>) -> u64 {
    pcm.planes().flatten().fold(0u64, |hash, value| {
        assert!(value.is_finite());
        hash.wrapping_mul(0x100000001b3) ^ u64::from(value.to_bits())
    })
}
fuzz_target!(|input: &[u8]| {
    if input.is_empty() || input.len() > 32769 {
        return;
    }
    let options = DecodeOptions {
        verify_crc: input[0] & 1 != 0,
        ..Default::default()
    };
    let data = &input[1..];
    let _ = tags::Id3Tag::parse(data, options.limits);
    let _ = tags::Id3v1::parse(data);
    let mut reader = bits::BitReader::new(data);
    for (i, &byte) in data.iter().take(64).enumerate() {
        let width = u32::from(byte % 33);
        let before = reader.position();
        let result = reader.read(width);
        if let Ok(value) = result {
            let expected = (0..width as usize).fold(0, |n, bit| {
                let at = before + bit;
                (n << 1) | u32::from(data[at / 8] >> (7 - at % 8) & 1)
            });
            assert_eq!(value, expected);
        } else {
            assert_eq!(reader.position(), before);
        }
        if i % 7 == 0 {
            let _ = reader.skip(usize::from(byte));
        }
    }
    let mut a = FrameDecoder::with_options(options);
    let mut b = FrameDecoder::with_options(options);
    let mut offset = 0;
    let mut free = None;
    for _ in 0..64 {
        let Some(bytes) = data.get(offset..) else {
            break;
        };
        let Ok(header) = FrameHeader::parse(bytes) else {
            break;
        };
        let _ = tags::VbrHeader::parse(header, bytes);
        if header.bitrate().is_none() && free.is_none() {
            free = FreeFormat::infer(bytes, options.limits).ok();
        }
        let left = a
            .decode(bytes, free)
            .map(|out| (out.header, out.bytes, pcm_hash(out.pcm)));
        let right = b
            .decode(bytes, free)
            .map(|out| (out.header, out.bytes, pcm_hash(out.pcm)));
        assert_eq!(left, right);
        let length = match left {
            Ok((_, length, _)) => length,
            Err(_) => {
                a.reset();
                b.reset();
                break;
            }
        };
        assert!(length > 0);
        offset += length;
    }
    a.reset();
    b.reset();
    assert_eq!(
        a.decode(data, free).map(|out| pcm_hash(out.pcm)),
        b.decode(data, free).map(|out| pcm_hash(out.pcm))
    );
});
