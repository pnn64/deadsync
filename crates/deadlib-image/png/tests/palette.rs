use deadlib_image_png::{
    Animation, BlendOp, ColorType, DecodeEvent, DecodeOptions, DecodeStatus, DisposeOp,
    EncodeOptions, Frame, FrameControl, Image, Limits, PalettePolicy, Png, RowSamples, Samples,
    StreamDecoder, Transparency, decode, decode_reader, decode_with_options, encode, encode_writer,
};
use std::io::{self, Read};

fn crc(bytes: &[u8]) -> u32 {
    let mut state = !0u32;
    for &b in bytes {
        state ^= u32::from(b);
        for _ in 0..8 {
            state = (state >> 1) ^ if state & 1 != 0 { 0xedb8_8320 } else { 0 };
        }
    }
    !state
}

// Remove palette entries without changing compressed image/frame samples.
fn shorten_palette(bytes: &[u8], entries: usize) -> Vec<u8> {
    let mut out = bytes[..8].to_vec();
    let mut pos = 8;
    while pos < bytes.len() {
        let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        let kind = &bytes[pos + 4..pos + 8];
        if kind == b"PLTE" {
            out.extend_from_slice(&(entries as u32 * 3).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(&bytes[pos + 8..pos + 8 + entries * 3]);
            let sum = crc(&out[out.len() - 4 - entries * 3..]);
            out.extend_from_slice(&sum.to_be_bytes());
        } else {
            out.extend_from_slice(&bytes[pos..pos + len + 12]);
        }
        pos += len + 12;
    }
    out
}

struct OneByte<'a>(&'a [u8]);
impl Read for OneByte<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let count = out.len().min(1);
        self.0.read(&mut out[..count])
    }
}

fn event_rows(
    bytes: &[u8],
    options: DecodeOptions,
) -> Result<Vec<Vec<u8>>, deadlib_image_png::Error> {
    let mut decoder = StreamDecoder::new(options);
    let mut offset = 0;
    let mut images = Vec::<Vec<u8>>::new();
    loop {
        let end = bytes.len().min(offset + 1);
        let progress = decoder.process(&bytes[offset..end])?;
        offset += progress.consumed;
        match progress.status {
            DecodeStatus::Event => {
                if let DecodeEvent::Row(row) = decoder.event().unwrap() {
                    let n = match row.image {
                        deadlib_image_png::ImageKind::Default => 0,
                        deadlib_image_png::ImageKind::AnimationFrame(n) => n as usize + 1,
                    };
                    images.resize_with(images.len().max(n + 1), Vec::new);
                    let RowSamples::U8(samples) = row.samples else {
                        panic!("indexed samples")
                    };
                    for (x, &sample) in samples.iter().enumerate() {
                        let index = row.y as usize * 17 + row.x as usize + x * row.x_step as usize;
                        let image = &mut images[n];
                        image.resize(image.len().max(index + 1), 0);
                        image[index] = sample;
                    }
                }
            }
            DecodeStatus::Done => return Ok(images),
            DecodeStatus::NeedInput if offset == bytes.len() => decoder.finish()?,
            DecodeStatus::NeedInput => (),
        }
    }
}

fn control(width: u32, height: u32, blend: BlendOp) -> FrameControl {
    FrameControl {
        width,
        height,
        x_offset: 0,
        y_offset: 0,
        delay_num: 1,
        delay_den: 10,
        dispose: DisposeOp::None,
        blend,
    }
}

#[test]
fn out_of_range_indices_preserve_samples_render_black_and_remain_invalid_for_encoders() {
    let strict = DecodeOptions {
        palette_policy: PalettePolicy::Strict,
        ..Default::default()
    };
    for depth in [1, 2, 4, 8] {
        for interlaced in [false, true] {
            for default_is_frame in [false, true] {
                let mask = (1usize << depth) - 1;
                let image = Image {
                    width: 17,
                    height: 17,
                    color_type: ColorType::Indexed,
                    bit_depth: depth,
                    samples: Samples::U8((0..289).map(|n| (n & mask) as u8).collect()),
                };
                let mut original = Png::new(image.clone());
                original.palette = vec![[200, 100, 50]; mask + 1];
                original.transparency = Some(Transparency::Palette(vec![0]));
                let mut frames = Vec::new();
                if default_is_frame {
                    frames.push(Frame {
                        control: control(17, 17, BlendOp::Source),
                        image: None,
                    });
                }
                // OVER must treat the recovered pixel as fully opaque.
                frames.push(Frame {
                    control: control(17, 17, BlendOp::Over),
                    image: Some(image.clone()),
                });
                original.animation = Some(Animation {
                    plays: 0,
                    default_is_frame,
                    frames,
                });
                let bytes = shorten_palette(
                    &encode(
                        &original,
                        &EncodeOptions {
                            interlaced,
                            idat_chunk_size: 1,
                            ..Default::default()
                        },
                    )
                    .unwrap(),
                    1,
                );
                let actual = decode(&bytes).unwrap();
                assert_eq!(actual.image, image);
                assert_eq!(actual.animation, original.animation);
                assert_eq!(actual.palette, vec![[200, 100, 50]]);
                assert_eq!(
                    decode_reader(OneByte(&bytes), &Default::default()).unwrap(),
                    actual
                );
                assert!(decode_with_options(&bytes, &strict).is_err());
                assert!(decode_reader(OneByte(&bytes), &strict).is_err());
                assert!(event_rows(&bytes, strict).is_err());
                let rows = event_rows(&bytes, DecodeOptions::default()).unwrap();
                let Samples::U8(samples) = &image.samples else {
                    unreachable!()
                };
                for row_image in rows.iter().filter(|v| !v.is_empty()) {
                    assert_eq!(row_image, samples);
                }
                let rgba = actual.to_rgba16(Limits::default()).unwrap();
                let Samples::U16(rgba) = rgba.samples else {
                    unreachable!()
                };
                for (index, pixel) in samples.iter().zip(rgba.as_chunks::<4>().0) {
                    assert_eq!(
                        pixel,
                        &if *index == 0 {
                            [51400, 25700, 12850, 0]
                        } else {
                            [0, 0, 0, 65535]
                        }
                    );
                }
                let mut compositor = actual.compositor(Limits::default()).unwrap();
                while let Some(frame) = compositor.next_frame().unwrap() {
                    for (index, pixel) in samples.iter().zip(frame.samples.as_chunks::<4>().0) {
                        // A transparent source can retain hidden RGB in a SOURCE frame.
                        assert_eq!(pixel[3], if *index == 0 { 0 } else { 65535 });
                        if *index != 0 {
                            assert_eq!(pixel, &[0, 0, 0, 65535]);
                        }
                    }
                }
                assert!(encode(&actual, &Default::default()).is_err());
                let mut destination = Vec::new();
                assert!(encode_writer(&actual, &Default::default(), &mut destination).is_err());
                assert!(destination.is_empty());

                // An invalid index solely in fdAT also obeys the selected policy.
                let mut frames_only = actual.clone();
                frames_only.image.samples = Samples::U8(vec![0; 289]);
                frames_only.palette.resize(mask + 1, [200, 100, 50]);
                let bytes = shorten_palette(&encode(&frames_only, &Default::default()).unwrap(), 1);
                let recovered = decode(&bytes).unwrap();
                assert_eq!(
                    decode_reader(OneByte(&bytes), &Default::default()).unwrap(),
                    recovered
                );
                assert!(decode_with_options(&bytes, &strict).is_err());
                assert!(decode_reader(OneByte(&bytes), &strict).is_err());
                assert!(event_rows(&bytes, strict).is_err());
                assert!(encode(&recovered, &Default::default()).is_err());
                let mut compositor = recovered.compositor(Limits::default()).unwrap();
                while compositor.next_frame().unwrap().is_some() {}
            }
        }
    }
}

#[test]
fn independent_palette_overflow_fixture() {
    // Constructed by Rust png, independently of the native writer. An index
    // of 1 is within the bit depth but outside the single-entry PLTE.
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::One);
    encoder.set_palette(&[17, 23, 42][..]);
    encoder.set_trns(&[0][..]);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[128]).unwrap();
    writer.finish().unwrap();
    let png = decode(&bytes).unwrap();
    assert_eq!(png.image.samples, Samples::U8(vec![1]));
    assert_eq!(png.palette, [[17, 23, 42]]);
    assert_eq!(
        png.to_rgba16(Limits::default()).unwrap().samples,
        Samples::U16(vec![0, 0, 0, 65535])
    );
    assert_eq!(
        decode_reader(OneByte(&bytes), &Default::default()).unwrap(),
        png
    );
}
