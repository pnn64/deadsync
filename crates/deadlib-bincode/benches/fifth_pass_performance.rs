use bincode::{
    config::{self, Config},
    de::{Decode, Decoder},
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

const STRING_COUNT: usize = 16_384;
const BUFFER_COUNT: usize = 4_096;
const BYTE_BUFFER_LEN: usize = 256;
const NUMBER_BUFFER_LEN: usize = 64;

struct CountingAlloc;

static TRACK_ALLOC: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static ZEROED_ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static DEALLOCS: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

// SAFETY: every operation forwards the caller's pointer, layout, and size
// unchanged to `System`; the atomic counters do not affect allocation behavior.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: this forwards the exact layout supplied by the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if TRACK_ALLOC.load(Ordering::Relaxed) && !ptr.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: this forwards the exact layout supplied by the allocator caller.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if TRACK_ALLOC.load(Ordering::Relaxed) && !ptr.is_null() {
            ZEROED_ALLOCS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if TRACK_ALLOC.load(Ordering::Relaxed) {
            DEALLOCS.fetch_add(1, Ordering::Relaxed);
            DEALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this forwards the pointer and layout supplied by the allocator caller.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: this forwards the pointer, layout, and size supplied by the caller.
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if TRACK_ALLOC.load(Ordering::Relaxed) && !new_ptr.is_null() {
            REALLOCS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
            DEALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        new_ptr
    }
}

#[derive(Clone, Copy, Debug)]
struct AllocStats {
    allocs: u64,
    zeroed_allocs: u64,
    reallocs: u64,
    deallocs: u64,
    allocated: u64,
    deallocated: u64,
}

fn allocations(run: impl FnOnce()) -> AllocStats {
    ALLOCS.store(0, Ordering::Relaxed);
    ZEROED_ALLOCS.store(0, Ordering::Relaxed);
    REALLOCS.store(0, Ordering::Relaxed);
    DEALLOCS.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
    DEALLOC_BYTES.store(0, Ordering::Relaxed);
    TRACK_ALLOC.store(true, Ordering::SeqCst);
    run();
    TRACK_ALLOC.store(false, Ordering::SeqCst);
    AllocStats {
        allocs: ALLOCS.load(Ordering::Relaxed),
        zeroed_allocs: ZEROED_ALLOCS.load(Ordering::Relaxed),
        reallocs: REALLOCS.load(Ordering::Relaxed),
        deallocs: DEALLOCS.load(Ordering::Relaxed),
        allocated: ALLOC_BYTES.load(Ordering::Relaxed),
        deallocated: DEALLOC_BYTES.load(Ordering::Relaxed),
    }
}

#[cfg(windows)]
fn thread_cycles(run: impl FnOnce()) -> u64 {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
    }

    // SAFETY: GetCurrentThread returns a valid pseudo-handle for the calling
    // thread and both cycle output pointers remain valid for each call.
    unsafe {
        let thread = GetCurrentThread();
        let mut start = 0;
        let mut end = 0;
        assert_ne!(QueryThreadCycleTime(thread, &mut start), 0);
        run();
        assert_ne!(QueryThreadCycleTime(thread, &mut end), 0);
        end - start
    }
}

#[cfg(windows)]
fn cycles_per(iterations: u64, mut run: impl FnMut()) -> u64 {
    run();
    thread_cycles(|| {
        for _ in 0..iterations {
            run();
        }
    }) / iterations
}

// This is the generic fallback used by reusable-vector decoding immediately
// before this pass, after the existing type-specific checks decline these
// element types. The comparison is conservative because it omits that old
// dispatch overhead: the outer Vec keeps its allocation, while clearing it
// drops every allocation owned by an element.
fn old_decode_into<T: Decode<()>, C: Config>(
    src: &[u8],
    values: &mut Vec<T>,
    config: C,
) -> Result<(), bincode::error::DecodeError> {
    values.clear();
    let reader = bincode::de::read::SliceReader::new(src);
    let mut decoder = bincode::de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    let encoded_len = u64::decode(&mut decoder)?;
    let len = usize::try_from(encoded_len)
        .map_err(|_| bincode::error::DecodeError::OutsideUsizeRange(encoded_len))?;
    decoder.claim_container_read::<T>(len)?;
    values.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<T>());
        values.push(T::decode(&mut decoder)?);
    }
    Ok(())
}

fn strings() -> Vec<String> {
    (0..STRING_COUNT)
        .map(|index| format!("chart-{index:05}-{}", "dead-sync".repeat(7)))
        .collect()
}

fn byte_buffers() -> Vec<Vec<u8>> {
    (0..BUFFER_COUNT)
        .map(|buffer| {
            (0..BYTE_BUFFER_LEN)
                .map(|index| (buffer.wrapping_mul(31).wrapping_add(index)) as u8)
                .collect()
        })
        .collect()
}

fn number_buffers() -> Vec<Vec<u64>> {
    (0..BUFFER_COUNT)
        .map(|buffer| {
            (0..NUMBER_BUFFER_LEN)
                .map(|index| {
                    let value = (buffer * NUMBER_BUFFER_LEN + index) as u64;
                    value.wrapping_mul(1_000_003) ^ value.rotate_left(17)
                })
                .collect()
        })
        .collect()
}

fn assert_zero_churn(stats: AllocStats) {
    assert_eq!(
        (
            stats.allocs,
            stats.zeroed_allocs,
            stats.reallocs,
            stats.deallocs,
            stats.allocated,
            stats.deallocated,
        ),
        (0, 0, 0, 0, 0, 0)
    );
}

fn allocation_pair<T>(encoded: &[u8], expected: &[T]) -> (AllocStats, AllocStats)
where
    T: Decode<()> + PartialEq + std::fmt::Debug,
{
    let config = config::standard();
    let mut old: Vec<T> = Vec::new();
    let mut new: Vec<T> = Vec::new();
    old_decode_into(encoded, &mut old, config).unwrap();
    bincode::decode_from_slice_into_vec(encoded, &mut new, config).unwrap();

    let old_stats = allocations(|| old_decode_into(encoded, &mut old, config).unwrap());
    let new_stats = allocations(|| {
        bincode::decode_from_slice_into_vec(encoded, &mut new, config).unwrap();
    });
    assert_eq!(old, expected);
    assert_eq!(new, expected);
    assert!(old_stats.allocs + old_stats.zeroed_allocs > 0);
    assert!(old_stats.deallocs > 0);
    assert_zero_churn(new_stats);
    (old_stats, new_stats)
}

fn report_allocations(
    encoded_strings: &[u8],
    strings: &[String],
    encoded_bytes: &[u8],
    bytes: &[Vec<u8>],
    encoded_numbers: &[u8],
    numbers: &[Vec<u64>],
) {
    let string_stats = allocation_pair(encoded_strings, strings);
    let byte_stats = allocation_pair(encoded_bytes, bytes);
    let number_stats = allocation_pair(encoded_numbers, numbers);
    eprintln!(
        "fifth-pass allocation profile per operation:\n\
         string elements old/new {string_stats:?}\n\
         byte buffers    old/new {byte_stats:?}\n\
         number buffers  old/new {number_stats:?}"
    );
}

#[cfg(windows)]
fn cycle_pair<T: Decode<()>>(encoded: &[u8], iterations: u64) -> (u64, u64) {
    let config = config::standard();
    let mut old: Vec<T> = Vec::new();
    let mut new: Vec<T> = Vec::new();
    old_decode_into(encoded, &mut old, config).unwrap();
    bincode::decode_from_slice_into_vec(encoded, &mut new, config).unwrap();
    let old_cycles = cycles_per(iterations, || {
        old_decode_into(black_box(encoded), black_box(&mut old), config).unwrap();
        black_box(old.len());
    });
    let new_cycles = cycles_per(iterations, || {
        bincode::decode_from_slice_into_vec(black_box(encoded), black_box(&mut new), config)
            .unwrap();
        black_box(new.len());
    });
    (old_cycles, new_cycles)
}

#[cfg(windows)]
fn report_cycles(encoded_strings: &[u8], encoded_bytes: &[u8], encoded_numbers: &[u8]) {
    let string_cycles = cycle_pair::<String>(encoded_strings, 50);
    let byte_cycles = cycle_pair::<Vec<u8>>(encoded_bytes, 50);
    let number_cycles = cycle_pair::<Vec<u64>>(encoded_numbers, 50);
    eprintln!(
        "fifth-pass thread cycles per operation:\n\
         string elements old/new {string_cycles:?}\n\
         byte buffers    old/new {byte_cycles:?}\n\
         number buffers  old/new {number_cycles:?}"
    );
}

fn bench_pair<T>(
    c: &mut Criterion,
    group_name: &str,
    encoded: &[u8],
    mut old: Vec<T>,
    mut new: Vec<T>,
) where
    T: Decode<()>,
{
    let config = config::standard();
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Bytes(encoded.len() as u64));
    group.bench_function(
        BenchmarkId::new("old_inner_allocating", encoded.len()),
        |b| {
            b.iter(|| {
                old_decode_into(black_box(encoded), black_box(&mut old), config).unwrap();
                black_box(old.len());
            });
        },
    );
    group.bench_function(BenchmarkId::new("new_deep_reused", encoded.len()), |b| {
        b.iter(|| {
            bincode::decode_from_slice_into_vec(black_box(encoded), black_box(&mut new), config)
                .unwrap();
            black_box(new.len());
        });
    });
    group.finish();
}

fn fifth_pass_performance(c: &mut Criterion) {
    let config = config::standard();
    let strings = strings();
    let bytes = byte_buffers();
    let numbers = number_buffers();
    let encoded_strings = bincode::encode_to_vec(&strings, config).unwrap();
    let encoded_bytes = bincode::encode_to_vec(&bytes, config).unwrap();
    let encoded_numbers = bincode::encode_to_vec(&numbers, config).unwrap();

    report_allocations(
        &encoded_strings,
        &strings,
        &encoded_bytes,
        &bytes,
        &encoded_numbers,
        &numbers,
    );
    #[cfg(windows)]
    report_cycles(&encoded_strings, &encoded_bytes, &encoded_numbers);

    bench_pair(
        c,
        "deep_reuse_string_elements",
        &encoded_strings,
        strings.clone(),
        strings,
    );
    bench_pair(
        c,
        "deep_reuse_byte_buffers",
        &encoded_bytes,
        bytes.clone(),
        bytes,
    );
    bench_pair(
        c,
        "deep_reuse_number_buffers",
        &encoded_numbers,
        numbers.clone(),
        numbers,
    );
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = fifth_pass_performance
}
criterion_main!(benches);
