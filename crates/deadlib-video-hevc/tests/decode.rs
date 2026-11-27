use deadlib_video_hevc::{
    DecodeError, Decoder, Frame, Mp4Error, decode_annex_b, decode_mp4, decode_mp4_pictures,
    parse_annex_b,
};

const MP4: &[u8] = include_bytes!("fixtures/main.mp4");
const ANNEX_B: &[u8] = include_bytes!("fixtures/main.hevc");

// FNV-1a per frame over the visible Y, Cb and Cr samples from FFmpeg.
const EXPECTED: [u64; 12] = [
    0xb4e84bc69dbb1b2f,
    0xb9acc41b8cbcaf2f,
    0x9b3cce035e210413,
    0xfa60299771cbdcd3,
    0x741be1194a2d469c,
    0xb9b9e0c3036f9b65,
    0xd0cd9892d7845a57,
    0x75528784973c3dd1,
    0x7e30d2c12f458561,
    0xf6ac5e1588388328,
    0xb89536b0a1b6c94e,
    0xcaad3e631ffcd668,
];

fn fingerprint(frame: Frame) -> u64 {
    assert_eq!((frame.width, frame.height, frame.bit_depth), (82, 50, 8));
    assert_eq!(frame.chroma_format_idc, 1);
    let mut hash = 0xcbf29ce484222325u64;
    for (plane, size) in [frame.y, frame.u, frame.v]
        .into_iter()
        .zip([4100, 1025, 1025])
    {
        let samples = plane.as_u8().unwrap();
        assert_eq!(samples.len(), size);
        for &sample in samples {
            hash = (hash ^ u64::from(sample)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[test]
fn decodes_mp4_and_annex_b_in_display_order() {
    let mut hashes = Vec::new();
    decode_mp4(MP4, |frame| hashes.push(fingerprint(frame))).unwrap();
    assert_eq!(hashes, EXPECTED);

    hashes.clear();
    decode_annex_b(ANNEX_B, |frame| hashes.push(fingerprint(frame))).unwrap();
    assert_eq!(hashes, EXPECTED);
}

#[test]
fn retained_pictures_keep_their_samples() {
    let mut pictures = Vec::new();
    decode_mp4_pictures(MP4, |picture| pictures.push(picture)).unwrap();
    let hashes: Vec<_> = pictures
        .iter()
        .map(|p| fingerprint(p.to_owned_frame()))
        .collect();
    assert_eq!(hashes, EXPECTED);
}

#[test]
fn picture_output_survives_threaded_reordering() {
    let mut decoder = Decoder::new().with_display_order().with_threads(2);
    let mut pictures = Vec::new();
    for nal in parse_annex_b(ANNEX_B) {
        assert!(decoder.decode_nal_picture(&nal).unwrap().is_none());
        while let Some(picture) = decoder.next_picture() {
            pictures.push(picture);
        }
    }
    while let Some(picture) = decoder.flush_picture().unwrap() {
        pictures.push(picture);
    }
    drop(decoder);
    let hashes: Vec<_> = pictures
        .iter()
        .map(|p| fingerprint(p.to_owned_frame()))
        .collect();
    assert_eq!(hashes, EXPECTED);
}

#[test]
fn rejects_truncated_packets_and_configuration() {
    let mut decoder = Decoder::new();
    assert_eq!(
        decoder.push_decoder_config(&[1; 22]),
        Err(DecodeError::UnexpectedEof)
    );
    assert_eq!(
        decoder.push_packet(&[0, 0, 0, 9, 0x46, 1, 0x50], 4, |_| unreachable!()),
        Err(DecodeError::UnexpectedEof)
    );
    assert_eq!(
        decode_mp4(&MP4[..MP4.len() - 1], |_| unreachable!()),
        Err(Mp4Error::Truncated)
    );
}
