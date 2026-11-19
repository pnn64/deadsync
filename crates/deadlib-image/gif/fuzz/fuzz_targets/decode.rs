#![no_main]
use deadlib_image_gif::*;
use libfuzzer_sys::fuzz_target;

struct Font;
impl TextRasterizer for Font {
    fn rasterize(
        &mut self,
        character: u8,
        _width: u8,
        _height: u8,
        mask: &mut [u8],
    ) -> Result<(), Error> {
        for (n, p) in mask.iter_mut().enumerate() {
            *p = character.rotate_left((n % 8) as u32) & 1;
        }
        Ok(())
    }
}

fn check(bytes: &[u8], strict: bool) {
    let options = DecodeOptions {
        limits: Limits {
            max_input_bytes: 1 << 20,
            max_frame_pixels: 1 << 16,
            max_total_pixels: 1 << 18,
            max_metadata_bytes: 1 << 16,
            max_blocks: 4096,
            max_frames: 64,
        },
        strict,
        ..Default::default()
    };
    let actual = decode_with_options(bytes, &options);
    let mut decoder = Decoder::new(options);
    let mut at = 0;
    let mut blocks = Vec::new();
    let mut header = None;
    let mut failed = false;
    let chunk = 1 + bytes.first().copied().unwrap_or(0) as usize;
    while at < bytes.len() {
        let end = (at + chunk).min(bytes.len());
        match decoder.update(&bytes[at..end]) {
            Err(_) => {
                failed = true;
                break;
            }
            Ok(p) => {
                assert!(p.consumed > 0);
                at += p.consumed;
                match p.event {
                    Some(Event::Header) => header = decoder.header().cloned(),
                    Some(Event::Block(b)) => blocks.push(b),
                    // Feed any suffix too: strict decoding rejects it, while
                    // compatibility decoding consumes and charges its bytes.
                    Some(Event::End) | None => {}
                }
            }
        }
    }
    let streaming_ok = !failed && at == bytes.len() && decoder.finish().is_ok();
    assert_eq!(actual.is_ok(), streaming_ok);
    let Ok(actual) = actual else {
        return;
    };
    assert_eq!(header.as_ref(), Some(&actual.header));
    assert_eq!(blocks, actual.blocks);
    match encode(&actual, &EncodeOptions::default()) {
        Ok(encoded) => assert_eq!(decode(&encoded).unwrap(), actual),
        // Relaxed decoding can retain an invalid background index or a version
        // declaration that strict encoding requires the caller to repair.
        Err(_) => assert!(!strict),
    }
    if let Ok(mut compositor) = Compositor::with_options(
        &actual.header,
        CompositeOptions {
            background: Background::Transparent,
            ..Default::default()
        },
    ) {
        for block in &actual.blocks {
            if let Block::Extension(Extension::PlainText(text)) = block {
                let _ = compositor.render_text(text, &mut Font);
            } else {
                let _ = compositor.render_block(block);
            }
        }
    }
    // Most reference decoders ignore plain-text graphics and their control scope.
    if actual.blocks.iter().any(|b| {
        matches!(
            b,
            Block::Extension(Extension::PlainText(_) | Extension::Unknown { .. })
        )
    }) {
        return;
    }
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    opts.set_memory_limit(gif::MemoryLimit::Bytes(
        std::num::NonZeroU64::new(1 << 20).unwrap(),
    ));
    let Ok(mut reference) = opts.read_info(bytes) else {
        return;
    };
    for frame in actual.frames() {
        let Ok(Some(f)) = reference.read_next_frame() else {
            return;
        };
        assert_eq!(
            (f.left, f.top, f.width, f.height),
            (frame.left, frame.top, frame.width, frame.height)
        );
        assert_eq!(f.buffer.as_ref(), frame.indices);
    }
}

fuzz_target!(|bytes: &[u8]| {
    check(bytes, true);
    check(bytes, false);
});
