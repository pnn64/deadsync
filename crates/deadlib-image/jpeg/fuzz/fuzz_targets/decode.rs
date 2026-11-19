#![no_main]
//! The bounded pixel decoder (AVX2-dispatched where available, unless built
//! with `--cfg deadlib_image_jpeg_portable`) must agree exactly with the
//! portable coefficient -> plane -> image pipeline: equal acceptance and
//! identical pixels under the same strict or compatible input policy. The only
//! tolerated difference is the reference exceeding
//! a storage limit that the bounded row window stays within.
use deadlib_image::Image;
use deadlib_image_jpeg::*;
use libfuzzer_sys::fuzz_target;

fn reference(bytes: &[u8], options: DecodeOptions, method: Upsampling) -> Result<Image> {
    for event in MarkerReader::with_strict(bytes, options.strict)? {
        if let Event::Segment { marker, .. } = event? {
            if marker == 0xde {
                return decode_hierarchical_with_options(bytes, options)?
                    .reconstruct()?
                    .to_image(method);
            }
            if let Ok(process) = Process::try_from(marker) {
                if process.is_lossless() {
                    return decode_lossless_with_options(bytes, options)?
                        .into_planar()?
                        .to_image(method);
                }
                break;
            }
        }
    }
    decode_coefficients_with_options(bytes, options)?
        .reconstruct()?
        .to_image(method)
}

fuzz_target!(|bytes: &[u8]| {
    let limits = Limits {
        max_pixels: 1 << 20,
        max_memory_bytes: 64 << 20,
        max_metadata_bytes: 1 << 20,
        max_scans: 256,
    };
    let method = if bytes.len() % 2 == 0 {
        Upsampling::Linear
    } else {
        Upsampling::Nearest
    };
    let options = DecodeOptions {
        limits,
        strict: bytes.len() % 4 < 2,
    };
    let actual = decode_with_options(bytes, options, method);
    match (actual, reference(bytes, options, method)) {
        (Ok(actual), Ok(expected)) => assert!(actual == expected, "pixels differ"),
        (Err(_), Err(_)) | (Ok(_), Err(Error::LimitExceeded(_))) => {}
        (actual, expected) => panic!(
            "acceptance differs: {:?} vs {:?}",
            actual.map(|_| ()),
            expected.map(|_| ())
        ),
    }
});
