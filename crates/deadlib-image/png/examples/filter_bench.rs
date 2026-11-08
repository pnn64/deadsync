//! Whole PNG decoding with independent filters and stored DEFLATE.
use deadlib_image_png::{ColorType, Image, Samples, decode, decode_reader};
use std::{hint::black_box, io::Write, time::Instant};

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i32::from(a) + i32::from(b) - i32::from(c);
    let distances = [
        (p - i32::from(a)).abs(),
        (p - i32::from(b)).abs(),
        (p - i32::from(c)).abs(),
    ];
    if distances[0] <= distances[1] && distances[0] <= distances[2] {
        a
    } else if distances[1] <= distances[2] {
        b
    } else {
        c
    }
}

fn chunk(output: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(name);
    output.extend_from_slice(data);
    let mut crc = flate2::Crc::new();
    crc.update(name);
    crc.update(data);
    output.extend_from_slice(&crc.sum().to_be_bytes());
}

fn fixture(color: ColorType, depth: u8, noise: bool, filter: u8) -> (Image, Vec<u8>, Vec<u8>) {
    let (width, height) = (513, 257);
    let channels = color.channels();
    let count = width * height * channels;
    let mut random = 0x4fe3_a279u32;
    let values: Vec<u16> = (0..count)
        .map(|n| {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            if noise {
                random as u16
            } else {
                ((n % channels) * 8192 + (n / channels / width) * 137 + (n / channels % width) * 71)
                    as u16
            }
        })
        .collect();
    let samples = if depth == 16 {
        Samples::U16(values)
    } else {
        Samples::U8(values.iter().map(|&v| v as u8).collect())
    };
    let wire: Vec<u8> = match &samples {
        Samples::U8(v) => v.clone(),
        Samples::U16(v) => v.iter().flat_map(|v| v.to_be_bytes()).collect(),
    };
    let image = Image {
        width: width as u32,
        height: height as u32,
        color_type: color,
        bit_depth: depth,
        samples,
    };
    let bpp = channels * usize::from(depth / 8);
    let stride = width * bpp;
    let zeros = vec![0; stride];
    let mut filtered = Vec::with_capacity(wire.len() + height);
    for y in 0..height {
        filtered.push(filter);
        let row = &wire[y * stride..(y + 1) * stride];
        let up = if y == 0 {
            zeros.as_slice()
        } else {
            &wire[(y - 1) * stride..y * stride]
        };
        for n in 0..stride {
            let a = if n < bpp { 0 } else { row[n - bpp] };
            let c = if n < bpp { 0 } else { up[n - bpp] };
            let predictor = match filter {
                0 => 0,
                1 => a,
                2 => up[n],
                3 => ((u16::from(a) + u16::from(up[n])) / 2) as u8,
                4 => paeth(a, up[n], c),
                _ => unreachable!(),
            };
            filtered.push(row[n].wrapping_sub(predictor));
        }
    }
    let mut compressor = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::none());
    compressor.write_all(&filtered).unwrap();
    let idat = compressor.finish().unwrap();
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&image.width.to_be_bytes());
    header.extend_from_slice(&image.height.to_be_bytes());
    header.extend_from_slice(&[depth, color as u8, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &idat);
    chunk(&mut png, b"IEND", &[]);
    (image, wire, png)
}

fn reference(bytes: &[u8]) -> Vec<u8> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    let mut output = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut output).unwrap();
    output.truncate(info.buffer_size());
    reader.finish().unwrap();
    output
}

fn time<T>(mut operation: impl FnMut() -> T, iterations: usize) -> u128 {
    for _ in 0..3 {
        black_box(operation());
    }
    let began = Instant::now();
    for _ in 0..iterations {
        black_box(operation());
    }
    began.elapsed().as_nanos() / iterations as u128
}

fn main() {
    let iterations = std::env::var("DEADLIB_PNG_BENCH_ITERS")
        .unwrap_or_else(|_| "10".into())
        .parse::<usize>()
        .unwrap()
        .max(1);
    println!(
        "case\tcodec\toperation\titerations\tencoded_bytes\tsample_bytes\tns_per_image\tMiB_per_second\timage_pipeline_bytes"
    );
    for (name, color) in [
        ("gray", ColorType::Grayscale),
        ("ga", ColorType::GrayscaleAlpha),
        ("rgb", ColorType::Rgb),
        ("rgba", ColorType::Rgba),
    ] {
        for depth in [8, 16] {
            for noise in [false, true] {
                for filter in 0..5 {
                    let (image, wire, bytes) = fixture(color, depth, noise, filter);
                    assert_eq!(decode(&bytes).unwrap().image, image);
                    assert_eq!(
                        decode_reader(bytes.as_slice(), &Default::default())
                            .unwrap()
                            .image,
                        image
                    );
                    assert_eq!(reference(&bytes), wire);
                    let pattern = if noise { "noise" } else { "gradient" };
                    let case = format!("{pattern}-{name}{depth}-filter{filter}");
                    for (codec, operation, ns) in [
                        (
                            "deadlib-image-png",
                            "decode",
                            time(|| decode(black_box(&bytes)).unwrap(), iterations),
                        ),
                        (
                            "png-0.18.1",
                            "decode",
                            time(|| reference(black_box(&bytes)), iterations),
                        ),
                        (
                            "deadlib-image-png",
                            "decode-reader",
                            time(
                                || {
                                    decode_reader(black_box(bytes.as_slice()), &Default::default())
                                        .unwrap()
                                },
                                iterations,
                            ),
                        ),
                    ] {
                        let rate = wire.len() as f64 * 1e9 / ns as f64 / 1048576.0;
                        println!(
                            "{case}\t{codec}\t{operation}\t{iterations}\t{}\t{}\t{ns}\t{rate:.3}\t0",
                            bytes.len(),
                            wire.len()
                        );
                    }
                }
            }
        }
    }
}
