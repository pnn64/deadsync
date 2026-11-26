use std::sync::Arc;

use deadlib_video_mpeg2::{
    decode_elementary_stream, decode_program_stream, video_elementary_stream, Error, Frame,
    PictureKind::{B, I, P},
};

const VIDEO: &[u8] = include_bytes!("fixtures/progressive.m2v");

fn collect(frames: &mut Vec<(u16, deadlib_video_mpeg2::PictureKind, u64)>, frame: Arc<Frame>) {
    assert_eq!((frame.width(), frame.height()), (48, 34));
    assert_eq!(frame.frame_rate(), (25, 1));
    let mut hash = 0xcbf29ce484222325u64;
    for (index, plane) in frame.planes().iter().enumerate() {
        let (width, height) = if index == 0 { (48, 34) } else { (24, 17) };
        assert_eq!((plane.width(), plane.height()), (width, height));
        assert_eq!(plane.rows().len(), height);
        for row in plane.rows() {
            assert_eq!(row.len(), width);
            for &sample in row {
                hash = (hash ^ u64::from(sample)).wrapping_mul(0x100000001b3);
            }
        }
    }
    frames.push((frame.temporal_reference(), frame.picture_kind(), hash));
}

#[test]
fn decodes_elementary_stream_in_display_order() {
    let mut retained = Vec::new();
    decode_elementary_stream(VIDEO, |frame| retained.push(frame)).unwrap();
    let mut frames = Vec::new();
    for frame in retained {
        collect(&mut frames, frame);
    }
    // FNV-1a of each visible YCbCr frame from FFmpeg 7.1 with -idct simple.
    assert_eq!(
        frames,
        [
            (0, I, 0x7a2546f4abf8f17a),
            (1, B, 0xf1cbb0d6ac29fe69),
            (2, B, 0xf1cbb0d6ac29fe69),
            (3, P, 0x489c50f952ac58a5),
            (4, B, 0x9f4a343166f3144d),
            (5, B, 0x9f4a343166f3144d),
            (6, P, 0x38c5b5c94a1bc9fd),
            (7, P, 0x9f4a343166f3144d),
        ]
    );
}

#[test]
fn joins_video_packets_across_a_start_code() {
    let mut program = vec![0, 0, 1, 0xba, 0x44, 0, 4, 0, 4, 1, 0, 0, 3, 0xf8];
    packet(&mut program, 0xe0, &[0x80, 0, 0], &VIDEO[..2]);
    packet(&mut program, 0xc0, &[], &[0; 16]);
    packet(&mut program, 0xe1, &[0x80, 0, 0], &[0; 16]);
    packet(&mut program, 0xe0, &[0xff, 0x0f], &VIDEO[2..]);
    program.extend_from_slice(&[0, 0, 1, 0xb9]);

    assert_eq!(video_elementary_stream(&program).unwrap(), VIDEO);
    let mut frames = Vec::new();
    decode_program_stream(&program, |frame| collect(&mut frames, frame)).unwrap();
    assert_eq!(frames.len(), 8);
}

#[test]
fn rejects_a_picture_with_missing_slices() {
    let first_slice = VIDEO
        .windows(4)
        .position(|unit| unit == [0, 0, 1, 1])
        .unwrap();
    assert_eq!(
        decode_elementary_stream(&VIDEO[..first_slice], |_| unreachable!()),
        Err(Error::Truncated)
    );
}

fn packet(program: &mut Vec<u8>, id: u8, header: &[u8], data: &[u8]) {
    let length = u16::try_from(header.len() + data.len()).unwrap();
    program.extend_from_slice(&[0, 0, 1, id]);
    program.extend_from_slice(&length.to_be_bytes());
    program.extend_from_slice(header);
    program.extend_from_slice(data);
}
