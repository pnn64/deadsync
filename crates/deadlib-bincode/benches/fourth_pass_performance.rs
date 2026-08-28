use bincode::{
    config,
    de::{Decode, Decoder},
    Encode,
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

const ARRAY_LEN: usize = 4_096;

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

#[derive(Encode)]
struct Payload {
    names: Vec<String>,
    samples: Vec<u64>,
}

fn payload() -> Payload {
    Payload {
        names: (0..4_096)
            .map(|index| format!("chart-{index:04}-{}", "dead-sync".repeat(8)))
            .collect(),
        samples: (0..131_072).map(|value| value * 1_000_003).collect(),
    }
}

struct OldFloatArray([f32; ARRAY_LEN]);

impl<Context> Decode<Context> for OldFloatArray {
    fn decode<D: Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        decoder.claim_container_read::<f32>(ARRAY_LEN)?;
        let mut values = [0.0; ARRAY_LEN];
        for value in &mut values {
            decoder.unclaim_bytes_read(std::mem::size_of::<f32>());
            *value = f32::decode(decoder)?;
        }
        Ok(Self(values))
    }
}

fn report_allocations(data: &Payload, encoded: &[u8], array: &[u8]) {
    let config = config::standard();
    let mut dst = vec![0u8; encoded.len()];

    let old_size = allocations(|| {
        black_box(bincode::encode_to_vec(data, config).unwrap().len());
    });
    let new_size = allocations(|| {
        black_box(bincode::encoded_size(data, config).unwrap());
    });
    let old_slice = allocations(|| {
        drop(bincode::encode_to_vec(data, config).unwrap());
    });
    let new_slice = allocations(|| {
        black_box(bincode::encode_into_slice(data, &mut dst, config).unwrap());
    });
    let old_array = allocations(|| {
        black_box(bincode::decode_from_slice::<OldFloatArray, _>(array, config).unwrap());
    });
    let new_array = allocations(|| {
        black_box(bincode::decode_from_slice::<[f32; ARRAY_LEN], _>(array, config).unwrap());
    });
    assert!(old_size.allocs + old_size.zeroed_allocs > 0);
    assert!(old_slice.allocs + old_slice.zeroed_allocs > 0);
    for stats in [new_size, new_slice, old_array, new_array] {
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
    eprintln!(
        "fourth-pass allocation profile per operation:\n\
         allocating size query {old_size:?}\n\
         size writer           {new_size:?}\n\
         allocating encode    {old_slice:?}\n\
         slice encode         {new_slice:?}\n\
         scalar array decode  {old_array:?}\n\
         batched array decode {new_array:?}"
    );
}

#[cfg(windows)]
fn report_cycles(data: &Payload, encoded: &[u8], array: &[u8]) {
    let config = config::standard();
    let mut dst = vec![0u8; encoded.len()];
    let old_size = cycles_per(100, || {
        black_box(
            bincode::encode_to_vec(black_box(data), config)
                .unwrap()
                .len(),
        );
    });
    let new_size = cycles_per(100, || {
        black_box(bincode::encoded_size(black_box(data), config).unwrap());
    });
    let old_slice = cycles_per(100, || {
        black_box(bincode::encode_to_vec(black_box(data), config).unwrap());
    });
    let new_slice = cycles_per(100, || {
        black_box(
            bincode::encode_into_slice(black_box(data), black_box(&mut dst), config).unwrap(),
        );
    });
    let old_array = cycles_per(1_000, || {
        black_box(
            bincode::decode_from_slice::<OldFloatArray, _>(black_box(array), config).unwrap(),
        );
    });
    let new_array = cycles_per(1_000, || {
        black_box(
            bincode::decode_from_slice::<[f32; ARRAY_LEN], _>(black_box(array), config).unwrap(),
        );
    });
    eprintln!(
        "fourth-pass thread cycles per operation:\n\
         allocating size query {old_size}\n\
         size writer           {new_size}\n\
         allocating encode    {old_slice}\n\
         slice encode         {new_slice}\n\
         scalar array decode  {old_array}\n\
         batched array decode {new_array}"
    );
}

fn fourth_pass_performance(c: &mut Criterion) {
    let config = config::standard();
    let data = payload();
    let encoded = bincode::encode_to_vec(&data, config).unwrap();
    let mut dst = vec![0u8; encoded.len()];
    assert_eq!(bincode::encoded_size(&data, config).unwrap(), encoded.len());
    assert_eq!(
        bincode::encode_into_slice(&data, &mut dst, config).unwrap(),
        encoded.len()
    );
    assert_eq!(dst, encoded);

    let array_values =
        std::array::from_fn::<_, ARRAY_LEN, _>(|index| (index as f32).mul_add(0.125, -256.0));
    let array = bincode::encode_to_vec(array_values, config).unwrap();
    let old_array = bincode::decode_from_slice::<OldFloatArray, _>(&array, config)
        .unwrap()
        .0;
    let new_array = bincode::decode_from_slice::<[f32; ARRAY_LEN], _>(&array, config)
        .unwrap()
        .0;
    assert!(old_array
        .0
        .iter()
        .zip(new_array)
        .all(|(old, new)| old.to_bits() == new.to_bits()));

    report_allocations(&data, &encoded, &array);
    #[cfg(windows)]
    report_cycles(&data, &encoded, &array);

    let mut sizes = c.benchmark_group("encoded_size_query");
    sizes.throughput(Throughput::Bytes(encoded.len() as u64));
    sizes.bench_function(BenchmarkId::new("old_allocating", encoded.len()), |b| {
        b.iter(|| {
            black_box(
                bincode::encode_to_vec(black_box(&data), config)
                    .unwrap()
                    .len(),
            );
        });
    });
    sizes.bench_function(BenchmarkId::new("new_size_writer", encoded.len()), |b| {
        b.iter(|| {
            black_box(bincode::encoded_size(black_box(&data), config).unwrap());
        });
    });
    sizes.finish();

    let mut slices = c.benchmark_group("caller_slice_encode");
    slices.throughput(Throughput::Bytes(encoded.len() as u64));
    slices.bench_function(BenchmarkId::new("old_allocating", encoded.len()), |b| {
        b.iter(|| {
            black_box(bincode::encode_to_vec(black_box(&data), config).unwrap());
        });
    });
    slices.bench_function(BenchmarkId::new("new_slice", encoded.len()), |b| {
        b.iter(|| {
            black_box(
                bincode::encode_into_slice(black_box(&data), black_box(&mut dst), config).unwrap(),
            );
        });
    });
    slices.finish();

    let mut arrays = c.benchmark_group("fixed_numeric_array_decode");
    arrays.throughput(Throughput::Bytes(array.len() as u64));
    arrays.bench_function(BenchmarkId::new("old_scalar", array.len()), |b| {
        b.iter(|| {
            black_box(
                bincode::decode_from_slice::<OldFloatArray, _>(black_box(&array), config).unwrap(),
            );
        });
    });
    arrays.bench_function(BenchmarkId::new("new_batched", array.len()), |b| {
        b.iter(|| {
            black_box(
                bincode::decode_from_slice::<[f32; ARRAY_LEN], _>(black_box(&array), config)
                    .unwrap(),
            );
        });
    });
    arrays.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = fourth_pass_performance
}
criterion_main!(benches);
