//! Resident wavl native-output regression, caller storage and scoped heap counts.
use deadlib_audio_decode_wav::{
    Container, DataLength, RawChunk, SampleFormat, Spec, Wave, WavlWriter, WriteOptions,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    io::Cursor,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed},
    time::Instant,
};

struct Heap;
static TRACK: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static REQUESTED: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn allocated(bytes: usize) {
    CALLS.fetch_add(1, Relaxed);
    REQUESTED.fetch_add(bytes, Relaxed);
    let live = LIVE.fetch_add(bytes, Relaxed) + bytes;
    PEAK.fetch_max(live, Relaxed);
}
// SAFETY: every operation forwards the unchanged allocation/layout contract to
// System. Atomics only observe successful allocations; counters never allocate.
unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() && TRACK.load(Relaxed) {
            allocated(l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() && TRACK.load(Relaxed) {
            allocated(l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if TRACK.load(Relaxed) {
            LIVE.fetch_sub(l.size(), Relaxed);
        }
        unsafe {
            System.dealloc(p, l);
        }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let result = unsafe { System.realloc(p, l, n) };
        if !result.is_null() && TRACK.load(Relaxed) {
            LIVE.fetch_sub(l.size(), Relaxed);
            allocated(n);
        }
        result
    }
}
#[global_allocator]
static ALLOCATOR: Heap = Heap;

#[cfg(windows)]
fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut core::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut core::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut n = 0;
    // SAFETY: the pseudo-handle is valid in this process and the output points to
    // a writable u64 for the duration of the synchronous call.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut n) },
        0
    );
    n
}
#[cfg(not(windows))]
fn cycles() -> u64 {
    0
}

fn bench(name: &str, iters: usize, bytes: usize, mut work: impl FnMut()) {
    work();
    CALLS.store(0, Relaxed);
    REQUESTED.store(0, Relaxed);
    LIVE.store(0, Relaxed);
    PEAK.store(0, Relaxed);
    TRACK.store(true, Relaxed);
    work();
    TRACK.store(false, Relaxed);
    let allocations = CALLS.load(Relaxed);
    let requested = REQUESTED.load(Relaxed);
    let peak = PEAK.load(Relaxed);
    assert_eq!(
        LIVE.load(Relaxed),
        0,
        "operation must drop its own allocations"
    );
    for trial in 0..3 {
        let start = Instant::now();
        let c = cycles();
        for _ in 0..iters {
            work();
        }
        let cpu = cycles() - c;
        let elapsed = start.elapsed().as_nanos() as f64;
        let ns = elapsed / iters as f64;
        println!(
            "{name}\t{trial}\t{iters}\t{ns:.3}\t{:.3}\t{:.3}\t{allocations}\t{requested}\t{peak}",
            cpu as f64 / iters as f64,
            bytes as f64 / (1 << 20) as f64 * 1e9 / ns
        );
    }
}

#[derive(Clone, Copy)]
struct Case<'a> {
    options: WriteOptions,
    known: bool,
    metadata: &'a [RawChunk<'a>],
    samples: &'a [i16],
    batch: usize,
    silence: u64,
}
fn render(out: &mut [u8], case: Case<'_>) -> usize {
    let spec = Spec::new(SampleFormat::I16, 2, 48000);
    let bytes = 4
        + case.samples.len() as u64 * 2
        + case.samples.len().div_ceil(case.batch) as u64 * 8
        + if case.silence == 0 {
            0
        } else {
            case.silence.div_ceil(u64::from(u32::MAX)) * 12
        };
    let length = DataLength {
        bytes,
        frames: case.samples.len() as u64 / 2 + case.silence,
    };
    let sink = Cursor::new(out);
    let mut writer = if case.known {
        WavlWriter::with_lengths(sink, spec, case.options, case.metadata, length).unwrap()
    } else {
        WavlWriter::new(sink, spec, case.options, case.metadata).unwrap()
    };
    for batch in case.samples.chunks(case.batch) {
        writer.write_i16(batch).unwrap();
    }
    if case.silence != 0 {
        writer.write_silence(case.silence).unwrap();
    }
    let result = if case.known {
        writer.finish_stream().unwrap()
    } else {
        writer.finish().unwrap()
    };
    result.position() as usize
}
#[path = "support/affinity.rs"]
mod affinity;

fn main() {
    let affinity = affinity::pin_thread();
    println!("# thread_affinity_mask={affinity:#x}; frequency is uncontrolled");
    let samples = (0..1usize << 21)
        .map(|n| (n as i16).wrapping_mul(1733))
        .collect::<Vec<_>>();
    let metadata = [RawChunk {
        id: *b"meta",
        data: b"0123456789abcdef0123456789abcdef",
    }; 16];
    let options = WriteOptions {
        container: Container::Riff,
        auto_rf64: false,
        force_extensible: false,
    };
    let empty = Case {
        options,
        known: false,
        metadata: &[],
        samples: &[],
        batch: 4096,
        silence: 0,
    };
    let cases = [
        ("wavl/new_empty", 1_000_000, empty),
        (
            "wavl/known_empty",
            1_000_000,
            Case {
                known: true,
                ..empty
            },
        ),
        (
            "wavl/new_metadata16",
            100_000,
            Case {
                metadata: &metadata,
                ..empty
            },
        ),
        (
            "wavl/known_metadata16",
            100_000,
            Case {
                known: true,
                metadata: &metadata,
                ..empty
            },
        ),
        (
            "wavl/segmented_i16",
            128,
            Case {
                samples: &samples,
                ..empty
            },
        ),
        (
            "wavl/segmented_i16_known",
            128,
            Case {
                known: true,
                samples: &samples,
                ..empty
            },
        ),
        (
            "wavl/u64_silence",
            100_000,
            Case {
                options: WriteOptions {
                    auto_rf64: true,
                    ..options
                },
                silence: u64::from(u32::MAX) + 17,
                ..empty
            },
        ),
    ];
    let mut output = vec![0; samples.len() * 2 + 8192];
    let mut decoded = vec![0; samples.len()];
    println!(
        "# state_bytes: WavlWriter={}",
        std::mem::size_of::<WavlWriter<Cursor<&mut [u8]>>>()
    );
    println!(
        "# resident PCM16 stereo; caller_output_bytes={}; batch_samples=4096; silence_is_virtual; all decode verification outside measurement",
        output.len()
    );
    println!(
        "operation\ttrial\titers\tns_per_op\tthread_cycles_per_op\tMiB_per_s\talloc_calls\trequested_bytes\tpeak_requested_live_bytes"
    );
    for (name, iters, case) in cases {
        let n = render(&mut output, case);
        let wave = Wave::parse(&output[..n]).unwrap();
        let pcm = wave.pcm().unwrap();
        assert_eq!(pcm.frames(), case.samples.len() as u64 / 2 + case.silence);
        if !case.samples.is_empty() {
            assert_eq!(pcm.read_i16(0, &mut decoded).unwrap(), samples.len());
            assert_eq!(decoded, samples);
        }
        if case.silence != 0 {
            let mut last = [42; 2];
            assert_eq!(pcm.read_i16(pcm.frames() - 1, &mut last).unwrap(), 2);
            assert_eq!(last, [0; 2]);
        }
        bench(name, iters, case.samples.len() * 2, || {
            black_box(render(black_box(&mut output), black_box(case)));
        });
    }
}
