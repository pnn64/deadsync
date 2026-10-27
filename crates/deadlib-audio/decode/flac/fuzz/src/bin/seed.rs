use deadlib_audio_decode_flac::*;
use std::{fs, io::Cursor, path::Path};
fn write(root: &Path, target: &str, name: &str, data: &[u8]) {
    if data.len() <= 131072 {
        fs::write(root.join(target).join(name), data).unwrap();
    }
}
fn stream(root: &Path, name: &str, data: &[u8]) {
    let limits = Limits {
        metadata_bytes: 128 << 20,
        ..Default::default()
    };
    let meta = Metadata::parse(data, limits).unwrap();
    write(
        root,
        "primitives",
        name,
        &data[..meta.audio_offset.min(131072)],
    );
    let mut decoder = FrameDecoder::new(Some(meta.stream_info)).unwrap();
    let mut offset = meta.audio_offset;
    let mut frames = Vec::new();
    let mut total = 0;
    for i in 0..3 {
        if offset == data.len() {
            break;
        }
        let frame = decoder.decode(&data[offset..]).unwrap();
        let bytes = &data[offset..offset + frame.bytes];
        let mut raw = meta.stream_info.encode().unwrap().to_vec();
        raw.extend(bytes);
        write(root, "frame", &format!("{name}-{i}"), &raw);
        if frames.len() + bytes.len() + 42 > 131072 {
            break;
        }
        frames.extend(bytes);
        total += frame.pcm.samples() as u64;
        offset += frame.bytes;
    }
    let mut info = meta.stream_info;
    info.total_samples = total;
    info.md5 = [0; 16];
    let mut native = b"fLaC".to_vec();
    MetadataBlock {
        kind: 0,
        data: &info.encode().unwrap(),
    }
    .write(true, &mut native)
    .unwrap();
    native.extend_from_slice(&frames);
    write(root, "native", name, &native);
    let mut muxer = OggMuxer::new(Vec::new(), info, &[], OggWriteOptions::default()).unwrap();
    let mut offset = 0;
    while offset < frames.len() {
        let n = decoder.decode(&frames[offset..]).unwrap().bytes;
        muxer.write_frame(&frames[offset..offset + n]).unwrap();
        offset += n;
    }
    write(root, "ogg", name, &muxer.finish().unwrap().output);
}
fn main() {
    let mut args = std::env::args_os().skip(1);
    let root = std::path::PathBuf::from(args.next().expect("seed OUTPUT [OFFICIAL_CORPUS]"));
    assert!(!root.exists(), "seed output must be new");
    for target in ["primitives", "frame", "native", "ogg", "encode"] {
        fs::create_dir_all(root.join(target)).unwrap();
    }
    for bits in 4..=32u8 {
        for channels in 1..=8u8 {
            for level in [0, 5, 8] {
                let name = format!("generated-{bits}-{channels}-{level}");
                let mut seed = 193u32;
                let input: Vec<_> = (0..257 * channels as usize)
                    .map(|i| {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        let value = match (bits + channels + level) % 4 {
                            0 => seed as i32,
                            1 => {
                                if i % 2 == 0 {
                                    i32::MIN
                                } else {
                                    i32::MAX
                                }
                            }
                            2 => (i as i32).wrapping_mul(123456789),
                            _ => (((i / channels as usize) as f64 * 0.37).sin() * 1e9) as i32,
                        };
                        value >> (32 - bits)
                    })
                    .collect();
                let options = EncodeOptions {
                    bits_per_sample: bits,
                    channels,
                    compression_level: level,
                    block_size: 128,
                    variable_block_size: level == 8,
                    ..Default::default()
                };
                let mut encoder =
                    Encoder::with_metadata(Cursor::new(Vec::new()), options, &[], 4).unwrap();
                encoder.write_interleaved(&input).unwrap();
                let native = encoder.finish_seekable().unwrap().output.into_inner();
                stream(&root, &name, &native);
                // Preserve some complete SEEKTABLE/MD5 files for the native target.
                write(&root, "native", &format!("{name}-finalized"), &native);
                let mut raw = vec![bits - 4, channels - 1, level, 3, u8::from(level == 8), 17];
                raw.extend(input.iter().flat_map(|&x| (x << (32 - bits)).to_le_bytes()));
                raw.truncate(8192);
                write(&root, "encode", &name, &raw);
            }
        }
    }
    if let Some(corpus) = args.next() {
        for group in ["subset", "uncommon", "faulty"] {
            let mut paths: Vec<_> = fs::read_dir(Path::new(&corpus).join(group))
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| p.extension().is_some_and(|e| e == "flac"))
                .collect();
            paths.sort();
            for (i, path) in paths.iter().enumerate() {
                let data = fs::read(path).unwrap();
                let name = format!("official-{group}-{i}");
                if group != "faulty" && data.starts_with(b"fLaC") {
                    stream(&root, &name, &data);
                } else {
                    write(&root, "native", &name, &data[..data.len().min(131072)]);
                }
            }
        }
    }
    for target in ["primitives", "frame", "native", "ogg", "encode"] {
        println!(
            "{target}: {} seeds",
            fs::read_dir(root.join(target)).unwrap().count()
        );
    }
}
