use deadlib_video_mpeg4::{
    decode_avi, decode_elementary_stream, Error, Frame, MsMpeg4v2Decoder, PictureKind,
};
use PictureKind::{I, P};

const STREAM: &[u8] = include_bytes!("fixtures/progressive.m4v");
// Hashes of visible YCbCr samples from FFmpeg 7.1 with -idct simple.
const EXPECTED: [(PictureKind, u64); 10] = [
    (I, 0x2e6b0b95f5f914da),
    (P, 0x8dca12c5bb773356),
    (P, 0x3951b996126b1d52),
    (P, 0x87c13d034d3de47c),
    (P, 0x736801dbb1c36bf4),
    (I, 0x63a2d3005d16b574),
    (P, 0xbcc87adcc29f374e),
    (P, 0xb0e3c4617e5b7fe6),
    (P, 0x14fdf0bb617afbbd),
    (P, 0x8259f712e7d4da93),
];

fn fingerprint(frame: &Frame, [width, height]: [usize; 2]) -> (PictureKind, u64) {
    assert_eq!((frame.width(), frame.height()), (width, height));
    let mut hash = 0xcbf29ce484222325u64;
    for (index, plane) in frame.planes().iter().enumerate() {
        let dimensions = if index == 0 {
            (width, height)
        } else {
            (width.div_ceil(2), height.div_ceil(2))
        };
        assert_eq!((plane.width(), plane.height()), dimensions);
        assert_eq!(plane.rows().len(), dimensions.1);
        for row in plane.rows() {
            assert_eq!(row.len(), dimensions.0);
            for &sample in row {
                hash = (hash ^ u64::from(sample)).wrapping_mul(0x100000001b3);
            }
        }
    }
    (frame.picture_kind(), hash)
}

#[test]
fn decodes_intra_and_predicted_pictures_with_video_packets() {
    let mut frames = Vec::new();
    decode_elementary_stream(STREAM, |frame| frames.push(fingerprint(&frame, [160, 90]))).unwrap();
    assert_eq!(frames, EXPECTED);
}

#[test]
fn decodes_avi_with_layer_header_in_stream_format() {
    let start = STREAM
        .windows(4)
        .position(|window| window == [0, 0, 1, 0xb6])
        .unwrap();
    let (configuration, pictures) = STREAM.split_at(start);

    let mut format = vec![0; 40];
    format[0] = 40;
    format[16..20].copy_from_slice(b"XVID");
    format.extend_from_slice(configuration);
    let stream = group(
        b"LIST",
        b"strl",
        &[chunk(b"strh", b"vidsXVID"), chunk(b"strf", &format)],
    );
    let header = group(b"LIST", b"hdrl", &[stream]);
    let record = group(
        b"LIST",
        b"rec ",
        &[chunk(b"01wb", &[1, 2, 3]), chunk(b"00dc", pictures)],
    );
    let movie = group(b"LIST", b"movi", &[record]);
    let avi = group(b"RIFF", b"AVI ", &[header, movie]);

    let mut frames = Vec::new();
    decode_avi(&avi, |frame| frames.push(frame)).unwrap();
    assert_eq!(
        frames
            .iter()
            .map(|frame| fingerprint(frame, [160, 90]))
            .collect::<Vec<_>>(),
        EXPECTED
    );

    assert_eq!(
        decode_avi(&avi[..avi.len() - 1], |_| unreachable!()),
        Err(Error::Truncated)
    );
}

#[test]
fn decodes_mp42_avi_with_partial_macroblocks() {
    let mut frames = Vec::new();
    decode_avi(include_bytes!("fixtures/mp42.avi"), |frame| {
        frames.push(fingerprint(&frame, [144, 82]));
    })
    .unwrap();
    // Hashes of visible YCbCr samples from FFmpeg 7.1 with -idct simple.
    assert_eq!(
        frames,
        [
            (I, 0xd1b5b3bcc1903cb3),
            (P, 0xe02431eb1c905a64),
            (P, 0x0c93a236fe88a06c),
            (P, 0x48fcadcc27cc850e),
            (P, 0x1b49548242ac1461),
            (I, 0x9ea23631fbf9d302),
            (P, 0xe32ba4e2ece21a51),
            (P, 0x10bb4ff1aff248f7),
            (P, 0x2e3e6ffe020262a4),
            (P, 0xcfe7a1ea4d4a4f98),
        ]
    );
}

#[test]
fn decodes_mp42_packets_and_keeps_reference_after_truncation() {
    // Two one-row slices. Each starts with an independent DC predictor.
    let intra = [0x03, 0x88, 0xfe, 0x81, 0x24, 0x08, 0xe4, 0x90, 0x00];
    let skip = [0x43, 0xc0];
    let zero_motion = [0x42, 0xff, 0xc0];
    let mut decoder = MsMpeg4v2Decoder::new(16, 32).unwrap();
    assert!(matches!(
        decoder.push_packet(&skip),
        Err(Error::MissingReference)
    ));

    let reference = decoder.push_packet(&intra).unwrap();
    assert_eq!(reference.picture_kind(), I);
    for (y, row) in reference.planes()[0].rows().enumerate() {
        assert_eq!(row, &[if y < 16 { 192 } else { 128 }; 16]);
    }
    assert!(matches!(
        decoder.push_packet(&intra[..1]),
        Err(Error::Truncated)
    ));

    for packet in [&skip[..], &zero_motion] {
        let frame = decoder.push_packet(packet).unwrap();
        assert_eq!(frame.picture_kind(), P);
        for (plane, expected) in frame.planes().iter().zip(reference.planes()) {
            assert!(plane.rows().eq(expected.rows()));
        }
    }
}

fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut bytes = id.to_vec();
    bytes.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(data);
    if data.len() & 1 != 0 {
        bytes.push(0);
    }
    bytes
}

fn group(id: &[u8; 4], kind: &[u8; 4], children: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = kind.to_vec();
    for child in children {
        bytes.extend_from_slice(child);
    }
    chunk(id, &bytes)
}
