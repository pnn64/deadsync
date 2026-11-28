//! Paired benchmark of two revisions on external MPEG video files.
use std::{borrow::Cow, ffi::c_void, hint::black_box, io::Write, path::Path};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThread() -> *mut c_void;
    fn SetThreadAffinityMask(thread: *mut c_void, mask: usize) -> usize;
    #[cfg(not(allocations))]
    fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
}

#[link(name = "avutil")]
unsafe extern "C" {
    fn av_sha_alloc() -> *mut c_void;
    fn av_sha_init(sha: *mut c_void, bits: i32) -> i32;
    fn av_sha_update(sha: *mut c_void, data: *const u8, len: usize);
    fn av_sha_final(sha: *mut c_void, out: *mut u8);
    fn av_free(ptr: *mut c_void);
}

struct Sha(*mut c_void);
impl Sha {
    fn new() -> Self {
        let state = unsafe { av_sha_alloc() };
        assert!(!state.is_null());
        assert_eq!(unsafe { av_sha_init(state, 256) }, 0);
        Self(state)
    }
    fn update(&self, bytes: &[u8]) {
        unsafe { av_sha_update(self.0, bytes.as_ptr(), bytes.len()) };
    }
    fn finish(&self) -> [u8; 32] {
        let mut digest = [0; 32];
        unsafe { av_sha_final(self.0, digest.as_mut_ptr()) };
        digest
    }
}
impl Drop for Sha {
    fn drop(&mut self) {
        unsafe { av_free(self.0) };
    }
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
    fn record(p: *mut u8, bytes: usize) {
        if !p.is_null() && bytes != 0 {
            CALLS.fetch_add(1, Relaxed);
            BYTES.fetch_add(bytes as u64, Relaxed);
        }
    }
    unsafe impl GlobalAlloc for Meter {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let p = unsafe { System.alloc(layout) };
            record(p, layout.size());
            p
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let p = unsafe { System.alloc_zeroed(layout) };
            record(p, layout.size());
            p
        }
        unsafe fn realloc(&self, p: *mut u8, layout: Layout, bytes: usize) -> *mut u8 {
            let p = unsafe { System.realloc(p, layout, bytes) };
            record(p, bytes);
            p
        }
        unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
            unsafe { System.dealloc(p, layout) };
        }
    }
    pub fn snapshot() -> (u64, u64) {
        (CALLS.load(Relaxed), BYTES.load(Relaxed))
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Output {
    frames: u64,
    pixels: u64,
    error: Option<String>,
    digest: Option<[u8; 32]>,
}

fn is_program(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0, 0, 1, 0xba])
}

macro_rules! engine {
    ($name:ident, $codec:ident) => {
        mod $name {
            use super::*;
            use $codec as codec;

            pub fn elementary(bytes: &[u8]) -> Result<Cow<'_, [u8]>, codec::Error> {
                if is_program(bytes) {
                    codec::video_elementary_stream(bytes).map(Cow::Owned)
                } else {
                    Ok(Cow::Borrowed(bytes))
                }
            }

            pub fn scan(bytes: &[u8]) -> u64 {
                let mut sum = 0u64;
                for unit in codec::units(black_box(bytes)) {
                    sum = sum.wrapping_add(unit.payload.len() as u64 * 256 + u64::from(unit.code));
                }
                black_box(sum)
            }

            pub fn decode(bytes: &[u8], verify: bool) -> Output {
                let mut out = Output::default();
                let sha = verify.then(Sha::new);
                let mut emit = |frame: std::sync::Arc<codec::Frame>| {
                    out.frames += 1;
                    out.pixels += (frame.width() * frame.height()) as u64;
                    if let Some(sha) = &sha {
                        for v in [
                            frame.width() as u32,
                            frame.height() as u32,
                            frame.frame_rate().0,
                            frame.frame_rate().1,
                            u32::from(frame.temporal_reference()),
                            frame.picture_kind() as u32,
                        ] {
                            sha.update(&v.to_le_bytes());
                        }
                        for plane in frame.planes() {
                            for row in plane.rows() {
                                sha.update(row);
                            }
                        }
                    } else {
                        black_box(frame.planes()[0].rows().next().unwrap()[0]);
                    }
                };
                let result = (|| {
                    let elementary = elementary(bytes)?;
                    let mut decoder = codec::Decoder::with_mpeg1_recovery();
                    for unit in codec::units(&elementary) {
                        decoder.push(unit, &mut emit)?;
                    }
                    decoder.finish(&mut emit)
                })();
                out.error = result.err().map(|e| format!("{e:?}"));
                out.digest = sha.map(|s| s.finish());
                out
            }

            // Separate from the timed path: compare every visible byte and
            // frame field directly, including any frames emitted before an error.
            pub fn exact(bytes: &[u8], mut consume: impl FnMut(&[u8])) -> Output {
                let mut out = Output::default();
                let mut emit = |frame: std::sync::Arc<codec::Frame>| {
                    out.frames += 1;
                    out.pixels += (frame.width() * frame.height()) as u64;
                    for v in [
                        frame.width() as u32,
                        frame.height() as u32,
                        frame.frame_rate().0,
                        frame.frame_rate().1,
                        u32::from(frame.temporal_reference()),
                        frame.picture_kind() as u32,
                    ] {
                        consume(&v.to_le_bytes());
                    }
                    for plane in frame.planes() {
                        for row in plane.rows() {
                            consume(row);
                        }
                    }
                };
                let result = (|| {
                    let elementary = elementary(bytes)?;
                    let mut decoder = codec::Decoder::with_mpeg1_recovery();
                    for unit in codec::units(&elementary) {
                        decoder.push(unit, &mut emit)?;
                    }
                    decoder.finish(&mut emit)
                })();
                out.error = result.err().map(|e| format!("{e:?}"));
                out
            }
        }
    };
}
engine!(old, baseline);
engine!(new, candidate);

#[cfg(not(allocations))]
fn cycles() -> u64 {
    let mut count = 0;
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut count) },
        0
    );
    count
}

fn measure<T>(op: impl FnOnce() -> T) -> (T, u64, u64, u64) {
    #[cfg(allocations)]
    {
        let (calls, bytes) = meter::snapshot();
        let out = black_box(op());
        let (end_calls, end_bytes) = meter::snapshot();
        (out, 0, end_calls - calls, end_bytes - bytes)
    }
    #[cfg(not(allocations))]
    {
        let start = cycles();
        let out = black_box(op());
        (out, cycles() - start, 0, 0)
    }
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
    #[cfg(any(itg_kernels, itg_flags, itg_motion, itg_prediction))]
    let kernel_repeats = args.get(6).map(|value| {
        let repeats: usize = value.parse().expect("invalid kernel repetition count");
        assert!(repeats > 0, "kernel repetition count must be positive");
        repeats
    });
    #[cfg(itg_kernels)]
    if mode == "kernels" {
        kernels(rounds, &args[4], &args[5], kernel_repeats.unwrap_or(64));
        return;
    }
    #[cfg(itg_flags)]
    if mode == "kernels" {
        flag_kernels(rounds, &args[4], kernel_repeats.unwrap_or(256));
        return;
    }
    #[cfg(itg_motion)]
    if mode == "kernels" {
        motion_kernels(rounds, &args[4], kernel_repeats.unwrap_or(256));
        return;
    }
    #[cfg(itg_prediction)]
    if mode == "kernels" {
        prediction_kernels(rounds, &args[4], kernel_repeats.unwrap_or(256));
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
            let data = std::fs::read(file).unwrap();
            let result = old::decode(&data, false);
            assert!(result.error.is_none(), "{file}: {result:?}");
            eprintln!("captured {file}: {} frames", result.frames);
        }
        baseline::write_trace(&args[5]);
        return;
    }
    let mut verification = (mode == "full").then(|| {
        let mut file = std::io::BufWriter::new(std::fs::File::create(&args[5]).unwrap());
        writeln!(file, "method,clip,input_bytes,frames,error,verified_bytes").unwrap();
        file
    });
    if mode == "exact" {
        println!("method,clip,input_bytes,frames,error,verified_bytes");
    } else if mode == "verify" {
        println!("method,clip,input_bytes,frames,error,sha256");
    } else {
        println!(
            "clip,round,revision,operation,frames,input_bytes,cycles,heap_calls,requested_bytes"
        );
    }
    for (index, file) in list.trim_start_matches('\u{feff}').lines().enumerate() {
        if file.is_empty() {
            continue;
        }
        let name = Path::new(file).file_name().unwrap().to_string_lossy();
        let data = std::fs::read(file).unwrap();
        let verified = if mode == "exact" || mode == "full" {
            let mut reference = Vec::new();
            let a = old::exact(&data, |bytes| reference.extend_from_slice(bytes));
            let mut remaining = reference.as_slice();
            let b = new::exact(&data, |bytes| {
                let (expected, tail) = remaining.split_at(bytes.len());
                assert!(
                    expected == bytes,
                    "pixel or frame metadata mismatch: {file}"
                );
                remaining = tail;
            });
            assert!(remaining.is_empty(), "missing output: {file}");
            assert_eq!(a, b, "{file}");
            let record = format!(
                "exact,{name},{},{},{:?},{}",
                data.len(),
                a.frames,
                a.error,
                reference.len()
            );
            if let Some(log) = &mut verification {
                writeln!(log, "{record}").unwrap();
                log.flush().unwrap();
                Some(a)
            } else {
                println!("{record}");
                continue;
            }
        } else {
            None
        };
        if mode == "verify" {
            let a = old::decode(&data, true);
            let b = new::decode(&data, true);
            assert_eq!(a, b, "{file}");
            let digest: String = a
                .digest
                .unwrap()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            println!(
                "verify,{name},{},{},{:?},{digest}",
                data.len(),
                a.frames,
                a.error
            );
            continue;
        }
        let a = verified.unwrap_or_else(|| {
            let a = old::decode(&data, false);
            let b = new::decode(&data, false);
            assert_eq!(a, b, "{file}");
            a
        });
        if a.error.is_some() || a.frames == 0 {
            eprintln!("skipped {name}: {a:?}");
            continue;
        }
        let es = old::elementary(&data).unwrap();
        let expected = old::scan(&es);
        assert_eq!(expected, new::scan(&es));
        for round in 0..rounds {
            for candidate in [(round + index) % 2 == 0, (round + index) % 2 != 0] {
                let revision = if candidate { "after" } else { "before" };
                if mode == "scan" || mode == "both" {
                    let scan = if candidate { new::scan } else { old::scan };
                    let reps = 8;
                    let (sum, cycles, calls, bytes) = measure(|| {
                        let mut sum = 0u64;
                        for _ in 0..reps {
                            sum = sum.wrapping_add(scan(&es));
                        }
                        sum
                    });
                    assert_eq!(sum, expected.wrapping_mul(reps));
                    println!(
                        "{name},{round},{revision},scan,0,{},{cycles},{calls},{bytes}",
                        es.len() as u64 * reps
                    );
                }
                if mode == "pipeline" || mode == "both" || mode == "full" {
                    let decode = if candidate { new::decode } else { old::decode };
                    let (out, cycles, calls, bytes) = measure(|| decode(&data, false));
                    assert_eq!(out, a);
                    println!(
                        "{name},{round},{revision},pipeline,{},{},{cycles},{calls},{bytes}",
                        out.frames,
                        data.len()
                    );
                }
            }
        }
    }
}

#[cfg(itg_prediction)]
fn prediction_kernels(rounds: usize, directory: &str, repeats: usize) {
    let raw = std::fs::read(Path::new(directory).join("predictions.bin")).unwrap();
    let mut a = baseline::PerfPredictions::new(&raw);
    let mut b = candidate::PerfPredictions::new(&raw);
    a.run(1);
    b.run(1);
    assert!(a.samples().eq(b.samples()), "prediction output mismatch");
    println!("round,revision,operation,operations,cycles,heap_calls,requested_bytes");
    for round in 0..rounds {
        for candidate in [round % 2 == 0, round % 2 != 0] {
            let revision = if candidate { "after" } else { "before" };
            let (_, cycles, calls, bytes) = measure(|| {
                if candidate {
                    b.run(repeats);
                } else {
                    a.run(repeats);
                }
            });
            println!(
                "{round},{revision},prediction,{},{cycles},{calls},{bytes}",
                a.len() * repeats
            );
        }
    }
    assert!(
        a.samples().eq(b.samples()),
        "prediction output mismatch after replay"
    );
}

#[cfg(itg_motion)]
fn motion_kernels(rounds: usize, directory: &str, repeats: usize) {
    let raw = std::fs::read(Path::new(directory).join("read-motions.bin")).unwrap();
    assert_eq!(raw.len() % 20, 0);
    let records: Vec<_> = raw
        .chunks_exact(20)
        .map(|r| {
            assert!(r[8] <= 8 && r[9] < 8);
            (
                r[..8].try_into().unwrap(),
                r[8],
                r[9],
                [r[10], r[11]],
                [
                    i32::from_le_bytes(r[12..16].try_into().unwrap()),
                    i32::from_le_bytes(r[16..20].try_into().unwrap()),
                ],
            )
        })
        .collect();
    assert!(!records.is_empty());
    let expected = baseline::perf_motion_values(&records);
    assert_eq!(
        expected,
        candidate::perf_motion_values(&records),
        "motion vector, status or bit position mismatch"
    );
    let expected_once = expected
        .iter()
        .fold(0u64, |sum, (motion, position, status)| {
            let value = (u64::from(motion[0] as u32) << 32) | u64::from(motion[1] as u32);
            sum.wrapping_add(value ^ ((*position as u64) << 2) ^ u64::from(*status))
        });
    let expected_sum = expected_once.wrapping_mul(repeats as u64);
    assert_eq!(baseline::perf_motion_batch(&records, 1), expected_once);
    assert_eq!(candidate::perf_motion_batch(&records, 1), expected_once);
    println!("round,revision,operation,operations,cycles,heap_calls,requested_bytes");
    for round in 0..rounds {
        for candidate in [round % 2 == 0, round % 2 != 0] {
            let revision = if candidate { "after" } else { "before" };
            let f = if candidate {
                candidate::perf_motion_batch
            } else {
                baseline::perf_motion_batch
            };
            let (sum, cycles, calls, bytes) = measure(|| f(&records, repeats));
            assert_eq!(sum, expected_sum);
            println!(
                "{round},{revision},read-motion,{},{cycles},{calls},{bytes}",
                records.len() * repeats
            );
        }
    }
}

#[cfg(itg_flags)]
fn flag_kernels(rounds: usize, directory: &str, repeats: usize) {
    let raw = std::fs::read(Path::new(directory).join("flags.bin")).unwrap();
    assert_eq!(raw.len() % 6, 0);
    let records: Vec<_> = raw
        .chunks_exact(6)
        .map(|r| {
            assert!(r[4] <= 4 && r[5] < 8);
            (r[..4].try_into().unwrap(), r[4], r[5])
        })
        .collect();
    assert!(!records.is_empty());
    let expected = baseline::perf_flag_values(&records);
    assert_eq!(
        expected,
        candidate::perf_flag_values(&records),
        "flag status or bit position mismatch"
    );
    let expected_once = expected
        .iter()
        .fold(0u64, |sum, value| sum.wrapping_add(*value));
    let expected_sum = expected_once.wrapping_mul(repeats as u64);
    assert_eq!(baseline::perf_flag_batch(&records, 1), expected_once);
    assert_eq!(candidate::perf_flag_batch(&records, 1), expected_once);
    println!("round,revision,operation,operations,cycles,heap_calls,requested_bytes");
    for round in 0..rounds {
        for candidate in [round % 2 == 0, round % 2 != 0] {
            let revision = if candidate { "after" } else { "before" };
            let f = if candidate {
                candidate::perf_flag_batch
            } else {
                baseline::perf_flag_batch
            };
            let (sum, cycles, calls, bytes) = measure(|| f(&records, repeats));
            assert_eq!(sum, expected_sum);
            println!(
                "{round},{revision},flag,{},{cycles},{calls},{bytes}",
                records.len() * repeats
            );
        }
    }
}

#[cfg(itg_kernels)]
fn kernels(rounds: usize, directory: &str, operation: &str, repeats: usize) {
    fn coeff(bytes: &[u8]) -> [i16; 64] {
        std::array::from_fn(|i| i16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]))
    }
    let dir = Path::new(directory);
    let raw = std::fs::read(dir.join("coefficients.bin")).unwrap();
    assert_eq!(raw.len() % 128, 0);
    let coefficients: Vec<_> = raw.chunks_exact(128).map(coeff).collect();
    let raw = std::fs::read(dir.join("blocks.bin")).unwrap();
    assert_eq!(raw.len() % 193, 0);
    let blocks: Vec<_> = raw
        .chunks_exact(193)
        .map(|r| (r[0] != 0, coeff(&r[1..129]), r[129..].try_into().unwrap()))
        .collect();
    assert!(!blocks.is_empty() && !coefficients.is_empty());
    let mut a = baseline::PerfPlane::new(&blocks);
    let mut b = candidate::PerfPlane::new(&blocks);
    a.run(&blocks, 1);
    b.run(&blocks, 1);
    assert_eq!(a.samples(), b.samples());
    println!("round,revision,operation,blocks,cycles,heap_calls,requested_bytes");
    for round in 0..rounds {
        for candidate in [round % 2 == 0, round % 2 != 0] {
            let revision = if candidate { "after" } else { "before" };
            if operation == "inverse" {
                let f = if candidate {
                    candidate::perf_inverse_batch
                } else {
                    baseline::perf_inverse_batch
                };
                let (_, cycles, calls, bytes) = measure(|| f(&coefficients, repeats));
                println!(
                    "{round},{revision},inverse,{},{cycles},{calls},{bytes}",
                    coefficients.len() * repeats
                );
            } else if operation == "block" {
                let (_, cycles, calls, bytes) = measure(|| {
                    if candidate {
                        b.run(&blocks, repeats);
                    } else {
                        a.run(&blocks, repeats);
                    }
                });
                println!(
                    "{round},{revision},block,{},{cycles},{calls},{bytes}",
                    blocks.len() * repeats
                );
            } else {
                panic!("unknown kernel");
            }
        }
    }
    assert_eq!(a.samples(), b.samples());
}
