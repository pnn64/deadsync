#![no_main]
use deadlib_image_png::{
    AnimationPolicy, DecodeEvent, DecodeOptions, DecodeStatus, Image, ImageKind, Limits,
    PalettePolicy, Row, RowSamples, Samples, StreamDecoder, decode_with_options,
};
use libfuzzer_sys::fuzz_target;

fn samples(image: &Image, row: Row<'_>) {
    let channels = image.color_type.channels();
    for pixel in 0..row.width as usize {
        let offset =
            (row.y as usize * image.width as usize + row.x as usize + pixel * row.x_step as usize)
                * channels;
        match (&image.samples, row.samples) {
            (Samples::U8(v), RowSamples::U8(s)) => assert_eq!(
                &v[offset..offset + channels],
                &s[pixel * channels..(pixel + 1) * channels]
            ),
            (Samples::U16(v), RowSamples::U16(s)) => assert_eq!(
                &v[offset..offset + channels],
                &s[pixel * channels..(pixel + 1) * channels]
            ),
            _ => panic!("native sample representation"),
        }
    }
}
fn check(bytes: &[u8], repair_crc: bool, animation_policy: AnimationPolicy) {
    if bytes.len() > 65536 {
        return;
    }
    let mut fixed = bytes.to_vec();
    if repair_crc {
        let mut pos = 8usize;
        while pos + 12 <= fixed.len() {
            let n = u32::from_be_bytes(fixed[pos..pos + 4].try_into().unwrap()) as usize;
            let Some(end) = pos.checked_add(12).and_then(|p| p.checked_add(n)) else {
                break;
            };
            if end > fixed.len() {
                break;
            }
            let crc = crc32fast::hash(&fixed[pos + 4..end - 4]);
            fixed[end - 4..end].copy_from_slice(&crc.to_be_bytes());
            pos = end;
        }
    }
    let options = DecodeOptions {
        limits: Limits {
            max_input_bytes: 1 << 20,
            max_output_bytes: 1 << 20,
            max_metadata_bytes: 1 << 16,
            max_chunks: 4096,
            max_frames: 64,
        },
        palette_policy: if bytes.len() & 1 == 0 {
            PalettePolicy::RecoverBlack
        } else {
            PalettePolicy::Strict
        },
        animation_policy,
    };
    let expected = decode_with_options(&fixed, &options);
    if options.animation_policy == AnimationPolicy::RecoverDefault {
        let strict_options = DecodeOptions {
            animation_policy: AnimationPolicy::Strict,
            ..options
        };
        if let Ok(strict) = decode_with_options(&fixed, &strict_options) {
            assert_eq!(expected.as_ref().unwrap(), &strict);
        } else if let Ok(png) = &expected {
            assert!(png.animation.is_none());
            assert!(png.animation_error.is_some());
            // Independently run the buffered static parser after removing only
            // APNG chunks. Keep every core/metadata byte and relative position.
            let mut static_bytes = fixed[..8].to_vec();
            let mut offset = 8;
            while offset < fixed.len() {
                let len =
                    u32::from_be_bytes(fixed[offset..offset + 4].try_into().unwrap()) as usize;
                let end = offset + 12 + len;
                if !matches!(&fixed[offset + 4..offset + 8], b"acTL" | b"fcTL" | b"fdAT") {
                    static_bytes.extend_from_slice(&fixed[offset..end]);
                }
                offset = end;
            }
            let mut actual = png.clone();
            actual.animation_error = None;
            assert_eq!(
                decode_with_options(&static_bytes, &strict_options).unwrap(),
                actual
            );
        }
    }
    let mut decoder = StreamDecoder::new(options);
    let mut offset = 0;
    let chunk = 1 + usize::from(bytes.last().copied().unwrap_or(0) % 31);
    loop {
        let end = fixed.len().min(offset + chunk);
        let progress = match decoder.process(&fixed[offset..end]) {
            Ok(p) => p,
            Err(e) => {
                assert!(expected.is_err(), "streaming rejected a valid image: {e}");
                assert_eq!(decoder.process(&[]), Err(e));
                return;
            }
        };
        offset += progress.consumed;
        assert_eq!(decoder.total_in(), offset);
        if let (Some(DecodeEvent::Row(row)), Ok(png)) = (decoder.event(), &expected) {
            let image = match row.image {
                ImageKind::Default => &png.image,
                ImageKind::AnimationFrame(n) => {
                    if let Some(animation) = &png.animation {
                        animation.frames[n as usize].image.as_ref().unwrap()
                    } else {
                        assert!(png.animation_error.is_some());
                        // A later APNG fault discarded these earlier frames.
                        continue;
                    }
                }
            };
            samples(image, row);
        }
        match progress.status {
            DecodeStatus::Done => {
                if offset == fixed.len() {
                    assert!(expected.is_ok(), "streaming accepted malformed PNG");
                } else {
                    assert!(expected.is_err());
                }
                decoder.finish().unwrap();
                return;
            }
            DecodeStatus::NeedInput if offset == fixed.len() => {
                assert!(expected.is_err());
                assert!(decoder.finish().is_err());
                return;
            }
            DecodeStatus::NeedInput => assert_eq!(offset, end),
            DecodeStatus::Event => (),
        }
    }
}
fuzz_target!(|bytes: &[u8]| {
    check(bytes, true, AnimationPolicy::Strict);
    check(bytes, true, AnimationPolicy::RecoverDefault);
    check(bytes, false, AnimationPolicy::RecoverDefault);
});
