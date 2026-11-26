use deadlib_video_mpeg4::{decode_avi, decode_elementary_stream, Error, Frame, PictureKind};
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

fn fingerprint(frame: &Frame) -> (PictureKind, u64) {
    assert_eq!((frame.width(), frame.height()), (160, 90));
    let mut hash = 0xcbf29ce484222325u64;
    for (index, plane) in frame.planes().iter().enumerate() {
        let dimensions = if index == 0 { (160, 90) } else { (80, 45) };
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
    decode_elementary_stream(STREAM, |frame| frames.push(fingerprint(&frame))).unwrap();
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
            .map(|frame| fingerprint(frame))
            .collect::<Vec<_>>(),
        EXPECTED
    );

    assert_eq!(
        decode_avi(&avi[..avi.len() - 1], |_| unreachable!()),
        Err(Error::Truncated)
    );
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
