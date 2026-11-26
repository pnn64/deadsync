use deadlib_video_h264::{
    nal::{parse_annex_b, parse_avcc, parse_avcc_config},
    ChromaFormat, Frame, OrderedDecoder,
};

const BASELINE: &[u8] = include_bytes!("fixtures/baseline.h264");
const BASELINE_YUV: &[u8] = include_bytes!("fixtures/baseline.yuv");
const HIGH: &[u8] = include_bytes!("fixtures/high.h264");
const HIGH_YUV: &[u8] = include_bytes!("fixtures/high.yuv");
const HIGH_AVCC: &[u8] = include_bytes!("fixtures/high.avcc");
const HIGH_CONFIG: &[u8] = include_bytes!("fixtures/high.avcc-config");

fn assert_frames(frames: &[Frame], expected: &[u8]) {
    const LUMA: usize = 46 * 30;
    const CHROMA: usize = 23 * 15;
    const FRAME_SIZE: usize = LUMA + 2 * CHROMA;

    assert_eq!(frames.len() * FRAME_SIZE, expected.len());
    for (index, (frame, pixels)) in frames
        .iter()
        .zip(expected.chunks_exact(FRAME_SIZE))
        .enumerate()
    {
        assert_eq!((frame.width, frame.height), (46, 30));
        assert_eq!(frame.bit_depth, 8);
        assert_eq!(frame.chroma_format, ChromaFormat::Yuv420);
        assert_eq!(frame.y, pixels[..LUMA], "frame {index} luma");
        assert_eq!(frame.u, pixels[LUMA..LUMA + CHROMA], "frame {index} Cb");
        assert_eq!(frame.v, pixels[LUMA + CHROMA..], "frame {index} Cr");
    }
}

fn decode_annex_b(data: &[u8]) -> Vec<Frame> {
    let mut decoder = OrderedDecoder::new();
    let mut frames = Vec::new();
    for nal in parse_annex_b(data) {
        frames.extend(decoder.decode_nal(&nal).unwrap());
    }
    frames.extend(decoder.flush());
    assert!(decoder.flush().is_empty());
    frames
}

#[test]
fn baseline_cavlc_matches_reference() {
    assert_frames(&decode_annex_b(BASELINE), BASELINE_YUV);
}

#[test]
fn high_cabac_with_b_pictures_matches_reference() {
    assert_frames(&decode_annex_b(HIGH), HIGH_YUV);
}

#[test]
fn avcc_preserves_timestamps_across_reordering() {
    let config = parse_avcc_config(HIGH_CONFIG).unwrap();
    let mut decoder = OrderedDecoder::<u64>::new();
    for nal in config.sps_nals.iter().chain(&config.pps_nals) {
        assert!(decoder.decode_nal_with_meta(nal, 0).unwrap().is_empty());
    }

    // Packet sizes and presentation times from the MP4, in decode order.
    let sizes = [1308, 92, 57, 25, 25, 63, 40, 33, 634, 83, 68, 34, 26];
    let display_indices = [0, 4, 2, 1, 3, 7, 5, 6, 8, 12, 10, 9, 11];
    let mut frames = Vec::new();
    let mut samples = HIGH_AVCC;
    for (size, display_index) in sizes.into_iter().zip(display_indices) {
        let (sample, remaining) = samples.split_at(size);
        for nal in parse_avcc(sample, config.length_size) {
            frames.extend(
                decoder
                    .decode_nal_with_meta(&nal, display_index * 2048)
                    .unwrap(),
            );
        }
        samples = remaining;
    }
    assert!(samples.is_empty());
    frames.extend(decoder.flush_with_meta());
    let (decoded, timestamps): (Vec<_>, Vec<_>) = frames.into_iter().unzip();
    assert_eq!(
        timestamps,
        (0..13).map(|index| index * 2048).collect::<Vec<_>>()
    );
    assert_frames(&decoded, HIGH_YUV);
}

#[cfg(feature = "std")]
#[test]
fn threaded_high_cabac_matches_reference() {
    let mut decoder = deadlib_video_h264::threading::ThreadedDecoder::new(2);
    let mut frames = Vec::new();
    for nal in parse_annex_b(HIGH) {
        frames.extend(decoder.decode_nal(&nal).unwrap());
    }
    frames.extend(decoder.flush());
    assert_frames(&frames, HIGH_YUV);
}
