//! Extract independent PNGSuite zlib streams and PNG files for fuzz campaigns.
//! An optional first argument creates a fresh corpus root instead of merging seeds.
#[allow(dead_code)]
#[path = "../tests/support/ancillary_order.rs"]
mod ancillary_order;
#[allow(dead_code)]
#[path = "../tests/support/apng_recovery.rs"]
mod apng_recovery;
#[allow(dead_code)]
#[path = "../tests/support/budgets.rs"]
mod budgets;
#[path = "../tests/support/chunks.rs"]
mod chunks;
#[allow(dead_code)]
#[path = "../tests/support/compositor.rs"]
mod compositor;
#[allow(dead_code)]
#[path = "../tests/support/hdr.rs"]
mod hdr;
#[path = "../tests/support/huffman.rs"]
mod huffman;
#[allow(dead_code)]
#[path = "../tests/support/named.rs"]
mod named;
#[allow(dead_code)]
#[path = "../tests/support/prefix.rs"]
mod prefix;
#[path = "../tests/support/text_body.rs"]
mod text_body;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn main() -> std::io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let corpus = if let Some(destination) = std::env::args_os().nth(1) {
        let path = PathBuf::from(destination);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir(&path)?;
        path
    } else {
        root.join("fuzz/corpus")
    };
    let png = corpus.join("decode");
    let zlib = corpus.join("zlib");
    let png_stream = corpus.join("png_stream");
    let inflate_stream = corpus.join("inflate_stream");
    let encode_stream = corpus.join("encode_stream");
    let profiles = corpus.join("profiles");
    let deflate_stream = corpus.join("deflate_stream");
    let huffman_dir = corpus.join("huffman");
    let reference_samples = corpus.join("reference_samples");
    fs::create_dir_all(&reference_samples)?;
    for format in 0..4 {
        for precision in 1..=16 {
            for flags in [0, 1, 2, 3, 4, 16, 32, 128] {
                let bits = if flags & 1 == 0 {
                    precision - 1
                } else {
                    precision
                };
                fs::write(
                    reference_samples
                        .join(format!("format-{format}-depth-{precision}-flags-{flags}")),
                    [format, 16, 12, flags, bits, bits, bits, bits, 4, 3, 7, 255],
                )?;
            }
        }
    }
    for format in 0..4 {
        for variant in 0..16 {
            for flags in [0, 2] {
                fs::write(
                    reference_samples.join(format!("mixed-{format}-{variant}-{flags}")),
                    [
                        format,
                        16,
                        12,
                        flags,
                        variant,
                        (variant + 5) % 16,
                        (variant + 10) % 16,
                        (variant + 15) % 16,
                        4,
                        3,
                        7,
                        255,
                    ],
                )?;
            }
        }
    }
    fs::create_dir_all(&huffman_dir)?;
    fs::create_dir_all(&zlib)?;
    let cases = huffman::all_cases();
    let seeds = huffman::seed_indices(&cases);
    for &index in &seeds {
        let mut input = (index as u32).to_le_bytes().to_vec();
        input.extend_from_slice(&[0, 2, 3, 0, 0, 0, 0, 0]);
        fs::write(huffman_dir.join(format!("case-{index}")), input)?;
        fs::write(zlib.join(format!("huffman-{index}")), &cases[index].bytes)?;
    }
    println!("Huffman: {} fixtures, {} seeds", cases.len(), seeds.len());
    fs::create_dir_all(&deflate_stream)?;
    for size in [32767, 32768, 32769, 65535, 65536, 131072] {
        let mut random = 0x7139_4a55u32;
        for style in 0..4 {
            let raw: Vec<_> = (0..size)
                .map(|n| match style {
                    0 => 0,
                    1 => {
                        random ^= random << 13;
                        random ^= random >> 17;
                        random ^= random << 5;
                        random as u8
                    }
                    2 => {
                        if n / 32768 % 2 == 0 {
                            (n % 7) as u8
                        } else {
                            (n * 71 + n / 13) as u8
                        }
                    }
                    _ => ((n % 32768) * 71 + (n % 32768) / 31) as u8,
                })
                .collect();
            fs::write(deflate_stream.join(format!("block-{size}-{style}")), raw)?;
        }
    }
    fs::create_dir_all(&profiles)?;
    for format in 0..15 {
        for profile in 0..8 {
            for flags in [0, 1, 2, 3, 4, 5, 8, 9, 16, 20, 32, 64, 65, 128, 129] {
                fs::write(
                    profiles.join(format!("profile-{format}-{profile}-{flags}")),
                    [flags, profile, format, 9, 0, 0, 0, 0],
                )?;
            }
        }
    }
    fs::create_dir_all(&encode_stream)?;
    for format in 0..15 {
        for flags in [0, 1, 2, 3, 4, 6, 12, 14, 16, 32, 64, 127] {
            for filter in 0..6 {
                fs::write(
                    encode_stream.join(format!("format-{format}-{flags}-{filter}")),
                    [format, 17, 13, flags, filter, 0, 7, 127, 255, 1, 42, 128],
                )?;
            }
        }
    }
    fs::create_dir_all(&png)?;
    fs::create_dir_all(&zlib)?;
    fs::create_dir_all(&png_stream)?;
    fs::create_dir_all(&inflate_stream)?;
    let decoder_limits = corpus.join("decoder_limits");
    fs::create_dir_all(&decoder_limits)?;
    for (name, bytes) in budgets::seeds() {
        fs::write(decoder_limits.join(name), bytes)?;
    }
    let mut prefix_count = 0;
    for case in prefix::fixtures() {
        for variant in 0..3 {
            let mut bytes = case.bytes.clone();
            if variant == 1 {
                bytes.extend(prefix::suffix(63, prefix_count));
            }
            if variant == 2 {
                bytes.pop();
            }
            let name = format!("prefix-{}-variant-{variant}", case.name);
            fs::write(zlib.join(&name), &bytes)?;
            fs::write(inflate_stream.join(&name), &bytes)?;
            prefix_count += 1;
        }
    }
    assert_eq!(prefix_count, 2220);
    println!(
        "Native resource budgets: 6376 seeds; independent mixed/one-bit prefixes: {prefix_count} seeds per raw corpus"
    );
    let compositor_inputs = corpus.join("compositor");
    fs::create_dir_all(&compositor_inputs)?;
    for format in 0..compositor::FORMATS.len() {
        for interlaced in [false, true] {
            for default_is_frame in [false, true] {
                for variant in 0..4 {
                    for recover_indices in [false, true] {
                        let mut bytes =
                            compositor::seed(format, interlaced, default_is_frame, variant);
                        if recover_indices {
                            bytes[3] |= 4;
                        }
                        fs::write(compositor_inputs.join(format!("format-{format}-adam7-{interlaced}-default-{default_is_frame}-pattern-{variant}-recover-{recover_indices}")), bytes)?;
                    }
                }
            }
        }
    }
    for (name, case) in compositor::fixtures() {
        let data = case.wire();
        let name = format!("compositor-{name}.png");
        fs::write(png.join(&name), &data)?;
        fs::write(png_stream.join(&name), &data)?;
    }
    // Independent Table 7 placements, with both valid and malformed ordering.
    // Keep a bounded pair subset in addition to all format/single placements.
    let mut order_cases = 0;
    for context in ancillary_order::contexts() {
        for kind in ancillary_order::NAMES {
            for slot in 0..6 {
                let mut entries = vec![ancillary_order::entry(context, kind, slot)];
                ancillary_order::companion(context, &mut entries);
                let data = ancillary_order::fixture(context, &entries, 3);
                let name = format!("ancillary-order-{order_cases}.png");
                fs::write(png.join(&name), &data)?;
                fs::write(png_stream.join(&name), &data)?;
                order_cases += 1;
            }
        }
        if context.color == 2 && context.depth == 8 {
            for first in ancillary_order::NAMES {
                for second in ancillary_order::NAMES {
                    for a in ancillary_order::PAIR_SLOTS {
                        for b in ancillary_order::PAIR_SLOTS {
                            let mut entries = vec![
                                ancillary_order::entry(context, first, a),
                                ancillary_order::entry(context, second, b),
                            ];
                            ancillary_order::companion(context, &mut entries);
                            let data = ancillary_order::fixture(context, &entries, 3);
                            let name = format!("ancillary-order-{order_cases}.png");
                            fs::write(png.join(&name), &data)?;
                            fs::write(png_stream.join(&name), &data)?;
                            order_cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(order_cases, 7884);
    for (format, color, channels) in [
        (0, deadlib_image_png::ColorType::Grayscale, 1),
        (1, deadlib_image_png::ColorType::Rgb, 3),
        (2, deadlib_image_png::ColorType::GrayscaleAlpha, 2),
        (3, deadlib_image_png::ColorType::Rgba, 4),
    ] {
        for variant in 0..16 {
            let bits: Vec<u8> = (0..channels)
                .map(|c| 1 + ((variant + c * 5) % 16) as u8)
                .collect();
            let values: Vec<u16> = (0..9 * 7 * channels)
                .map(|n| {
                    let maximum = (1u32 << bits[n % channels]) - 1;
                    (match n % 7 {
                        0 => 0,
                        1 => maximum,
                        _ => (n as u32 * 137 + 31) & maximum,
                    }) as u16
                })
                .collect();
            let reference = deadlib_image_png::Png::from_reference_samples(
                9,
                7,
                color,
                &bits,
                deadlib_image_png::Samples::U16(values),
            )
            .unwrap();
            for interlaced in [false, true] {
                for (mode, compression) in [
                    deadlib_image_png::zlib::Compression::Stored,
                    deadlib_image_png::zlib::Compression::Fast,
                ]
                .into_iter()
                .enumerate()
                {
                    let options = deadlib_image_png::EncodeOptions {
                        interlaced,
                        compression,
                        idat_chunk_size: 7,
                        ..Default::default()
                    };
                    let data = deadlib_image_png::encode(&reference, &options).unwrap();
                    let name = format!("reference-{format}-{variant}-{interlaced}-{mode}.png");
                    fs::write(png.join(&name), &data)?;
                    fs::write(png_stream.join(&name), &data)?;
                }
            }
        }
    }
    let chunk_cases = chunks::seeds();
    for (name, bytes) in &chunk_cases {
        // Case-sensitive names collide on Windows filesystems. Encode bytes in
        // the seed filename while preserving the distinct literal wire names.
        let label: String = name.bytes().map(|b| format!("{b:02x}")).collect();
        for directory in [&png, &png_stream] {
            fs::write(directory.join(format!("chunk-{label}")), bytes)?;
        }
    }
    println!("PNG chunk grammar: {} independent seeds", chunk_cases.len());
    let mut named_count = 0;
    for case in named::names().into_iter().filter(|case| case.c_reference) {
        for (index, form) in named::FORMS.into_iter().enumerate() {
            let bytes = form.fixture(&case.bytes, deadlib_image_png::ChunkPosition::BeforePalette);
            for directory in [&png, &png_stream] {
                fs::write(
                    directory.join(format!("named-{}-{index}", case.label)),
                    &bytes,
                )?;
            }
            named_count += 1;
        }
    }
    println!("Named PNG metadata: {named_count} independent seeds");
    let text_body_dir = corpus.join("text_body");
    fs::create_dir_all(&text_body_dir)?;
    for flags in 0u8..16 {
        fs::write(text_body_dir.join(format!("empty-{flags}")), [flags])?;
        for byte in 0u8..=255 {
            fs::write(
                text_body_dir.join(format!("byte-{flags}-{byte}")),
                [flags, byte],
            )?;
            if flags < 2 {
                let bytes = text_body::fixture(
                    &[byte],
                    flags != 0,
                    deadlib_image_png::ChunkPosition::BeforePalette,
                );
                for directory in [&png, &png_stream] {
                    fs::write(directory.join(format!("body-{flags}-{byte}")), &bytes)?;
                }
            }
        }
        for size in [79, 80, 4096] {
            for style in 0..4 {
                let mut text: Vec<_> = (0..size)
                    .map(|n| {
                        if style == 0 {
                            b'A'
                        } else {
                            match n % 192 {
                                0 => b'\n',
                                1..=95 => (31 + n % 192) as u8,
                                _ => (64 + n % 192) as u8,
                            }
                        }
                    })
                    .collect();
                if style == 2 {
                    text[size - 1] = 0x1b;
                }
                if style == 3 {
                    text[size / 2] = 0;
                }
                let mut input = vec![flags];
                input.extend(text);
                fs::write(
                    text_body_dir.join(format!("body-{flags}-{size}-{style}")),
                    input,
                )?;
            }
        }
    }
    println!("Latin-1 text bodies: 512 PNG seeds and 4304 structured seeds");
    let mut animation_count = 0;
    for (color, depth) in apng_recovery::formats() {
        for interlaced in [false, true] {
            for default_is_frame in [false, true] {
                let fixture =
                    apng_recovery::Fixture::new(color, depth, interlaced, default_is_frame);
                let mut cases = fixture.cases();
                cases.push(("valid".into(), fixture.wire()));
                for (name, bytes) in cases {
                    let label = format!(
                        "apng-{}-{depth}-{}-{}-{name}",
                        color as u8,
                        u8::from(interlaced),
                        u8::from(default_is_frame)
                    );
                    for directory in [&png, &png_stream] {
                        fs::write(directory.join(&label), &bytes)?;
                    }
                    animation_count += 1;
                }
            }
        }
    }
    println!("Independent APNG validity/recovery: {animation_count} seeds");
    let mut prefix_count = 0;
    for (color, depth) in apng_recovery::formats() {
        for interlaced in [false, true] {
            let mut fixture = apng_recovery::Fixture::new(color, depth, interlaced, true);
            let ac = fixture
                .chunks
                .iter()
                .position(|c| &c.kind == b"acTL")
                .unwrap();
            let fc = fixture
                .chunks
                .iter()
                .position(|c| &c.kind == b"fcTL")
                .unwrap();
            fixture.chunks.swap(ac, fc);
            let mut cases = fixture.cases();
            cases.push(("valid".into(), fixture.wire()));
            for (name, bytes) in cases {
                let label = format!(
                    "apng-prefctl-{}-{depth}-{}-{name}",
                    color as u8,
                    u8::from(interlaced)
                );
                for directory in [&png, &png_stream] {
                    fs::write(directory.join(&label), &bytes)?;
                }
                prefix_count += 1;
            }
        }
    }
    println!("APNG fcTL before acTL: {prefix_count} seeds");
    let mut wpt_count = 0;
    for entry in fs::read_dir(root.join("tests/fixtures/wpt-apng/support"))? {
        let path = entry?.path();
        if path.extension().is_some_and(|v| v == "png") {
            let name = format!("wpt-{}", path.file_name().unwrap().to_string_lossy());
            let bytes = fs::read(path)?;
            for directory in [&png, &png_stream] {
                fs::write(directory.join(&name), &bytes)?;
            }
            wpt_count += 1;
        }
    }
    println!("Pinned WPT APNG/reference images: {wpt_count} seeds");
    let mut hdr_seeds = Vec::new();
    for (label, value) in hdr::byte_cases().chain(hdr::boundary_cases()) {
        hdr_seeds.push((label, value.fixture(2, 8, false)));
    }
    let value = hdr::Hdr::default();
    for (color, depth) in hdr::FORMATS {
        for interlaced in [false, true] {
            hdr_seeds.push((
                format!("format-{color}-{depth}-{}", u8::from(interlaced)),
                value.fixture(color, depth, interlaced),
            ));
        }
    }
    let payloads = value.payloads();
    for target in [1, 2] {
        for len in 0..=65 {
            let mut data = payloads.clone();
            data[target].1.resize(len, 0);
            let extra: Vec<_> = data.iter().map(|(k, d)| chunks::chunk(*k, d)).collect();
            hdr_seeds.push((
                format!("length-{target}-{len}"),
                hdr::fixture(3, 2, false, &extra),
            ));
        }
    }
    let core = hdr::core(3, 2, false);
    let extra: Vec<_> = payloads.iter().map(|(k, d)| chunks::chunk(*k, d)).collect();
    let unordered = [&extra[0], &extra[1], &extra[2], &core[1], &core[2]];
    chunks::permutations(&mut [0, 1, 2, 3, 4], 0, &mut |order| {
        let mut c = vec![core[0].clone()];
        c.extend(order.iter().map(|&i| unordered[i].clone()));
        c.push(core[3].clone());
        let label: String = order.iter().map(|i| char::from(b'0' + *i as u8)).collect();
        hdr_seeds.push((format!("order-{label}"), chunks::file(&c)));
    });
    for (label, bytes) in &hdr_seeds {
        for directory in [&png, &png_stream] {
            fs::write(directory.join(format!("hdr-{label}")), bytes)?;
        }
    }
    println!("Independent HDR fields/grammar: {} seeds", hdr_seeds.len());
    for entry in fs::read_dir(root.join("tests/fixtures/pngsuite"))? {
        let path = entry?.path();
        if path.extension().is_none_or(|v| v != "png") {
            continue;
        }
        let name = path.file_name().unwrap();
        let bytes = fs::read(&path)?;
        fs::write(png.join(name), &bytes)?;
        fs::write(png_stream.join(name), &bytes)?;
        if deadlib_image_png::decode(&bytes).is_err() {
            continue;
        }
        let mut compressed = Vec::new();
        let mut pos = 8;
        while pos + 12 <= bytes.len() {
            let len = u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            if &bytes[pos + 4..pos + 8] == b"IDAT" {
                compressed.extend_from_slice(&bytes[pos + 8..pos + 8 + len]);
            }
            pos += len + 12;
        }
        fs::write(zlib.join(name), &compressed)?;
        fs::write(inflate_stream.join(name), &compressed)?;
        let (mut raw, _) = deadlib_image_png::zlib::decompress_prefix(&compressed, 1 << 20)
            .map_err(std::io::Error::other)?;
        raw.truncate(131072);
        fs::write(deflate_stream.join(name), raw)?;
    }
    fs::copy(
        root.join("tests/fixtures/apng/basic_f20.png"),
        png.join("basic_f20.png"),
    )?;
    fs::copy(
        root.join("tests/fixtures/apng/basic_f20.png"),
        png_stream.join("basic_f20.png"),
    )?;
    Ok(())
}
