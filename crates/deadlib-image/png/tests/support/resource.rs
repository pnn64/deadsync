//! Native decoder parity under mutable resource budgets and reader behavior.
//! The 20-byte control header is separate from the PNG datastream.
use deadlib_image_png::{
    AnimationPolicy, DecodeEvent, DecodeOptions, DecodeStatus, Error, Image, ImageKind, Limits,
    PalettePolicy, Row, RowSamples, Samples, StreamDecoder, decode_reader, decode_with_options,
};
use std::io::{self, Read};

pub const HEADER_BYTES: usize = 20;

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
    fragment: usize,
    calls: usize,
    mode: u8,
    fault_at: usize,
    injected: bool,
}
impl Read for Reader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        assert!(self.calls < self.bytes.len() * 4 + 1024);
        if self.mode != 0 && self.calls % 2 == 1 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.mode >= 2 && self.offset == self.fault_at {
            self.injected = true;
            return if self.mode == 2 {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "injected decoder reader failure",
                ))
            } else {
                Ok(output.len() + 1)
            };
        }
        let remaining = if self.mode >= 2 {
            self.fault_at - self.offset
        } else {
            usize::MAX
        };
        let count = output
            .len()
            .min(self.fragment)
            .min(self.bytes.len() - self.offset)
            .min(remaining);
        output[..count].copy_from_slice(&self.bytes[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

fn samples(image: &Image, row: Row<'_>) {
    let channels = image.color_type.channels();
    for pixel in 0..row.width as usize {
        let at =
            (row.y as usize * image.width as usize + row.x as usize + pixel * row.x_step as usize)
                * channels;
        let start = pixel * channels;
        match (&image.samples, row.samples) {
            (Samples::U8(expected), RowSamples::U8(actual)) => assert_eq!(
                &expected[at..at + channels],
                &actual[start..start + channels]
            ),
            (Samples::U16(expected), RowSamples::U16(actual)) => assert_eq!(
                &expected[at..at + channels],
                &actual[start..start + channels]
            ),
            _ => panic!("native sample representation"),
        }
    }
}

pub fn check(input: &[u8], crc: impl Fn(&[u8]) -> u32) {
    if input.len() < HEADER_BYTES || input.len() > 65536 {
        return;
    }
    let u32_at = |at| u32::from_be_bytes(input[at..at + 4].try_into().unwrap()) as usize;
    let u16_at = |at| u16::from_be_bytes(input[at..at + 2].try_into().unwrap()) as usize;
    let flags = input[16];
    let options = DecodeOptions {
        limits: Limits {
            max_input_bytes: u32_at(0).min(1 << 20),
            max_output_bytes: u32_at(4).min(8 << 20),
            max_metadata_bytes: u32_at(8).min(1 << 16),
            max_chunks: u16_at(12).min(4096),
            max_frames: u16_at(14).min(256),
        },
        animation_policy: if flags & 1 == 0 {
            AnimationPolicy::Strict
        } else {
            AnimationPolicy::RecoverDefault
        },
        palette_policy: if flags & 2 == 0 {
            PalettePolicy::RecoverBlack
        } else {
            PalettePolicy::Strict
        },
    };
    let fragment = 1 + usize::from(input[17] % 32);
    let mut bytes = input[HEADER_BYTES..].to_vec();
    if flags & 16 != 0 {
        let mut offset = 8;
        while offset + 12 <= bytes.len() {
            let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
            let Some(end) = offset.checked_add(12).and_then(|at| at.checked_add(length)) else {
                break;
            };
            if end > bytes.len() {
                break;
            }
            let crc = crc(&bytes[offset + 4..end - 4]);
            bytes[end - 4..end].copy_from_slice(&crc.to_be_bytes());
            offset = end;
        }
    }
    let expected = decode_with_options(&bytes, &options);
    let mut reader = Reader {
        bytes: &bytes,
        offset: 0,
        fragment,
        calls: 0,
        mode: (flags >> 2) & 3,
        fault_at: u16_at(18).min(bytes.len()),
        injected: false,
    };
    let actual = decode_reader(&mut reader, &options);
    if reader.injected {
        assert!(matches!(actual, Err(Error::Io(_))));
    } else {
        match (&expected, actual) {
            (Ok(expected), Ok(actual)) => assert_eq!(expected, &actual),
            (Err(_), Err(_)) => (),
            (expected, actual) => {
                panic!("buffered/reader acceptance differs: {expected:?} vs {actual:?}")
            }
        }
    }
    let mut decoder = StreamDecoder::new(options);
    let mut offset = 0;
    // A highly compressed image can emit many rows without consuming input.
    // Every row contains at least one native sample; cumulative sample bytes
    // are capped by max_output_bytes. Other events are bounded by input size.
    for _ in 0..bytes.len() * 4 + options.limits.max_output_bytes + 1024 {
        let end = bytes.len().min(offset + fragment);
        let progress = match decoder.process(&bytes[offset..end]) {
            Ok(progress) => progress,
            Err(error) => {
                assert!(expected.is_err(), "push rejected valid PNG: {error}");
                assert!(decoder.total_in() <= options.limits.max_input_bytes);
                assert_eq!(decoder.finish(), Err(error.clone()));
                assert_eq!(decoder.process(&[]), Err(error));
                assert!(!decoder.is_done());
                return;
            }
        };
        offset += progress.consumed;
        assert_eq!(decoder.total_in(), offset);
        assert!(
            decoder.peak_image_buffer_bytes() <= 6 * options.limits.max_output_bytes + 128 * 1024
        );
        if let Ok(png) = &expected
            && let Some(DecodeEvent::Row(row)) = decoder.event()
        {
            let image = match row.image {
                ImageKind::Default => &png.image,
                ImageKind::AnimationFrame(index) => {
                    if let Some(animation) = &png.animation {
                        animation.frames[index as usize].image.as_ref().unwrap()
                    } else {
                        assert!(png.animation_error.is_some());
                        continue;
                    }
                }
            };
            samples(image, row);
        }
        match progress.status {
            DecodeStatus::Done => {
                assert_eq!(offset == bytes.len(), expected.is_ok());
                decoder.finish().unwrap();
                if let Ok(png) = &expected {
                    assert_eq!(decoder.animation_error(), png.animation_error.as_ref());
                }
                return;
            }
            DecodeStatus::NeedInput if offset == bytes.len() => {
                assert!(expected.is_err());
                assert!(decoder.finish().is_err());
                return;
            }
            DecodeStatus::NeedInput => assert_eq!(offset, end),
            DecodeStatus::Event => (),
        }
    }
    panic!("native decoder stalled");
}
