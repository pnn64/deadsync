use std::sync::Arc;

use deadlib_video_mpeg2::{
    decode_elementary_stream, decode_program_stream, units, video_elementary_stream, Decoder,
    Error, Frame,
    PictureKind::{B, I, P},
};

const VIDEO: &[u8] = include_bytes!("fixtures/progressive.m2v");
const MPEG1_VIDEO: &[u8] = include_bytes!("fixtures/progressive.m1v");

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
fn decodes_mpeg1_elementary_and_program_streams() {
    let mut frames = Vec::new();
    decode_elementary_stream(MPEG1_VIDEO, |frame| collect(&mut frames, frame)).unwrap();
    // FNV-1a of each visible YCbCr frame from FFmpeg 7.1 with -idct simple.
    assert_eq!(
        frames,
        [
            (0, I, 0x731b54b4ec9ec448),
            (1, B, 0xfd98501361171573),
            (2, B, 0xfd98501361171573),
            (3, P, 0xa5179c9ee4f551bb),
            (4, B, 0xbfdb13a387708457),
            (5, B, 0xbfdb13a387708457),
            (6, P, 0xc881a2af12c25e65),
            (7, P, 0xbfdb13a387708457),
        ]
    );

    let mut program = vec![0, 0, 1, 0xba, 0x21, 0, 1, 0, 1, 0x80, 0, 1];
    packet(&mut program, 0xe0, &[0x0f], MPEG1_VIDEO);
    program.extend_from_slice(&[0, 0, 1, 0xb9]);
    let mut unpacked = Vec::new();
    decode_program_stream(&program, |frame| collect(&mut unpacked, frame)).unwrap();
    assert_eq!(unpacked, frames);
}

#[test]
fn mpeg1_recovery_preserves_display_order_after_bad_and_missing_slices() {
    let mut damaged = Vec::new();
    let mut reference = 0;
    for unit in units(MPEG1_VIDEO) {
        if unit.code == 0 {
            reference = u16::from_be_bytes([unit.payload[0], unit.payload[1]]) >> 6;
        }
        if reference == 7 && unit.code == 3 {
            continue;
        }
        damaged.extend_from_slice(&[0, 0, 1, unit.code]);
        if reference != 1 || unit.code != 1 {
            damaged.extend_from_slice(unit.payload);
        }
    }

    assert!(decode_elementary_stream(&damaged, |_| {}).is_err());

    let mut expected = Vec::new();
    decode_elementary_stream(MPEG1_VIDEO, |frame| collect(&mut expected, frame)).unwrap();
    expected[1].2 = expected[0].2; // Damaged B picture repeats its older reference.
    expected[7].2 = expected[6].2; // The final P picture is missing its last slice.

    let mut decoder = Decoder::with_mpeg1_recovery();
    let mut actual = Vec::new();
    for unit in units(&damaged) {
        decoder
            .push(unit, |frame| collect(&mut actual, frame))
            .unwrap();
    }
    decoder.finish(|frame| collect(&mut actual, frame)).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn mpeg1_full_pel_motion_keeps_the_predictor_in_coded_units() {
    // Two 16x16 macroblocks. The I picture has a luma boundary at x=16.
    // The P picture codes a full-pel vector of +1 followed by a delta of -1.
    let stream = include_bytes!("fixtures/full-pel.m1v");
    let mut count = 0;
    decode_elementary_stream(stream, |frame| {
        let boundary = if count == 0 { 16 } else { 15 };
        for row in frame.planes()[0].rows() {
            assert!(row[..boundary].iter().all(|&pixel| pixel == 64));
            assert!(row[boundary..].iter().all(|&pixel| pixel == 192));
        }
        count += 1;
    })
    .unwrap();
    assert_eq!(count, 2);
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
