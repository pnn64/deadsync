use deadlib_image_bmp::{Bitmap, Pixels};
use std::{
    fs,
    hint::black_box,
    io::{self, BufWriter, Write},
    path::Path,
    time::{Duration, Instant},
};

#[path = "../../fixtures.rs"]
mod fixtures;
mod foreign;

unsafe extern "system" {
    fn GetCurrentProcess() -> *mut std::ffi::c_void;
    fn SetProcessAffinityMask(process: *mut std::ffi::c_void, mask: usize) -> i32;
    fn QueryProcessCycleTime(process: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
}

fn clocks() -> u64 {
    let mut count = 0;
    assert_ne!(
        unsafe { QueryProcessCycleTime(GetCurrentProcess(), &mut count) },
        0
    );
    count
}

fn measure<T>(span: Duration, count: usize, mut task: impl FnMut() -> T) -> (u64, u64) {
    if span.is_zero() {
        return (0, 0);
    }
    let (mut nanos, mut cycles) = ([0; 5], [0; 5]);
    let batch = if count < 100_000 { 16 } else { 1 };
    for (n, c) in nanos.iter_mut().zip(&mut cycles) {
        let start = Instant::now();
        let initial = clocks();
        let mut iterations = 0;
        loop {
            for _ in 0..batch {
                drop(black_box(task()));
            }
            iterations += batch;
            if start.elapsed() >= span {
                break;
            }
        }
        *c = (clocks() - initial) / iterations;
        *n = (start.elapsed().as_nanos() / u128::from(iterations)) as u64;
    }
    nanos.sort_unstable();
    cycles.sort_unstable();
    (nanos[2], cycles[2])
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Codec {
    Deadlib,
    Image,
    Oxide,
    Foreign(u32),
}
const DECODE: [Codec; 9] = [
    Codec::Deadlib,
    Codec::Image,
    Codec::Oxide,
    Codec::Foreign(0),
    Codec::Foreign(1),
    Codec::Foreign(2),
    Codec::Foreign(3),
    Codec::Foreign(4),
    Codec::Foreign(5),
];
const ENCODE: [Codec; 7] = [
    Codec::Deadlib,
    Codec::Image,
    Codec::Oxide,
    Codec::Foreign(0),
    Codec::Foreign(2),
    Codec::Foreign(3),
    Codec::Foreign(5),
];

impl Codec {
    fn name(self) -> &'static str {
        match self {
            Self::Deadlib => "deadlib",
            Self::Image => "image",
            Self::Oxide => "oxideav-bmp",
            Self::Foreign(i) => ["stb", "wuffs", "sdl3", "wic", "libnsbmp", "ffmpeg"][i as usize],
        }
    }
}

struct Libraries {
    core: foreign::Bridge,
    ffmpeg: Option<foreign::Bridge>,
}
impl Libraries {
    fn get(&self, id: u32) -> &foreign::Bridge {
        if id == 5 {
            self.ffmpeg.as_ref().unwrap()
        } else {
            &self.core
        }
    }
    fn available(&self, codec: Codec) -> bool {
        codec != Codec::Foreign(5) || self.ffmpeg.is_some()
    }
}

enum Bytes<'a> {
    Vec(Vec<u8>),
    Foreign(foreign::Buffer<'a>),
}
impl Bytes<'_> {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Vec(bytes) => bytes,
            Self::Foreign(bytes) => bytes.bytes(),
        }
    }
}

fn decode<'a>(
    lib: &'a Libraries,
    codec: Codec,
    bytes: &[u8],
    length: usize,
) -> Result<((u32, u32), Bytes<'a>), String> {
    let input = &bytes[..length];
    match codec {
        Codec::Deadlib => {
            let bmp = deadlib_image_bmp::decode(input).map_err(|e| e.to_string())?;
            let shape = (bmp.width, bmp.height);
            Ok((
                shape,
                Bytes::Vec(bmp.into_rgba8().map_err(|e| e.to_string())?),
            ))
        }
        Codec::Image => {
            let bmp = image::load_from_memory_with_format(input, image::ImageFormat::Bmp)
                .map_err(|e| e.to_string())?
                .into_rgba8();
            Ok((bmp.dimensions(), Bytes::Vec(bmp.into_raw())))
        }
        Codec::Oxide => {
            let bmp = oxideav_bmp::decode_rgba8(input).map_err(|e| e.to_string())?;
            Ok(((bmp.width, bmp.height), Bytes::Vec(bmp.data)))
        }
        Codec::Foreign(id) => {
            let bmp = lib.get(id).decode(id, bytes, length)?;
            Ok((bmp.shape(), Bytes::Foreign(bmp)))
        }
    }
}

struct Source {
    bmp: Bitmap,
    oxide: oxideav_bmp::BmpImage,
    channels: u32,
    top: bool,
}
impl Source {
    fn from_rgba(rgba: &[u8], w: u32, h: u32, format: &str, top: bool) -> Self {
        let (pixels, kind, n) = if format.starts_with("rgba") {
            (
                Pixels::Rgba(rgba.to_vec()),
                oxideav_bmp::BmpPixelFormat::Rgba,
                4,
            )
        } else {
            let rgb = rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect();
            (Pixels::Rgb(rgb), oxideav_bmp::BmpPixelFormat::Rgb24, 3)
        };
        let bmp = Bitmap {
            width: w,
            height: h,
            pixels,
        };
        let oxide =
            oxideav_bmp::BmpImage::packed(w, h, kind, w as usize * n as usize, raw(&bmp).to_vec())
                .unwrap();
        Self {
            bmp,
            oxide,
            channels: n,
            top,
        }
    }
}

fn raw(bmp: &Bitmap) -> &[u8] {
    match &bmp.pixels {
        Pixels::Rgb(v) | Pixels::Rgba(v) => v,
        _ => unreachable!(),
    }
}

fn encode<'a>(lib: &'a Libraries, codec: Codec, src: &Source) -> Result<Bytes<'a>, String> {
    let (w, h, n) = (src.bmp.width, src.bmp.height, src.channels);
    let data = raw(&src.bmp);
    if src.top && !matches!(codec, Codec::Deadlib | Codec::Oxide) {
        return Err("top-down output unavailable".into());
    }
    match codec {
        Codec::Deadlib => deadlib_image_bmp::encode_with_options(
            &src.bmp,
            deadlib_image_bmp::EncodeOptions {
                top_down: src.top,
                ..Default::default()
            },
        )
        .map(Bytes::Vec)
        .map_err(|e| e.to_string()),
        Codec::Image => {
            let mut dst = Vec::new();
            image::codecs::bmp::BmpEncoder::new(&mut dst)
                .encode(
                    data,
                    w,
                    h,
                    if n == 3 {
                        image::ExtendedColorType::Rgb8
                    } else {
                        image::ExtendedColorType::Rgba8
                    },
                )
                .map_err(|e| e.to_string())?;
            Ok(Bytes::Vec(dst))
        }
        Codec::Oxide => {
            let mut opts = oxideav_bmp::EncodeOptions::new().with_top_down(src.top);
            if n == 4 {
                opts = opts.with_bitfields(oxideav_bmp::BmpBitfields::BGRA8888);
            }
            oxideav_bmp::encode(&src.oxide, &opts)
                .map(Bytes::Vec)
                .map_err(|e| e.to_string())
        }
        Codec::Foreign(id) => lib.get(id).encode(id, data, w, h, n).map(Bytes::Foreign),
    }
}

fn prepare(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    let mut dec = BufWriter::new(fs::File::create(dir.join("decode.tsv")).unwrap());
    let mut enc = BufWriter::new(fs::File::create(dir.join("encode.tsv")).unwrap());
    for layout in fixtures::LAYOUTS {
        for (w, h) in [(1, 1), (17, 9), (511, 257), (1920, 1080)] {
            for top in [false, true] {
                if top && !layout.supports_top_down() {
                    continue;
                }
                let case = fixtures::build(*layout, w, h, top);
                assert_eq!(
                    (case.expected.width, case.expected.height),
                    (w as u32, h as u32)
                );
                let bmp = dir.join(format!("{}.bmp", case.name));
                let rgba = dir.join(format!("{}.rgba", case.name));
                fs::write(&bmp, &case.bytes).unwrap();
                fs::write(&rgba, &case.rgba).unwrap();
                writeln!(
                    dec,
                    "synthetic\t{}\t{}\t{w}\t{h}\t{}\t{}\tfixture/{}",
                    layout.name,
                    case.name,
                    bmp.display(),
                    rgba.display(),
                    case.policy
                )
                .unwrap();
                if !top && w > 1 && matches!(layout.name, "rgb24" | "rgba32" | "indexed8") {
                    let format = if layout.name == "indexed8" {
                        "rgb24-from-palette"
                    } else {
                        layout.name
                    };
                    for direction in ["bottom", "top"] {
                        writeln!(
                            enc,
                            "synthetic\t{format}\t{}-{direction}\t{w}\t{h}\t{}\t{direction}",
                            case.name,
                            rgba.display()
                        )
                        .unwrap();
                    }
                    if format == "rgba32" {
                        let transparent = dir.join(format!("{}-transparent.rgba", case.name));
                        let mut bytes = case.rgba;
                        for pixel in bytes.as_chunks_mut::<4>().0 {
                            pixel[3] = 0;
                        }
                        fs::write(&transparent, bytes).unwrap();
                        writeln!(
                            enc,
                            "synthetic\trgba32-transparent\t{}-transparent\t{w}\t{h}\t{}\tbottom",
                            case.name,
                            transparent.display()
                        )
                        .unwrap();
                    }
                }
            }
        }
    }
}

fn status(
    actual: ((u32, u32), Bytes<'_>),
    shape: (u32, u32),
    expected: &[u8],
) -> Result<(), String> {
    let (dims, pixels) = actual;
    if dims != shape {
        return Err(format!("shape differs {}x{}", dims.0, dims.1));
    }
    if expected.is_empty() {
        return Err("accepted without an independent oracle".to_owned());
    }
    let pixels = pixels.bytes();
    if pixels.len() != expected.len() {
        return Err("output length differs".into());
    }
    let diffs = pixels.iter().zip(expected).filter(|(a, b)| a != b).count();
    if diffs == 0 {
        Ok(())
    } else {
        Err(format!("{diffs} sample bytes differ"))
    }
}

fn wire(data: &[u8]) -> Option<(u32, u16, u32, bool)> {
    if !data.starts_with(b"BM") {
        return None;
    }
    let u32_at = |i| Some(u32::from_le_bytes(data.get(i..i + 4)?.try_into().ok()?));
    let dib = u32_at(14)?;
    let depth = u16::from_le_bytes(data.get(28..30)?.try_into().ok()?);
    Some((dib, depth, u32_at(30)?, (u32_at(22)? as i32) < 0))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap();
    let path = args.next().unwrap();
    if mode == "prepare" {
        prepare(Path::new(&path));
        return;
    }
    let dir = std::path::PathBuf::from(args.next().unwrap());
    let verify = args.next().is_some_and(|s| {
        assert_eq!(s, "--verify-only");
        true
    });
    assert!(matches!(mode.as_str(), "decode" | "encode") && args.next().is_none());
    let lib = Libraries {
        core: foreign::Bridge::load(&dir.join("bench_bmp.dll")),
        ffmpeg: dir
            .join("bench_bmp_ffmpeg.dll")
            .is_file()
            .then(|| foreign::Bridge::load(&dir.join("bench_bmp_ffmpeg.dll"))),
    };
    let span = Duration::from_millis(if verify {
        0
    } else {
        std::env::var("BMP_BENCH_MS").map_or(6, |s| s.parse().unwrap())
    });
    if !verify {
        let cpu: u32 = std::env::var("BMP_BENCH_CPU").map_or(2, |s| s.parse().unwrap());
        assert_ne!(
            unsafe { SetProcessAffinityMask(GetCurrentProcess(), 1 << cpu) },
            0
        );
    }
    let mut out = BufWriter::new(io::stdout().lock());
    writeln!(out, "origin\tgroup\tcase\tw\th\tcodec\tinput_bytes\toutput_bytes\tns\tcycles\tverifier\twire\tstatus").unwrap();
    let manifest = fs::read_to_string(path).unwrap();
    let reversed = std::env::var_os("BMP_BENCH_REVERSE").is_some();
    let mut lines: Vec<_> = manifest.lines().collect();
    if reversed {
        lines.reverse();
    }
    for (index, line) in lines.into_iter().enumerate() {
        let parts: Vec<_> = line.split('\t').collect();
        let [origin, group, name, w, h, input, expected, oracle @ ..] = parts.as_slice() else {
            panic!("bad row {line}");
        };
        let shape = (w.parse::<u32>().unwrap(), h.parse::<u32>().unwrap());
        let count = shape.0 as usize * shape.1 as usize;
        let encoding = mode == "encode";
        let rgba = if encoding {
            fs::read(input).unwrap()
        } else if *expected != "-" {
            fs::read(expected).unwrap()
        } else {
            Vec::new()
        };
        let source =
            encoding.then(|| Source::from_rgba(&rgba, shape.0, shape.1, group, *expected == "top"));
        let mut data = if encoding {
            Vec::new()
        } else {
            fs::read(input).unwrap()
        };
        let real_len = data.len();
        data.resize(real_len + 64, 0); // FFmpeg may read its required padding.
        let codecs: &[Codec] = if encoding { &ENCODE } else { &DECODE };
        for offset in 0..codecs.len() {
            let offset = if reversed {
                codecs.len() - 1 - offset
            } else {
                offset
            };
            let codec = codecs[(offset + index) % codecs.len()];
            if !lib.available(codec) {
                continue;
            }
            let mut verifier = oracle.first().copied().unwrap_or("-");
            let mut output_wire = String::from("-");
            let result = if let Some(src) = &source {
                encode(&lib, codec, src).and_then(|bmp| {
                    let contents = bmp.bytes();
                    let (dib, depth, compression, top) =
                        wire(contents).ok_or("missing Windows BMP output header")?;
                    output_wire = format!(
                        "{dib}/{depth}/{compression}/{}",
                        if top { "top" } else { "bottom" }
                    );
                    if depth != (src.channels * 8) as u16 || top != src.top {
                        return Err("emitted different depth or orientation".into());
                    }
                    let size = contents.len();
                    let mut padded = contents.to_vec();
                    padded.resize(size + 64, 0);
                    let checks = [
                        Codec::Deadlib,
                        Codec::Oxide,
                        Codec::Foreign(3),
                        Codec::Image,
                        Codec::Foreign(1),
                        Codec::Foreign(2),
                    ];
                    let agreed =
                        checks
                            .into_iter()
                            .filter(|&check| check != codec)
                            .find(|&check| {
                                decode(&lib, check, &padded, size)
                                    .and_then(|image| status(image, shape, &rgba))
                                    .is_ok()
                            });
                    if let Some(check) = agreed {
                        verifier = check.name();
                    } else {
                        return Err("no independent exact round trip".into());
                    }
                    drop(bmp);
                    let (ns, cycles) =
                        measure(span, count, || encode(&lib, codec, black_box(src)).unwrap());
                    Ok((size, ns, cycles))
                })
            } else {
                decode(&lib, codec, &data, real_len).and_then(|image| {
                    status(image, shape, &rgba)?;
                    let (ns, cycles) = measure(span, count, || {
                        decode(&lib, codec, black_box(&data), real_len).unwrap()
                    });
                    Ok((count * 4, ns, cycles))
                })
            };
            let (bytes, ns, cycles, status) = match result {
                Ok((n, ns, c)) => (n, ns, c, "ok".into()),
                Err(e) => (0, 0, 0, e.replace(['\t', '\r', '\n'], " ")),
            };
            let input_len = source.as_ref().map_or(real_len, |s| raw(&s.bmp).len());
            writeln!(out,"{origin}\t{group}\t{name}\t{w}\t{h}\t{}\t{input_len}\t{bytes}\t{ns}\t{cycles}\t{verifier}\t{output_wire}\t{status}",codec.name()).unwrap();
        }
        out.flush().unwrap();
    }
}
