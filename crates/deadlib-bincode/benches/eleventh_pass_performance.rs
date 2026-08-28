use bincode::{
    config,
    de::{BorrowDecode, BorrowDecoder, Decode, Decoder},
};
use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

const INTEGER_COUNT: usize = 1 << 17;
const ARRAY_LEN: usize = 8192;

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
// unchanged to `System`; the atomic counters do not affect allocator behavior.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: this forwards the exact layout supplied by the caller.
        let ptr = unsafe { System.alloc(layout) };
        if TRACK_ALLOC.load(Ordering::Relaxed) && !ptr.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: this forwards the exact layout supplied by the caller.
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
        // SAFETY: this forwards the pointer and layout supplied by the caller.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

    // SAFETY: GetCurrentThread returns a valid pseudo-handle for this thread,
    // and both output pointers remain valid during the calls.
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

fn decode_len<D: Decoder>(decoder: &mut D) -> Result<usize, bincode::error::DecodeError> {
    let encoded = u64::decode(decoder)?;
    usize::try_from(encoded).map_err(|_| bincode::error::DecodeError::OutsideUsizeRange(encoded))
}

// These wrappers preserve the implementations immediately before this pass.
struct OldBorrowedVec(Vec<u64>);

impl<'de, Context> BorrowDecode<'de, Context> for OldBorrowedVec {
    fn borrow_decode<D: BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let len = decode_len(decoder)?;
        decoder.claim_container_read::<u64>(len)?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            decoder.unclaim_bytes_read(std::mem::size_of::<u64>());
            values.push(u64::borrow_decode(decoder)?);
        }
        Ok(Self(values))
    }
}

struct OldFixedVec(Vec<u64>);

impl<Context> Decode<Context> for OldFixedVec {
    fn decode<D: Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let len = decode_len(decoder)?;
        decoder.claim_container_read::<u64>(len)?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            decoder.unclaim_bytes_read(std::mem::size_of::<u64>());
            values.push(u64::decode(decoder)?);
        }
        Ok(Self(values))
    }
}

struct OldArray<const N: usize>([u64; N]);

impl<Context, const N: usize> Decode<Context> for OldArray<N> {
    fn decode<D: Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        decoder.claim_bytes_read(std::mem::size_of::<[u64; N]>())?;
        let mut values = [MaybeUninit::<u64>::uninit(); N];
        for value in &mut values {
            decoder.unclaim_bytes_read(std::mem::size_of::<u64>());
            value.write(u64::decode(decoder)?);
        }
        // SAFETY: every element was initialized above, and u64 needs no drop
        // cleanup if decoding returned early.
        Ok(Self(unsafe {
            (&values as *const [MaybeUninit<u64>; N])
                .cast::<[u64; N]>()
                .read()
        }))
    }
}

fn integer_values(count: usize) -> Vec<u64> {
    // Compact values are the common case that variable-width encoding is
    // designed for and make per-element limit bookkeeping most visible.
    const VALUES: [u64; 7] = [0, 1, 2, 42, 127, 249, 250];
    (0..count)
        .map(|index| VALUES[index % VALUES.len()])
        .collect()
}

fn report_allocations(integers: &[u8], array: &[u8], fixed: &[u8]) {
    let config = config::standard().with_limit::<{ usize::MAX }>();
    let fixed_config = config::legacy()
        .with_big_endian()
        .with_limit::<{ usize::MAX }>();
    let old_borrowed = allocations(|| {
        black_box(
            bincode::borrow_decode_from_slice::<OldBorrowedVec, _>(integers, config).unwrap(),
        );
    });
    let new_borrowed = allocations(|| {
        black_box(bincode::borrow_decode_from_slice::<Vec<u64>, _>(integers, config).unwrap());
    });
    let old_array = allocations(|| {
        black_box(bincode::decode_from_slice::<OldArray<ARRAY_LEN>, _>(array, config).unwrap());
    });
    let new_array = allocations(|| {
        black_box(bincode::decode_from_slice::<[u64; ARRAY_LEN], _>(array, config).unwrap());
    });
    let old_fixed = allocations(|| {
        black_box(bincode::decode_from_slice::<OldFixedVec, _>(fixed, fixed_config).unwrap());
    });
    let new_fixed = allocations(|| {
        black_box(bincode::decode_from_slice::<Vec<u64>, _>(fixed, fixed_config).unwrap());
    });

    println!("borrowed varint vector old/new {old_borrowed:?} / {new_borrowed:?}");
    println!("varint array old/new {old_array:?} / {new_array:?}");
    println!("fixed-endian vector old/new {old_fixed:?} / {new_fixed:?}");

    assert_eq!(old_borrowed.allocs + old_borrowed.zeroed_allocs, 1);
    assert_eq!(new_borrowed.allocs + new_borrowed.zeroed_allocs, 1);
    assert_eq!(old_borrowed.allocated, (INTEGER_COUNT * 8) as u64);
    assert_eq!(new_borrowed.allocated, (INTEGER_COUNT * 8) as u64);
    assert_eq!(old_array.allocs + old_array.zeroed_allocs, 0);
    assert_eq!(new_array.allocs + new_array.zeroed_allocs, 0);
    assert_eq!(old_fixed.allocs + old_fixed.zeroed_allocs, 1);
    assert_eq!(new_fixed.allocs + new_fixed.zeroed_allocs, 1);
    assert_eq!(old_fixed.allocated, (INTEGER_COUNT * 8) as u64);
    assert_eq!(new_fixed.allocated, (INTEGER_COUNT * 8) as u64);
}

#[cfg(windows)]
fn report_cycles(integers: &[u8], array: &[u8], fixed: &[u8]) {
    let config = config::standard().with_limit::<{ usize::MAX }>();
    let fixed_config = config::legacy()
        .with_big_endian()
        .with_limit::<{ usize::MAX }>();
    let old_borrowed = cycles_per(32, || {
        black_box(
            bincode::borrow_decode_from_slice::<OldBorrowedVec, _>(integers, config).unwrap(),
        );
    });
    let new_borrowed = cycles_per(32, || {
        black_box(bincode::borrow_decode_from_slice::<Vec<u64>, _>(integers, config).unwrap());
    });
    let old_array = cycles_per(256, || {
        black_box(bincode::decode_from_slice::<OldArray<ARRAY_LEN>, _>(array, config).unwrap());
    });
    let new_array = cycles_per(256, || {
        black_box(bincode::decode_from_slice::<[u64; ARRAY_LEN], _>(array, config).unwrap());
    });
    let old_fixed = cycles_per(32, || {
        black_box(bincode::decode_from_slice::<OldFixedVec, _>(fixed, fixed_config).unwrap());
    });
    let new_fixed = cycles_per(32, || {
        black_box(bincode::decode_from_slice::<Vec<u64>, _>(fixed, fixed_config).unwrap());
    });

    println!("borrowed vector cycles old/new {old_borrowed} / {new_borrowed}");
    println!("varint array cycles old/new {old_array} / {new_array}");
    println!("fixed-endian vector cycles old/new {old_fixed} / {new_fixed}");
}

fn benchmark(c: &mut Criterion) {
    let integers = integer_values(INTEGER_COUNT);
    let array = std::array::from_fn::<_, ARRAY_LEN, _>(|index| integers[index]);
    let config = config::standard().with_limit::<{ usize::MAX }>();
    let encoded_integers = bincode::encode_to_vec(&integers, config).unwrap();
    let encoded_array = bincode::encode_to_vec(array, config).unwrap();
    let fixed_config = config::legacy()
        .with_big_endian()
        .with_limit::<{ usize::MAX }>();
    let encoded_fixed = bincode::encode_to_vec(&integers, fixed_config).unwrap();
    assert_eq!(
        bincode::borrow_decode_from_slice::<OldBorrowedVec, _>(&encoded_integers, config)
            .unwrap()
            .0
             .0,
        integers
    );
    assert_eq!(
        bincode::decode_from_slice::<OldArray<ARRAY_LEN>, _>(&encoded_array, config)
            .unwrap()
            .0
             .0,
        array
    );
    assert_eq!(
        bincode::decode_from_slice::<OldFixedVec, _>(&encoded_fixed, fixed_config)
            .unwrap()
            .0
             .0,
        integers
    );

    report_allocations(&encoded_integers, &encoded_array, &encoded_fixed);
    #[cfg(windows)]
    report_cycles(&encoded_integers, &encoded_array, &encoded_fixed);

    let mut group = c.benchmark_group("borrowed_varint_vector_decode");
    group.throughput(Throughput::Elements(INTEGER_COUNT as u64));
    group.bench_function("old_generic_elements", |b| {
        b.iter(|| {
            black_box(
                bincode::borrow_decode_from_slice::<OldBorrowedVec, _>(&encoded_integers, config)
                    .unwrap(),
            )
        });
    });
    group.bench_function("new_specialized_loop", |b| {
        b.iter(|| {
            black_box(
                bincode::borrow_decode_from_slice::<Vec<u64>, _>(&encoded_integers, config)
                    .unwrap(),
            )
        });
    });
    group.finish();

    let mut group = c.benchmark_group("varint_array_decode");
    group.throughput(Throughput::Elements(ARRAY_LEN as u64));
    group.bench_function("old_generic_elements", |b| {
        b.iter(|| {
            black_box(
                bincode::decode_from_slice::<OldArray<ARRAY_LEN>, _>(&encoded_array, config)
                    .unwrap(),
            )
        });
    });
    group.bench_function("new_specialized_loop", |b| {
        b.iter(|| {
            black_box(
                bincode::decode_from_slice::<[u64; ARRAY_LEN], _>(&encoded_array, config).unwrap(),
            )
        });
    });
    group.finish();

    let mut group = c.benchmark_group("fixed_big_endian_vector_decode");
    group.throughput(Throughput::Bytes((INTEGER_COUNT * 8) as u64));
    group.bench_function("old_element_reads", |b| {
        b.iter(|| {
            black_box(
                bincode::decode_from_slice::<OldFixedVec, _>(&encoded_fixed, fixed_config).unwrap(),
            )
        });
    });
    group.bench_function("new_contiguous_parse", |b| {
        b.iter(|| {
            black_box(
                bincode::decode_from_slice::<Vec<u64>, _>(&encoded_fixed, fixed_config).unwrap(),
            )
        });
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = benchmark
}
criterion_main!(benches);
