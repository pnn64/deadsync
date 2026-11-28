//! Paired revision benchmark for the original ITG3 AVI files.
use std::{ffi::c_void, hint::black_box, io::Write, path::Path};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThread() -> *mut c_void;
    fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
    #[cfg(not(allocations))]
    fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
}

#[cfg(allocations)]
mod meter {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        sync::atomic::{AtomicU64, Ordering::Relaxed},
    };
    static CALLS: AtomicU64 = AtomicU64::new(0);
    static BYTES: AtomicU64 = AtomicU64::new(0);
    struct Meter;
    #[global_allocator]
    static ALLOCATOR: Meter = Meter;
    fn record(p: *mut u8, n: usize) {
        if !p.is_null() && n != 0 {
            CALLS.fetch_add(1, Relaxed);
            BYTES.fetch_add(n as u64, Relaxed);
        }
    }
    unsafe impl GlobalAlloc for Meter {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            let p = unsafe { System.alloc(l) };
            record(p, l.size());
            p
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            let p = unsafe { System.alloc_zeroed(l) };
            record(p, l.size());
            p
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            let p = unsafe { System.realloc(p, l, n) };
            record(p, n);
            p
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            unsafe { System.dealloc(p, l) }
        }
    }
    pub fn snapshot() -> (u64, u64) {
        (CALLS.load(Relaxed), BYTES.load(Relaxed))
    }
}

#[cfg(not(allocations))]
fn cycles() -> u64 {
    let mut n = 0;
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut n) },
        0
    );
    n
}

fn measure<T>(op: impl FnOnce() -> T) -> (T, u64, u64, u64) {
    #[cfg(allocations)]
    {
        let (a, b) = meter::snapshot();
        let v = black_box(op());
        let (c, d) = meter::snapshot();
        (v, 0, c - a, d - b)
    }
    #[cfg(not(allocations))]
    {
        let start = cycles();
        let v = black_box(op());
        (v, cycles() - start, 0, 0)
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Output {
    frames: u64,
    error: Option<String>,
}

macro_rules! engine {
    ($name:ident, $codec:ident) => {
        mod $name {
            use super::*;
            use $codec as codec;

            pub fn decode(data: &[u8]) -> Output {
                let mut out = Output::default();
                let result = codec::decode_avi(black_box(data), |f| {
                    out.frames += 1;
                    black_box(f.planes()[0].rows().next().unwrap()[0]);
                });
                out.error = result.err().map(|e| format!("{e:?}"));
                out
            }

            pub fn exact(data: &[u8], mut consume: impl FnMut(&[u8])) -> Output {
                let mut out = Output::default();
                let result = codec::decode_avi(data, |f| {
                    out.frames += 1;
                    for n in [
                        f.width() as u64,
                        f.height() as u64,
                        f.picture_kind() as u64,
                        u64::from(f.presentation_time().is_some()),
                    ] {
                        consume(&n.to_le_bytes());
                    }
                    let (t, r) = f.presentation_time().unwrap_or_default();
                    consume(&t.to_le_bytes());
                    consume(&r.to_le_bytes());
                    for plane in f.planes() {
                        for row in plane.rows() {
                            consume(row);
                        }
                    }
                });
                out.error = result.err().map(|e| format!("{e:?}"));
                out
            }

            pub fn scan(packets: &[&[u8]]) -> u64 {
                let mut sum = 0u64;
                for &packet in packets {
                    for unit in codec::units(black_box(packet)) {
                        sum = sum
                            .wrapping_add(unit.payload.len() as u64 * 256 + u64::from(unit.code));
                    }
                }
                black_box(sum)
            }
        }
    };
}
engine!(old, baseline);
engine!(new, candidate);

// ITG3 files use video track 00. Extract only its video packets before timing.
fn packets(data: &[u8]) -> Vec<&[u8]> {
    fn walk<'a>(mut data: &'a [u8], in_movie: bool, out: &mut Vec<&'a [u8]>) {
        while let Some((header, rest)) = data.split_first_chunk::<8>() {
            let len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
            let Some((body, tail)) = rest.split_at_checked(len) else {
                break;
            };
            let id = &header[..4];
            if id == b"RIFF" || id == b"LIST" {
                if let Some((kind, contents)) = body.split_first_chunk::<4>() {
                    if kind == b"AVI "
                        || kind == b"AVIX"
                        || kind == b"movi"
                        || (in_movie && kind == b"rec ")
                    {
                        walk(contents, in_movie || kind == b"movi", out);
                    }
                }
            } else if in_movie && matches!(id, b"00dc" | b"00db") {
                out.push(body);
            }
            let Some(tail) = tail.get(len & 1..) else {
                break;
            };
            data = tail;
        }
    }
    let mut out = Vec::new();
    walk(data, false, &mut out);
    out
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let mode = &args[1];
    let rounds: usize = args[2].parse().unwrap();
    let cpu: u32 = args[3].parse().unwrap();
    assert_ne!(
        unsafe { SetThreadAffinityMask(GetCurrentThread(), 1usize << cpu) },
        0
    );
    #[cfg(itg_kernels)]
    if mode == "intra" || mode == "dc" {
        let data = std::fs::read(&args[4]).unwrap();
        let a = baseline::PerfIntra::parse(&data);
        let b = candidate::PerfIntra::parse(&data);
        assert_eq!(a.len(), b.len());
        if mode == "intra" {
            assert_eq!(
                baseline::perf_intra_exact(&a),
                candidate::perf_intra_exact(&b)
            );
        } else {
            assert_eq!(baseline::perf_dc_exact(&a), candidate::perf_dc_exact(&b));
        }
        let reps = if mode == "intra" { 64 } else { 512 };
        println!("round,revision,operation,units,cycles,heap_calls,requested_bytes");
        for round in 0..rounds {
            for candidate in [round % 2 == 0, round % 2 != 0] {
                let label = if candidate { "after" } else { "before" };
                let (_, cycles, calls, bytes) = measure(|| {
                    if mode == "dc" && candidate {
                        candidate::perf_dc_batch(&b, reps)
                    } else if mode == "dc" {
                        baseline::perf_dc_batch(&a, reps)
                    } else if candidate {
                        candidate::perf_intra_batch(&b, reps)
                    } else {
                        baseline::perf_intra_batch(&a, reps)
                    }
                });
                println!(
                    "{round},{label},{mode},{},{cycles},{calls},{bytes}",
                    a.len() * reps
                );
            }
        }
        return;
    }
    let list = std::fs::read_to_string(&args[4]).unwrap();
    #[cfg(itg_capture)]
    if mode == "capture" {
        for file in list
            .trim_start_matches('\u{feff}')
            .lines()
            .filter(|s| !s.is_empty())
        {
            let result = old::decode(&std::fs::read(file).unwrap());
            eprintln!("{file}: {result:?}");
        }
        baseline::write_trace(&args[5]);
        return;
    }
    let mut verify = (mode == "full").then(|| {
        let mut f = std::io::BufWriter::new(std::fs::File::create(&args[5]).unwrap());
        writeln!(f, "clip,input_bytes,frames,error,verified_bytes").unwrap();
        f
    });
    println!("clip,round,revision,operation,units,input_bytes,cycles,heap_calls,requested_bytes");
    for (index, file) in list
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|s| !s.is_empty())
        .enumerate()
    {
        let name = Path::new(file).file_name().unwrap().to_string_lossy();
        let data = std::fs::read(file).unwrap();
        if mode == "allocall" {
            assert!(cfg!(allocations));
            let mut first = None;
            for candidate in [index % 2 == 0, index % 2 != 0] {
                let (label, decode) = if candidate {
                    ("after", new::decode as fn(&[u8]) -> Output)
                } else {
                    ("before", old::decode as fn(&[u8]) -> Output)
                };
                let (out, cycles, calls, bytes) = measure(|| decode(&data));
                println!(
                    "{name},0,{label},pipeline,{},{},{cycles},{calls},{bytes}",
                    out.frames,
                    data.len()
                );
                if let Some(expected) = &first {
                    assert_eq!(&out, expected, "{file}");
                } else {
                    first = Some(out);
                }
            }
            continue;
        }
        let expected = if let Some(log) = &mut verify {
            let mut reference = Vec::new();
            let a = old::exact(&data, |p| reference.extend_from_slice(p));
            let mut tail = reference.as_slice();
            let b = new::exact(&data, |p| {
                let (expected, rest) = tail.split_at(p.len());
                assert!(expected == p, "output mismatch: {file}");
                tail = rest;
            });
            assert!(tail.is_empty(), "missing output: {file}");
            assert_eq!(a, b, "{file}");
            writeln!(
                log,
                "{name},{},{},{:?},{}",
                data.len(),
                a.frames,
                a.error,
                reference.len()
            )
            .unwrap();
            log.flush().unwrap();
            a
        } else {
            let a = old::decode(&data);
            assert_eq!(a, new::decode(&data), "{file}");
            a
        };
        if expected.error.is_some() || expected.frames == 0 {
            eprintln!("skipped {name}: {expected:?}");
            continue;
        }
        let packets = if mode == "both" || mode == "scan" {
            packets(&data)
        } else {
            Vec::new()
        };
        let scan_bytes: usize = packets.iter().map(|p| p.len()).sum();
        let sum = old::scan(&packets);
        assert_eq!(sum, new::scan(&packets));
        for round in 0..rounds {
            for candidate in [(round + index) % 2 == 0, (round + index) % 2 != 0] {
                let label = if candidate { "after" } else { "before" };
                if mode == "both" || mode == "scan" {
                    assert!(!packets.is_empty());
                    let scan = if candidate { new::scan } else { old::scan };
                    let reps = 16;
                    let (v, cycles, calls, bytes) = measure(|| {
                        let mut v = 0u64;
                        for _ in 0..reps {
                            v = v.wrapping_add(scan(&packets));
                        }
                        v
                    });
                    assert_eq!(v, sum.wrapping_mul(reps));
                    println!(
                        "{name},{round},{label},scan,{},{},{cycles},{calls},{bytes}",
                        scan_bytes as u64 * reps,
                        scan_bytes as u64 * reps
                    );
                }
                if mode != "scan" {
                    let f = if candidate { new::decode } else { old::decode };
                    let (v, cycles, calls, bytes) = measure(|| f(&data));
                    assert_eq!(v, expected);
                    println!(
                        "{name},{round},{label},pipeline,{},{},{cycles},{calls},{bytes}",
                        v.frames,
                        data.len()
                    );
                }
            }
        }
    }
}
