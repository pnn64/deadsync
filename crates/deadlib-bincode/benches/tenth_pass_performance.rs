use bincode::{
    config::{self, Config},
    de::{Decode, Decoder},
    enc::{write::Writer, Encode, Encoder},
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

const TOTAL_BYTES: usize = 1 << 20;
const OUTER_COUNT: usize = 256;

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
    // and both output pointers remain valid for the duration of each call.
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

// These wrappers reproduce the per-inner-array loops immediately before this
// pass while retaining the same wire bytes and limit accounting.
struct OldSlice<'a, const N: usize>(&'a [[u8; N]]);

impl<const N: usize> Encode for OldSlice<'_, N> {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), bincode::error::EncodeError> {
        (self.0.len() as u64).encode(encoder)?;
        for value in self.0 {
            value.encode(encoder)?;
        }
        Ok(())
    }
}

struct OldOuterEncoding<'a, const N: usize, const M: usize>(&'a [[u8; N]; M]);

impl<const N: usize, const M: usize> Encode for OldOuterEncoding<'_, N, M> {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), bincode::error::EncodeError> {
        for value in self.0 {
            value.encode(encoder)?;
        }
        Ok(())
    }
}

struct OldVec<const N: usize>(Vec<[u8; N]>);

impl<Context, const N: usize> Decode<Context> for OldVec<N> {
    fn decode<D: Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let encoded_len = u64::decode(decoder)?;
        let len = usize::try_from(encoded_len)
            .map_err(|_| bincode::error::DecodeError::OutsideUsizeRange(encoded_len))?;
        decoder.claim_container_read::<[u8; N]>(len)?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            decoder.unclaim_bytes_read(std::mem::size_of::<[u8; N]>());
            values.push(<[u8; N]>::decode(decoder)?);
        }
        Ok(Self(values))
    }
}

struct OldOuter<const N: usize, const M: usize>([[u8; N]; M]);

impl<Context, const N: usize, const M: usize> Decode<Context> for OldOuter<N, M> {
    fn decode<D: Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        decoder.claim_bytes_read(std::mem::size_of::<[[u8; N]; M]>())?;
        let mut values = [[0; N]; M];
        for value in &mut values {
            decoder.unclaim_bytes_read(std::mem::size_of::<[u8; N]>());
            *value = <[u8; N]>::decode(decoder)?;
        }
        Ok(Self(values))
    }
}

fn old_decode_vec_into<const N: usize, C: Config>(
    src: &[u8],
    values: &mut Vec<[u8; N]>,
    config: C,
) -> Result<(), bincode::error::DecodeError> {
    values.clear();
    let reader = bincode::de::read::SliceReader::new(src);
    let mut decoder = bincode::de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    let encoded_len = u64::decode(&mut decoder)?;
    let len = usize::try_from(encoded_len)
        .map_err(|_| bincode::error::DecodeError::OutsideUsizeRange(encoded_len))?;
    decoder.claim_container_read::<[u8; N]>(len)?;
    values.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<[u8; N]>());
        values.push(<[u8; N]>::decode(&mut decoder)?);
    }
    Ok(())
}

#[derive(Default)]
struct WriteStats {
    calls: usize,
    bytes: usize,
}

impl Writer for WriteStats {
    fn write(&mut self, bytes: &[u8]) -> Result<(), bincode::error::EncodeError> {
        self.calls += 1;
        self.bytes += bytes.len();
        Ok(())
    }
}

fn byte_arrays<const N: usize>() -> Vec<[u8; N]> {
    (0..TOTAL_BYTES / N)
        .map(|outer| {
            std::array::from_fn(|inner| {
                outer.wrapping_mul(31).wrapping_add(inner.wrapping_mul(17)) as u8
            })
        })
        .collect()
}

fn report_allocations<const N: usize>(
    values: &[[u8; N]],
    outer: &[[u8; N]; OUTER_COUNT],
    encoded: &[u8],
    encoded_outer: &[u8],
) {
    let config = config::standard();
    let mut old_writer = bincode::enc::EncoderImpl::new(WriteStats::default(), config);
    OldSlice(values).encode(&mut old_writer).unwrap();
    let old_writer = old_writer.into_writer();
    let mut new_writer = bincode::enc::EncoderImpl::new(WriteStats::default(), config);
    values.encode(&mut new_writer).unwrap();
    let new_writer = new_writer.into_writer();
    let mut old_outer_writer = bincode::enc::EncoderImpl::new(WriteStats::default(), config);
    OldOuterEncoding(outer)
        .encode(&mut old_outer_writer)
        .unwrap();
    let old_outer_writer = old_outer_writer.into_writer();
    let mut new_outer_writer = bincode::enc::EncoderImpl::new(WriteStats::default(), config);
    outer.encode(&mut new_outer_writer).unwrap();
    let new_outer_writer = new_outer_writer.into_writer();
    assert_eq!(old_writer.calls, values.len() + 1);
    assert_eq!(new_writer.calls, 2);
    assert_eq!(old_writer.bytes, new_writer.bytes);
    assert_eq!(old_outer_writer.calls, OUTER_COUNT);
    assert_eq!(new_outer_writer.calls, 1);
    assert_eq!(old_outer_writer.bytes, new_outer_writer.bytes);

    let mut old_encoded = Vec::with_capacity(encoded.len());
    let mut new_encoded = Vec::with_capacity(encoded.len());
    bincode::encode_into_vec(OldSlice(values), &mut old_encoded, config).unwrap();
    bincode::encode_into_vec(values, &mut new_encoded, config).unwrap();
    let old_encode = allocations(|| {
        bincode::encode_into_vec(OldSlice(values), &mut old_encoded, config).unwrap();
    });
    let new_encode = allocations(|| {
        bincode::encode_into_vec(values, &mut new_encoded, config).unwrap();
    });
    assert_eq!(old_encoded, new_encoded);

    let old_allocating = allocations(|| {
        let (OldVec(decoded), _) =
            bincode::decode_from_slice::<OldVec<N>, _>(encoded, config).unwrap();
        assert_eq!(decoded, values);
    });
    let new_allocating = allocations(|| {
        let decoded = bincode::decode_from_slice::<Vec<[u8; N]>, _>(encoded, config)
            .unwrap()
            .0;
        assert_eq!(decoded, values);
    });

    let mut old_reused = Vec::<[u8; N]>::with_capacity(values.len());
    let mut new_reused = Vec::<[u8; N]>::with_capacity(values.len());
    old_decode_vec_into(encoded, &mut old_reused, config).unwrap();
    bincode::decode_from_slice_into_vec(encoded, &mut new_reused, config).unwrap();
    let old_reuse = allocations(|| old_decode_vec_into(encoded, &mut old_reused, config).unwrap());
    let new_reuse = allocations(|| {
        bincode::decode_from_slice_into_vec(encoded, &mut new_reused, config).unwrap();
    });
    assert_eq!(old_reused, values);
    assert_eq!(new_reused, values);

    let old_outer = allocations(|| {
        black_box(
            bincode::decode_from_slice::<OldOuter<N, OUTER_COUNT>, _>(encoded_outer, config)
                .unwrap(),
        );
    });
    let new_outer = allocations(|| {
        black_box(
            bincode::decode_from_slice::<[[u8; N]; OUTER_COUNT], _>(encoded_outer, config).unwrap(),
        );
    });

    let zero = AllocStats {
        allocs: 0,
        zeroed_allocs: 0,
        reallocs: 0,
        deallocs: 0,
        allocated: 0,
        deallocated: 0,
    };
    for stats in [
        old_encode, new_encode, old_reuse, new_reuse, old_outer, new_outer,
    ] {
        assert_eq!(stats, zero);
    }
    assert_eq!(old_allocating.allocs + old_allocating.zeroed_allocs, 1);
    assert_eq!(new_allocating.allocs + new_allocating.zeroed_allocs, 1);
    assert_eq!(old_allocating.reallocs, 0);
    assert_eq!(new_allocating.reallocs, 0);

    eprintln!(
        "[u8; {N}] allocation profile per operation:\n\
         encode vector     old/new {old_encode:?} / {new_encode:?}\n\
         allocating decode old/new {old_allocating:?} / {new_allocating:?}\n\
         reused decode     old/new {old_reuse:?} / {new_reuse:?}\n\
         outer array       old/new {old_outer:?} / {new_outer:?}"
    );
}

#[cfg(windows)]
fn report_cycles<const N: usize>(values: &[[u8; N]], encoded: &[u8], encoded_outer: &[u8]) {
    const ITERATIONS: u64 = 20;
    let config = config::standard();
    let mut old_encoded = Vec::with_capacity(encoded.len());
    let mut new_encoded = Vec::with_capacity(encoded.len());
    let mut old_reused = Vec::<[u8; N]>::with_capacity(values.len());
    let mut new_reused = Vec::<[u8; N]>::with_capacity(values.len());

    let old_encode = cycles_per(ITERATIONS, || {
        bincode::encode_into_vec(OldSlice(values), black_box(&mut old_encoded), config).unwrap();
        black_box(old_encoded.len());
    });
    let new_encode = cycles_per(ITERATIONS, || {
        bincode::encode_into_vec(values, black_box(&mut new_encoded), config).unwrap();
        black_box(new_encoded.len());
    });
    let old_decode = cycles_per(ITERATIONS, || {
        old_decode_vec_into(encoded, black_box(&mut old_reused), config).unwrap();
        black_box(old_reused.len());
    });
    let new_decode = cycles_per(ITERATIONS, || {
        bincode::decode_from_slice_into_vec(encoded, black_box(&mut new_reused), config).unwrap();
        black_box(new_reused.len());
    });
    let old_outer = cycles_per(ITERATIONS, || {
        black_box(
            bincode::decode_from_slice::<OldOuter<N, OUTER_COUNT>, _>(encoded_outer, config)
                .unwrap(),
        );
    });
    let new_outer = cycles_per(ITERATIONS, || {
        black_box(
            bincode::decode_from_slice::<[[u8; N]; OUTER_COUNT], _>(encoded_outer, config).unwrap(),
        );
    });

    eprintln!(
        "[u8; {N}] thread cycles per operation:\n\
         encode vector old/new {old_encode:?} / {new_encode:?}\n\
         reused decode old/new {old_decode:?} / {new_decode:?}\n\
         outer decode  old/new {old_outer:?} / {new_outer:?}"
    );
}

fn bench_encode<const N: usize>(c: &mut Criterion, name: &str, values: &[[u8; N]], len: usize) {
    let config = config::standard();
    let mut old = Vec::with_capacity(len);
    let mut new = Vec::with_capacity(len);
    let mut group = c.benchmark_group(name);
    group.throughput(Throughput::Bytes(len as u64));
    group.bench_function(BenchmarkId::new("old_inner_writes", len), |b| {
        b.iter(|| {
            bincode::encode_into_vec(OldSlice(black_box(values)), black_box(&mut old), config)
                .unwrap();
            black_box(old.len());
        });
    });
    group.bench_function(BenchmarkId::new("new_contiguous", len), |b| {
        b.iter(|| {
            bincode::encode_into_vec(black_box(values), black_box(&mut new), config).unwrap();
            black_box(new.len());
        });
    });
    group.finish();
}

fn bench_decode<const N: usize>(c: &mut Criterion, name: &str, encoded: &[u8], capacity: usize) {
    let config = config::standard();
    let mut old = Vec::<[u8; N]>::with_capacity(capacity);
    let mut new = Vec::<[u8; N]>::with_capacity(capacity);
    let mut group = c.benchmark_group(name);
    group.throughput(Throughput::Bytes(encoded.len() as u64));
    group.bench_function(BenchmarkId::new("old_inner_reads", encoded.len()), |b| {
        b.iter(|| {
            old_decode_vec_into(black_box(encoded), black_box(&mut old), config).unwrap();
            black_box(old.len());
        });
    });
    group.bench_function(BenchmarkId::new("new_contiguous", encoded.len()), |b| {
        b.iter(|| {
            bincode::decode_from_slice_into_vec(black_box(encoded), black_box(&mut new), config)
                .unwrap();
            black_box(new.len());
        });
    });
    group.finish();
}

fn bench_outer<const N: usize>(c: &mut Criterion, name: &str, encoded: &[u8]) {
    let config = config::standard();
    let mut group = c.benchmark_group(name);
    group.throughput(Throughput::Bytes(encoded.len() as u64));
    group.bench_function(BenchmarkId::new("old_inner_reads", encoded.len()), |b| {
        b.iter(|| {
            black_box(
                bincode::decode_from_slice::<OldOuter<N, OUTER_COUNT>, _>(
                    black_box(encoded),
                    config,
                )
                .unwrap(),
            );
        });
    });
    group.bench_function(BenchmarkId::new("new_contiguous", encoded.len()), |b| {
        b.iter(|| {
            black_box(
                bincode::decode_from_slice::<[[u8; N]; OUTER_COUNT], _>(black_box(encoded), config)
                    .unwrap(),
            );
        });
    });
    group.finish();
}

fn bench_size<const N: usize>(c: &mut Criterion) {
    let config = config::standard();
    let values = byte_arrays::<N>();
    let outer: [[u8; N]; OUTER_COUNT] = std::array::from_fn(|index| values[index]);
    let encoded = bincode::encode_to_vec(&values, config).unwrap();
    let encoded_outer = bincode::encode_to_vec(outer, config).unwrap();
    assert_eq!(
        bincode::encode_to_vec(OldSlice(&values), config).unwrap(),
        encoded
    );
    assert_eq!(
        bincode::encode_to_vec(OldOuterEncoding(&outer), config).unwrap(),
        encoded_outer
    );

    report_allocations(&values, &outer, &encoded, &encoded_outer);
    #[cfg(windows)]
    report_cycles::<N>(&values, &encoded, &encoded_outer);

    bench_encode(
        c,
        &format!("byte_array_{N}_slice_encode"),
        &values,
        encoded.len(),
    );
    bench_decode::<N>(
        c,
        &format!("byte_array_{N}_vector_decode"),
        &encoded,
        values.len(),
    );
    bench_outer::<N>(c, &format!("byte_array_{N}_outer_decode"), &encoded_outer);
}

fn tenth_pass_performance(c: &mut Criterion) {
    bench_size::<16>(c);
    bench_size::<32>(c);
    bench_size::<64>(c);
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = tenth_pass_performance
}
criterion_main!(benches);
