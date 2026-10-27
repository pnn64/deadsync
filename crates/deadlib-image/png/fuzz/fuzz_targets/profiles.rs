#![no_main]
use deadlib_image_png::{
    ColorType::*, DecodeOptions, Image, Limits, Metadata, Png, Samples, decode_reader,
    decode_with_options, encode, encode_writer,
};
use libfuzzer_sys::fuzz_target;
use std::io::{Cursor, Read, Write};

const PROFILES: &[&[u8]] = &[
    include_bytes!("../../tests/fixtures/icc/sRGB-v2-magic.icc"),
    include_bytes!("../../tests/fixtures/icc/sRGB-v2-micro.icc"),
    include_bytes!("../../tests/fixtures/icc/sRGB-v2-nano.icc"),
    include_bytes!("../../tests/fixtures/icc/sRGB-v4.icc"),
    include_bytes!("../../tests/fixtures/icc/sGrey-v2-magic.icc"),
    include_bytes!("../../tests/fixtures/icc/sGrey-v2-micro.icc"),
    include_bytes!("../../tests/fixtures/icc/sGrey-v2-nano.icc"),
    include_bytes!("../../tests/fixtures/icc/sGrey-v4.icc"),
];
struct Fragments<'a>(&'a [u8], usize);
impl Read for Fragments<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let n = output.len().min(self.1);
        self.0.read(&mut output[..n])
    }
}
fuzz_target!(|input: &[u8]| {
    if input.len() < 8 || input.len() > 4096 {
        return;
    }
    let flags = input[0];
    let formats = [
        (Grayscale, 1),
        (Grayscale, 2),
        (Grayscale, 4),
        (Grayscale, 8),
        (Grayscale, 16),
        (Rgb, 8),
        (Rgb, 16),
        (Indexed, 1),
        (Indexed, 2),
        (Indexed, 4),
        (Indexed, 8),
        (GrayscaleAlpha, 8),
        (GrayscaleAlpha, 16),
        (Rgba, 8),
        (Rgba, 16),
    ];
    let (color, depth) = formats[input[2] as usize % formats.len()];
    let mut png = Png::new(Image {
        width: 1,
        height: 1,
        color_type: color,
        bit_depth: depth,
        samples: if depth == 16 {
            Samples::U16(vec![12345; color.channels()])
        } else {
            Samples::U8(vec![1; color.channels()])
        },
    });
    if color == Indexed {
        png.palette = vec![[0; 3], [255; 3]];
    }
    let kind = if flags & 1 == 0 { b"iCCP" } else { b"eXIf" };
    let mut payload = if flags & 2 != 0 {
        input[8..].to_vec()
    } else if flags & 1 == 0 {
        PROFILES[input[1] as usize % PROFILES.len()].to_vec()
    } else if input[1] & 1 == 0 {
        b"II\x2a\0\x08\0\0\0\0\0\0\0\0\0".to_vec()
    } else {
        b"MM\0\x2a\0\0\0\x08\0\0\0\0\0\0".to_vec()
    };
    if flags & 16 != 0 && flags & 1 == 0 && payload.len() > 9 {
        payload[9] = 0x40;
    }
    if flags & 32 != 0 && flags & 1 == 0 && payload.len() >= 132 {
        let count = u32::from_be_bytes(payload[128..132].try_into().unwrap()) as usize;
        if count <= (payload.len() - 132) / 12 {
            for n in 0..count / 2 {
                for b in 0..12 {
                    payload.swap(132 + n * 12 + b, 132 + (count - 1 - n) * 12 + b);
                }
            }
        }
    }
    if flags & 4 != 0 {
        let fields = if flags & 1 == 0 {
            &[0, 8, 12, 16, 20, 36, 64, 128, 132, 136, 140, 144, 148, 152][..]
        } else {
            &[0, 2, 4, 6, 8, 10, 12][..]
        };
        let offset = fields[input[3] as usize % fields.len()];
        if let Some(field) = payload.get_mut(offset..offset + 4) {
            field.copy_from_slice(&input[4..8]);
        }
    }
    if flags & 8 != 0 {
        payload.truncate(u16::from_le_bytes([input[4], input[5]]) as usize % (payload.len() + 1));
    }
    let mut data = if flags & 1 == 0 {
        let mut compressor = flate2::write::ZlibEncoder::new(
            Vec::new(),
            if flags & 64 == 0 {
                flate2::Compression::default()
            } else {
                flate2::Compression::none()
            },
        );
        compressor.write_all(&payload).unwrap();
        let mut data = b"Profile\0\0".to_vec();
        data.extend_from_slice(&compressor.finish().unwrap());
        data
    } else {
        payload
    };
    let base = encode(&png, &Default::default()).unwrap();
    let mut wire = base[..33].to_vec();
    wire.extend_from_slice(&(data.len() as u32).to_be_bytes());
    wire.extend_from_slice(kind);
    wire.append(&mut data);
    wire.extend_from_slice(&crc32fast::hash(&wire[37..]).to_be_bytes());
    wire.extend_from_slice(&base[33..]);
    let limits = Limits {
        max_input_bytes: 1 << 20,
        max_output_bytes: 1 << 16,
        max_metadata_bytes: if flags & 128 != 0 {
            1 + input[5] as usize * 4
        } else {
            1 << 16
        },
        max_chunks: 1024,
        max_frames: 16,
    };
    let options = DecodeOptions {
        limits,
        ..Default::default()
    };
    let expected = decode_with_options(&wire, &options);
    let streamed = decode_reader(Fragments(&wire, 1 + input[7] as usize % 31), &options);
    let Ok(actual) = expected else {
        assert!(streamed.is_err());
        return;
    };
    assert_eq!(streamed.unwrap(), actual);
    assert_eq!(actual.image, png.image);
    let metadata = actual.metadata(limits).unwrap();
    let mut output = Vec::new();
    if matches!(&metadata[0], Metadata::Unknown { .. }) {
        assert!(encode(&actual, &Default::default()).is_err());
        assert!(encode_writer(&actual, &Default::default(), &mut output).is_err());
        assert!(output.is_empty());
    } else {
        encode_writer(&actual, &Default::default(), &mut output).unwrap();
        assert_eq!(decode_with_options(&output, &options).unwrap(), actual);
        let reader = png::Decoder::new(Cursor::new(output)).read_info().unwrap();
        match &metadata[0] {
            Metadata::IccProfile { profile, .. } => assert_eq!(
                reader.info().icc_profile.as_deref(),
                Some(profile.as_slice())
            ),
            Metadata::Exif(data) => assert_eq!(
                reader.info().exif_metadata.as_deref(),
                Some(data.as_slice())
            ),
            _ => unreachable!(),
        }
    }
});
