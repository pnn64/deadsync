use deadlib_video_vp9::{
    decode_mp4, decode_mp4_pictures, DecodeError, Decoder, Frame, Mp4Error, PlaneData,
};

const FRAGMENTED: &[u8] = include_bytes!("fixtures/fragmented.mp4");
const INDEXED: &[u8] = include_bytes!("fixtures/indexed.mp4");

// Visible Y, U and V samples from FFmpeg 7.1.
const EXPECTED: [u64; 9] = [
    0x352c327cd40607f5,
    0x4a1e735261237c9b,
    0x9fec41d74ba31b27,
    0x0aec2ee77575600d,
    0x08eb57261538bab3,
    0x046fd84394636018,
    0xa93eb74f4645591a,
    0xa095d113423b74bc,
    0x9a999177534c8cda,
];

fn fingerprint(frame: Frame) -> u64 {
    assert_eq!((frame.width, frame.height, frame.bit_depth), (82, 50, 8));
    assert_eq!((frame.subsampling_x, frame.subsampling_y), (1, 1));
    let mut hash = 0xcbf29ce484222325u64;
    for (plane, len) in [frame.y, frame.u, frame.v]
        .into_iter()
        .zip([4100, 1025, 1025])
    {
        let PlaneData::U8(samples) = plane else {
            panic!("expected 8-bit samples")
        };
        assert_eq!(samples.len(), len);
        for sample in samples {
            hash = (hash ^ u64::from(sample)).wrapping_mul(0x100000001b3);
        }
    }
    hash
}

#[test]
fn decodes_indexed_and_fragmented_mp4_with_inter_frames() {
    for file in [INDEXED, FRAGMENTED] {
        let mut hashes = Vec::new();
        decode_mp4(file, |frame| hashes.push(fingerprint(frame))).unwrap();
        assert_eq!(hashes, EXPECTED);
    }
}

#[test]
fn retained_pictures_keep_their_samples_after_reference_refresh() {
    let mut pictures = Vec::new();
    decode_mp4_pictures(FRAGMENTED, |picture| pictures.push(picture)).unwrap();
    let hashes: Vec<_> = pictures
        .into_iter()
        .map(|picture| fingerprint(picture.to_owned_frame()))
        .collect();
    assert_eq!(hashes, EXPECTED);
}

#[test]
fn rejects_a_truncated_mp4_box() {
    let truncated = &FRAGMENTED[..FRAGMENTED.len() - 1];
    assert_eq!(
        decode_mp4(truncated, |_| unreachable!()),
        Err(Mp4Error::Truncated)
    );
}

#[test]
fn show_existing_frame_requires_a_reference() {
    // Profile 0, show-existing-frame, reference slot zero.
    let packet = [0x88];
    assert_eq!(
        Decoder::new().push_packet(&packet),
        Err(DecodeError::MissingReferenceFrame)
    );
}
