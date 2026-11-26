mod animation;
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
    fn GetCurrentThread() -> *mut c_void;
    fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
    fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
}

fn cycles() -> u64 {
    let mut cycles = 0;
    // QueryThreadCycleTime fills one u64 for the current thread.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) },
        0
    );
    cycles
}

fn measure<T>(sample: Duration, mut operation: impl FnMut() -> T) -> (u64, u64) {
    let mut ns = [0; 5];
    let mut cpu = [0; 5];
    for (ns, cpu) in ns.iter_mut().zip(&mut cpu) {
        let start = Instant::now();
        let cpu_start = cycles();
        let mut iterations = 0;
        while iterations == 0 || start.elapsed() < sample {
            drop(black_box(operation()));
            iterations += 1;
        }
        *cpu = (cycles() - cpu_start) / iterations;
        *ns = (start.elapsed().as_nanos() / u128::from(iterations)) as u64;
    }
    ns.sort_unstable();
    cpu.sort_unstable();
    (ns[2], cpu[2])
}

enum Bytes {
    U8(Vec<u8>),
    U16(Vec<u16>),
    Foreign(foreign::Buffer),
}

impl Bytes {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::U8(bytes) => bytes,
            // On Windows x86-64 the comparison layout is little-endian.
            Self::U16(samples) => unsafe {
                std::slice::from_raw_parts(samples.as_ptr().cast(), samples.len() * 2)
            },
            Self::Foreign(bytes) => bytes.bytes(),
        }
    }
}

struct Case<'a> {
    group: &'a str,
    name: &'a str,
    bytes: &'a [u8],
    width: u32,
    height: u32,
    color: u8,
    depth: u8,
    adam7: u8,
}

impl<'a> Case<'a> {
    fn new(group: &'a str, name: &'a str, bytes: &'a [u8]) -> Self {
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        Self {
            group,
            name,
            bytes,
            width: u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
            height: u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
            color: bytes[25],
            depth: bytes[24],
            adam7: bytes[28],
        }
    }

    fn row(
        &self,
        out: &mut impl Write,
        workload: &str,
        codec: &str,
        result: Result<(usize, u64, u64), String>,
    ) {
        let (size, ns, cycles, status) = match result {
            Ok((size, ns, cycles)) => (size, ns, cycles, String::from("ok")),
            Err(error) => (0, 0, 0, error.replace(['\t', '\r', '\n'], " ")),
        };
        writeln!(
            out,
            "{workload}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{codec}\t{}\t{size}\t{ns}\t{cycles}\t{status}",
            self.group,
            self.name,
            self.width,
            self.height,
            self.color,
            self.depth,
            self.adam7,
            self.bytes.len()
        )
        .unwrap();
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args
        .next()
        .expect("compare decode|encode|fpng|animation INPUTS.tsv");
    let manifest = args.next().expect("input manifest");
    let encode_modes = (mode == "encode").then(encode::modes);
    let verify = args.next().is_some_and(|arg| {
        assert_eq!(arg, "--verify-only");
        true
    });
    assert!(args.next().is_none());
    let millis = std::env::var("PNG_BENCH_MS").map_or(10, |s| s.parse().unwrap());
    let duration = Duration::from_millis(if verify { 0 } else { millis });
    if !verify {
        let cpu: usize = std::env::var("PNG_BENCH_CPU").map_or(0, |s| s.parse().unwrap());
        // CPU affinity applies only to this benchmark thread.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << cpu) },
            0
        );
    }
    let input = io::BufReader::new(std::fs::File::open(manifest).unwrap());
    let mut out = BufWriter::new(io::stdout().lock());
    writeln!(out, "workload\tgroup\tfile\twidth\theight\tcolor\tdepth\tadam7\tcodec\tinput_bytes\toutput_bytes\tns\tcycles\tstatus").unwrap();
    for (index, line) in input.lines().enumerate() {
        let line = line.unwrap();
        let (group, file) = line
            .split_once('\t')
            .expect("group then path separated by tab");
        let bytes =
            std::fs::read(Path::new(file)).unwrap_or_else(|error| panic!("{file}: {error}"));
        let case = Case::new(group, file, &bytes);
        match mode.as_str() {
            "decode" => decode::bench(&case, index, duration, &mut out),
            "encode" => encode::bench(
                &case,
                index,
                duration,
                encode_modes.as_deref().unwrap(),
                &mut out,
            ),
            "fpng" => encode::bench_fpng(&case, index, duration, &mut out),
            "animation" => animation::bench(&case, index, duration, &mut out),
            _ => panic!("unknown workload {mode}"),
        }
        out.flush().unwrap();
    }
}
