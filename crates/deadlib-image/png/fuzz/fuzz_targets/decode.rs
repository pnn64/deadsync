#![no_main]
use deadlib_image_png::{DecodeOptions, Limits, Samples, decode_with_options};
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|bytes: &[u8]| {
    let limits = Limits {
        max_input_bytes: 1 << 20,
        max_output_bytes: 1 << 20,
        max_metadata_bytes: 1 << 16,
        max_chunks: 4096,
        max_frames: 64,
    };
    let Ok(actual) = decode_with_options(
        bytes,
        &DecodeOptions {
            limits,
            ..Default::default()
        },
    ) else {
        return;
    };
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits { bytes: 2 << 20 });
    // Reference ancillary policies can differ; only reference-accepted images
    // are used for exact sample assertions. Native acceptance is not an oracle.
    let Ok(mut reader) = decoder.read_info() else {
        return;
    };
    let Some(size) = reader.output_buffer_size() else {
        return;
    };
    if size > 1 << 20 {
        return;
    }
    let mut output = vec![0; size];
    let Ok(info) = reader.next_frame(&mut output) else {
        return;
    };
    assert_eq!(
        (
            info.width,
            info.height,
            info.color_type as u8,
            info.bit_depth as u8
        ),
        (
            actual.image.width,
            actual.image.height,
            actual.image.color_type as u8,
            actual.image.bit_depth
        )
    );
    match &actual.image.samples {
        Samples::U16(v) => assert_eq!(
            &output[..info.buffer_size()],
            v.iter().flat_map(|v| v.to_be_bytes()).collect::<Vec<_>>()
        ),
        Samples::U8(v) if actual.image.bit_depth == 8 => {
            assert_eq!(&output[..info.buffer_size()], v)
        }
        Samples::U8(v) => {
            let depth = actual.image.bit_depth as usize;
            let width = info.width as usize;
            let stride = (width * depth).div_ceil(8);
            for (n, &sample) in v.iter().enumerate() {
                let (y, x) = (n / width, n % width);
                assert_eq!(
                    sample,
                    (output[y * stride + x * depth / 8] >> (8 - depth - x * depth % 8))
                        & ((1 << depth) - 1)
                );
            }
        }
    }
});
