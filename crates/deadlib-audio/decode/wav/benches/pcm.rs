//! Resident PCM16 baseline with caller-owned buffers and scoped heap accounting.
use deadlib_audio_decode_wav::{Container, Reader, SampleFormat, Spec, Wave, WriteOptions, Writer};
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

fn main() {
    let frames = 1 << 20;
    let samples: Vec<i16> = (0..frames * 2)
        .map(|i| (i as u32).wrapping_mul(1664525).wrapping_add(1013904223) as i16)
        .collect();
    let spec = Spec::new(SampleFormat::I16, 2, 48000);
    let options = WriteOptions {
        container: Container::Riff,
        auto_rf64: false,
        force_extensible: false,
    };
    let mut writer = Writer::new(Cursor::new(Vec::new()), spec, options, &[]).unwrap();
    writer.write_i16(&samples).unwrap();
    let encoded = writer.finish().unwrap().into_inner();
    let wave = Wave::parse(&encoded).unwrap();
    assert_eq!(wave.pcm().unwrap().as_i16().unwrap(), samples);
    let mut decoded = vec![0i16; samples.len()];
    let mut hound = hound::WavReader::new(Cursor::new(&encoded)).unwrap();
    for (o, v) in decoded.iter_mut().zip(hound.samples::<i16>()) {
        *o = v.unwrap();
    }
    assert_eq!(decoded, samples);
    println!(
        "# frames={frames}; channels=2; sample=i16; encoded_bytes={}; caller_full_output_bytes={}; stream_output_bytes=8192",
        encoded.len(),
        samples.len() * 2
    );
    println!(
        "operation\ttrial\titers\tns_per_op\tthread_cycles_per_op\tMiB_per_s\talloc_calls\trequested_bytes\tpeak_requested_live_bytes"
    );
    bench("wav/open_borrowed", 100_000, 0, || {
        black_box(Wave::parse(black_box(&encoded)).unwrap());
    });
    bench("hound/open", 100_000, 0, || {
        black_box(hound::WavReader::new(Cursor::new(black_box(&encoded))).unwrap());
    });
    bench("wav/direct_view", 100_000, 0, || {
        black_box(
            Wave::parse(black_box(&encoded))
                .unwrap()
                .pcm()
                .unwrap()
                .as_i16()
                .unwrap(),
        );
    });
    bench("wav/read_caller_i16", 128, samples.len() * 2, || {
        let p = Wave::parse(black_box(&encoded)).unwrap().pcm().unwrap();
        assert_eq!(
            p.read_i16(0, black_box(&mut decoded)).unwrap(),
            samples.len()
        );
        black_box(&decoded);
    });
    assert_eq!(decoded, samples);
    bench("hound/read_caller_i16", 32, samples.len() * 2, || {
        let mut r = hound::WavReader::new(Cursor::new(black_box(&encoded))).unwrap();
        for (o, v) in black_box(&mut decoded).iter_mut().zip(r.samples::<i16>()) {
            *o = v.unwrap();
        }
        black_box(&decoded);
    });
    assert_eq!(decoded, samples);
    bench("wav/stream_4096_samples", 64, samples.len() * 2, || {
        let mut r = Reader::new(Cursor::new(black_box(&encoded))).unwrap();
        let mut out = [0i16; 4096];
        let mut count = 0;
        loop {
            let n = r.read_i16(black_box(&mut out)).unwrap();
            if n == 0 {
                break;
            }
            count += n;
            black_box(&out);
        }
        assert_eq!(count, samples.len());
    });
    bench("hound/stream_4096_samples", 32, samples.len() * 2, || {
        let mut r = hound::WavReader::new(Cursor::new(black_box(&encoded))).unwrap();
        let mut it = r.samples::<i16>();
        let mut out = [0i16; 4096];
        let mut count = 0;
        while count < samples.len() {
            for o in black_box(&mut out) {
                *o = it.next().unwrap().unwrap();
            }
            count += out.len();
            black_box(&out);
        }
        assert_eq!(count, samples.len());
    });
    let mut r = Reader::new(Cursor::new(&encoded)).unwrap();
    let mut h = hound::WavReader::new(Cursor::new(&encoded)).unwrap();
    let mut position = 0u64;
    let mut out = [0i16; 512];
    bench("wav/seek_read_256_frames", 50_000, out.len() * 2, || {
        position = (position + 7919) % (frames as u64 - 256);
        r.seek_frame(black_box(position)).unwrap();
        assert_eq!(r.read_i16(black_box(&mut out)).unwrap(), 512);
        black_box(&out);
    });
    assert_eq!(
        out,
        samples[position as usize * 2..position as usize * 2 + 512]
    );
    bench("hound/seek_read_256_frames", 50_000, out.len() * 2, || {
        position = (position + 7919) % (frames as u64 - 256);
        h.seek(black_box(position as u32)).unwrap();
        for (o, v) in black_box(&mut out).iter_mut().zip(h.samples::<i16>()) {
            *o = v.unwrap();
        }
        black_box(&out);
    });
    assert_eq!(
        out,
        samples[position as usize * 2..position as usize * 2 + 512]
    );
    let mut output = vec![0u8; encoded.len() + 256];
    bench("wav/encode_native_i16", 128, samples.len() * 2, || {
        let mut w = Writer::new(
            Cursor::new(black_box(output.as_mut_slice())),
            spec,
            options,
            &[],
        )
        .unwrap();
        w.write_i16(black_box(&samples)).unwrap();
        black_box(w.finish().unwrap());
    });
    assert_eq!(
        Wave::parse(&output)
            .unwrap()
            .pcm()
            .unwrap()
            .as_i16()
            .unwrap(),
        samples
    );
    bench("wav/encode_known_i16", 128, samples.len() * 2, || {
        let mut w = Writer::with_frames(
            black_box(output.as_mut_slice()),
            spec,
            options,
            &[],
            frames as u64,
        )
        .unwrap();
        w.write_i16(black_box(&samples)).unwrap();
        black_box(w.finish_stream().unwrap());
    });
    assert_eq!(
        Wave::parse(&output)
            .unwrap()
            .pcm()
            .unwrap()
            .as_i16()
            .unwrap(),
        samples
    );
    let hs = hound::WavSpec {
        channels: 2,
        sample_rate: 48000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    bench("hound/encode_i16_bulk", 64, samples.len() * 2, || {
        let mut w =
            hound::WavWriter::new(Cursor::new(black_box(output.as_mut_slice())), hs).unwrap();
        let mut batch = w.get_i16_writer(samples.len() as u32);
        for &s in black_box(&samples) {
            // SAFETY: exactly the number of samples requested above is written.
            unsafe {
                batch.write_sample_unchecked(s);
            }
        }
        batch.flush().unwrap();
        w.finalize().unwrap();
    });
    let mut h = hound::WavReader::new(Cursor::new(&output)).unwrap();
    for (o, v) in decoded.iter_mut().zip(h.samples::<i16>()) {
        *o = v.unwrap();
    }
    assert_eq!(decoded, samples);
    bench("hound/encode_i16_blocks", 64, samples.len() * 2, || {
        let mut w =
            hound::WavWriter::new(Cursor::new(black_box(output.as_mut_slice())), hs).unwrap();
        for block in black_box(&samples).chunks(4096) {
            let mut batch = w.get_i16_writer(block.len() as u32);
            for &s in block {
                // SAFETY: exactly the requested block length is written.
                unsafe {
                    batch.write_sample_unchecked(s);
                }
            }
            batch.flush().unwrap();
        }
        w.finalize().unwrap();
    });
    let mut h = hound::WavReader::new(Cursor::new(&output)).unwrap();
    for (o, v) in decoded.iter_mut().zip(h.samples::<i16>()) {
        *o = v.unwrap();
    }
    assert_eq!(decoded, samples);
}
