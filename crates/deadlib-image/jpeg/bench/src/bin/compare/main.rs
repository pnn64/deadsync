mod decode;
mod encode;
mod foreign;

use std::{
    ffi::c_void,
    hint::black_box,
    io::{self, BufRead, BufWriter, Write},
    path::Path,
    time::{Duration, Instant},
};

unsafe extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn QueryProcessCycleTime(process: *mut c_void, count: *mut u64) -> i32;
    fn SetProcessAffinityMask(process: *mut c_void, mask: usize) -> i32;
}

fn cycles() -> u64 {
    let mut count = 0;
    // The OS writes the cycle count to the supplied u64.
    assert_ne!(
        unsafe { QueryProcessCycleTime(GetCurrentProcess(), &mut count) },
        0
    );
    count
}

fn measure<T>(sample: Duration, mut operation: impl FnMut() -> T) -> (u64, u64) {
    if sample.is_zero() {
        return (0, 0);
    }
    let mut elapsed = [0; 5];
    let mut clocks = [0; 5];
    for (elapsed, clocks) in elapsed.iter_mut().zip(&mut clocks) {
        let start = Instant::now();
        let c = cycles();
        let mut n = 0;
        while n == 0 || start.elapsed() < sample {
            drop(black_box(operation()));
            n += 1;
        }
        *clocks = (cycles() - c) / n;
        *elapsed = (start.elapsed().as_nanos() / u128::from(n)) as u64;
    }
    elapsed.sort_unstable();
    clocks.sort_unstable();
    (elapsed[2], clocks[2])
}

struct Bridges {
    turbo: foreign::Bridge,
    moz: foreign::Bridge,
    jpegli: foreign::Bridge,
    extra: foreign::Bridge,
}

impl Bridges {
    fn load(path: &Path) -> Self {
        let load = |codec: &str| foreign::Bridge::load(&path.join(format!("bench_{codec}.dll")));
        Self {
            turbo: load("turbo"),
            moz: load("mozjpeg"),
            jpegli: load("jpegli"),
            extra: load("extra"),
        }
    }
}

enum Bytes<'a> {
    U8(Vec<u8>),
    U16(Vec<u16>),
    Foreign(foreign::Buffer<'a>),
}

impl Bytes<'_> {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::U8(bytes) => bytes,
            Self::U16(samples) => {
                // Windows x86-64 uses little-endian u16 raster storage.
                unsafe { std::slice::from_raw_parts(samples.as_ptr().cast(), samples.len() * 2) }
            }
            Self::Foreign(buffer) => buffer.bytes(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Shape {
    w: u32,
    h: u32,
    channels: u32,
    precision: u32,
}

struct Pixels<'a> {
    shape: Shape,
    data: Bytes<'a>,
}

impl<'a> From<foreign::Buffer<'a>> for Pixels<'a> {
    fn from(buffer: foreign::Buffer<'a>) -> Self {
        let o = &buffer.output;
        Self {
            shape: Shape {
                w: o.width,
                h: o.height,
                channels: o.channels,
                precision: o.precision,
            },
            data: Bytes::Foreign(buffer),
        }
    }
}

fn difference(shape: Shape, a: &[u8], b: &[u8]) -> Result<(f64, u16), String> {
    if a.len() != b.len() {
        return Err("different raster length".into());
    }
    let samples = shape.w as usize * shape.h as usize * shape.channels as usize;
    if a.len() != samples * if shape.precision > 8 { 2 } else { 1 } {
        return Err("unexpected raster length".into());
    }
    let (mut squared, mut max) = (0u64, 0u16);
    let mut compare = |a: u16, b: u16| {
        let delta = a.abs_diff(b);
        max = max.max(delta);
        squared += u64::from(delta) * u64::from(delta);
    };
    if shape.precision <= 8 {
        for (&a, &b) in a.iter().zip(b) {
            compare(a.into(), b.into());
        }
    } else {
        for (&a, &b) in a.as_chunks::<2>().0.iter().zip(b.as_chunks::<2>().0) {
            compare(u16::from_le_bytes(a), u16::from_le_bytes(b));
        }
    }
    let range = f64::from((1u32 << shape.precision) - 1);
    Ok((
        10.0 * (range * range * samples as f64 / squared as f64).log10(),
        max,
    ))
}

#[derive(Clone, Copy)]
struct Header {
    process: u8,
    shape: Shape,
    sampling: u8,
}

fn header(mut data: &[u8]) -> Option<Header> {
    data = data.strip_prefix(&[255, 216])?;
    while data.first() == Some(&255) {
        let tag = *data.iter().find(|&&v| v != 255)?;
        let start = data.iter().position(|&v| v != 255)? + 1;
        data = data.get(start..)?;
        if matches!(tag, 0xd9 | 0xda) {
            break;
        }
        let len = u16::from_be_bytes(data.get(..2)?.try_into().ok()?) as usize;
        if len < 2 {
            return None;
        }
        if matches!(tag, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            let prefix = data.get(2..10)?;
            return Some(Header {
                process: tag,
                shape: Shape {
                    precision: prefix[0].into(),
                    h: u16::from_be_bytes([prefix[1], prefix[2]]).into(),
                    w: u16::from_be_bytes([prefix[3], prefix[4]]).into(),
                    channels: prefix[5].into(),
                },
                sampling: prefix[7],
            });
        }
        data = data.get(len..)?;
    }
    None
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args
        .next()
        .expect("compare decode|encode INPUTS.tsv BRIDGE_DIR [--verify-only] [--turbo-only]");
    let inputs = args.next().expect("input manifest");
    let bridges = Bridges::load(Path::new(&args.next().expect("bridge directory")));
    let (mut verify, mut turbo_only) = (false, false);
    for arg in args {
        match arg.as_str() {
            "--verify-only" => verify = true,
            "--turbo-only" if mode == "decode" => turbo_only = true,
            _ => panic!("unknown argument for {mode}: {arg}"),
        }
    }
    let ms: u64 = std::env::var("JPEG_BENCH_MS").map_or(8, |s| s.parse().unwrap());
    let sample = Duration::from_millis(if verify { 0 } else { ms });
    if !verify {
        let cpu: usize = std::env::var("JPEG_BENCH_CPU").map_or(2, |s| s.parse().unwrap());
        // Pin the process so the optional jpeg-decoder worker shares the same CPU.
        assert_ne!(
            unsafe { SetProcessAffinityMask(GetCurrentProcess(), 1 << cpu) },
            0
        );
    }
    let input = io::BufReader::new(std::fs::File::open(inputs).unwrap());
    let mut out = BufWriter::new(io::stdout().lock());
    writeln!(out, "group\tfile\tconfig\twidth\theight\tchannels\tprecision\tsof\tsampling\tcodec\tinput_bytes\toutput_bytes\tns\tcycles\tpsnr_db\tmax_diff\tverifier\tstatus").unwrap();
    for (index, line) in input.lines().enumerate() {
        let line = line.unwrap();
        let fields: Vec<_> = line.split('\t').collect();
        if mode == "decode" {
            decode::bench(&fields, index, sample, turbo_only, &bridges, &mut out);
        } else {
            assert_eq!(mode, "encode");
            encode::bench(&fields, index, sample, &bridges, &mut out);
        }
        out.flush().unwrap();
    }
}

struct Row<'a> {
    group: &'a str,
    file: &'a str,
    config: &'a str,
    shape: Shape,
    sof: u8,
    sampling: u8,
    codec: &'a str,
    input_bytes: usize,
    verifier: &'a str,
}

impl Row<'_> {
    fn write(&self, out: &mut impl Write, result: Result<(usize, u64, u64, f64, u16), String>) {
        let (size, ns, cycles, psnr, delta, status) = match result {
            Ok((size, ns, cycles, psnr, delta)) => {
                (size, ns, cycles, psnr, delta, String::from("ok"))
            }
            Err(error) => (0, 0, 0, f64::NAN, 0, error.replace(['\r', '\n', '\t'], " ")),
        };
        writeln!(out, "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:02x}\t{}x{}\t{}\t{}\t{size}\t{ns}\t{cycles}\t{psnr:.5}\t{delta}\t{}\t{status}",
            self.group, self.file, self.config, self.shape.w, self.shape.h, self.shape.channels, self.shape.precision,
            self.sof, self.sampling >> 4, self.sampling & 15, self.codec, self.input_bytes, self.verifier).unwrap();
    }
}
