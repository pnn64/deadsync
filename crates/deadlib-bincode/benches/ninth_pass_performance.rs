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

const VALUE_COUNT: usize = 262_144;
const ARRAY_COUNT: usize = 16_384;

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

// These wrappers reproduce the scalar slice/vector/array loops immediately
// before this pass while retaining identical wire data and limit accounting.
struct OldSlice<'a, T>(&'a [T]);

impl<T: Encode> Encode for OldSlice<'_, T> {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), bincode::error::EncodeError> {
        (self.0.len() as u64).encode(encoder)?;
        for value in self.0 {
            value.encode(encoder)?;
        }
        Ok(())
    }
}

struct OldArrayEncoding<'a, T, const N: usize>(&'a [T; N]);

impl<T: Encode, const N: usize> Encode for OldArrayEncoding<'_, T, N> {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), bincode::error::EncodeError> {
        for value in self.0 {
            value.encode(encoder)?;
        }
        Ok(())
    }
}

struct OldVec<T>(Vec<T>);

impl<Context, T: Decode<Context>> Decode<Context> for OldVec<T> {
    fn decode<D: Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let encoded_len = u64::decode(decoder)?;
        let len = usize::try_from(encoded_len)
            .map_err(|_| bincode::error::DecodeError::OutsideUsizeRange(encoded_len))?;
        decoder.claim_container_read::<T>(len)?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            decoder.unclaim_bytes_read(std::mem::size_of::<T>());
            values.push(T::decode(decoder)?);
        }
        Ok(Self(values))
    }
}

struct OldArray<T, const N: usize>([T; N]);

impl<Context, T, const N: usize> Decode<Context> for OldArray<T, N>
where
    T: Decode<Context> + Copy + Default,
{
    fn decode<D: Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        decoder.claim_bytes_read(std::mem::size_of::<[T; N]>())?;
        let mut values = [T::default(); N];
        for value in &mut values {
            decoder.unclaim_bytes_read(std::mem::size_of::<T>());
            *value = T::decode(decoder)?;
        }
        Ok(Self(values))
    }
}

fn old_decode_vec_into<T, C>(
    src: &[u8],
    values: &mut Vec<T>,
    config: C,
) -> Result<(), bincode::error::DecodeError>
where
    T: Decode<()>,
    C: Config,
{
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

fn bool_values() -> Vec<bool> {
    (0..VALUE_COUNT)
        .map(|index| index.wrapping_mul(31).count_ones() & 1 == 0)
        .collect()
}

fn signed_bytes() -> Vec<i8> {
    (0..VALUE_COUNT)
        .map(|index| index.wrapping_mul(31).wrapping_add(index >> 3) as i8)
        .collect()
}

fn write_stats<T: Encode, const N: usize>(values: &[T], array: &[T; N]) {
    let config = config::standard();
    let mut old_slice = bincode::enc::EncoderImpl::new(WriteStats::default(), config);
    OldSlice(values).encode(&mut old_slice).unwrap();
    let old_slice = old_slice.into_writer();
    let mut new_slice = bincode::enc::EncoderImpl::new(WriteStats::default(), config);
    values.encode(&mut new_slice).unwrap();
    let new_slice = new_slice.into_writer();

    let mut old_array = bincode::enc::EncoderImpl::new(WriteStats::default(), config);
    OldArrayEncoding(array).encode(&mut old_array).unwrap();
    let old_array = old_array.into_writer();
    let mut new_array = bincode::enc::EncoderImpl::new(WriteStats::default(), config);
    array.encode(&mut new_array).unwrap();
    let new_array = new_array.into_writer();

    assert_eq!(old_slice.calls, values.len() + 1);
    assert_eq!(new_slice.calls, 2);
    assert_eq!(old_slice.bytes, new_slice.bytes);
    assert_eq!(old_array.calls, N);
    assert_eq!(new_array.calls, 1);
    assert_eq!(old_array.bytes, new_array.bytes);
}

fn report_type_allocations<T, const N: usize>(
    label: &str,
    values: &[T],
    array: &[T; N],
    encoded: &[u8],
    encoded_array: &[u8],
) where
    T: Decode<()> + Encode + Copy + Default + PartialEq + std::fmt::Debug,
{
    let config = config::standard();
    write_stats(values, array);

    let mut old_reused = Vec::<T>::with_capacity(values.len());
    let mut new_reused = Vec::<T>::with_capacity(values.len());
    old_decode_vec_into(encoded, &mut old_reused, config).unwrap();
    bincode::decode_from_slice_into_vec(encoded, &mut new_reused, config).unwrap();
    let old_reused_stats =
        allocations(|| old_decode_vec_into(encoded, &mut old_reused, config).unwrap());
    let new_reused_stats = allocations(|| {
        bincode::decode_from_slice_into_vec(encoded, &mut new_reused, config).unwrap();
    });
    assert_eq!(old_reused, values);
    assert_eq!(new_reused, values);

    let old_allocating = allocations(|| {
        let (OldVec(decoded), _) =
            bincode::decode_from_slice::<OldVec<T>, _>(encoded, config).unwrap();
        assert_eq!(decoded, values);
    });
    let new_allocating = allocations(|| {
        let decoded = bincode::decode_from_slice::<Vec<T>, _>(encoded, config)
            .unwrap()
            .0;
        assert_eq!(decoded, values);
    });
    let old_array = allocations(|| {
        black_box(
            bincode::decode_from_slice::<OldArray<T, N>, _>(encoded_array, config)
                .unwrap()
                .0,
        );
    });
    let new_array = allocations(|| {
        black_box(
            bincode::decode_from_slice::<[T; N], _>(encoded_array, config)
                .unwrap()
                .0,
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
    assert_eq!(old_reused_stats, zero);
    assert_eq!(new_reused_stats, zero);
    assert_eq!(old_array, zero);
    assert_eq!(new_array, zero);
    assert_eq!(old_allocating.allocs + old_allocating.zeroed_allocs, 1);
    assert_eq!(new_allocating.allocs + new_allocating.zeroed_allocs, 1);
    assert_eq!(old_allocating.reallocs, 0);
    assert_eq!(new_allocating.reallocs, 0);

    eprintln!(
        "{label} allocation profile per operation:\n\
         allocating vector old/new {old_allocating:?} / {new_allocating:?}\n\
         reused vector     old/new {old_reused_stats:?} / {new_reused_stats:?}\n\
         fixed array       old/new {old_array:?} / {new_array:?}"
    );
}

#[cfg(windows)]
fn report_type_cycles<T, const N: usize>(
    label: &str,
    values: &[T],
    encoded: &[u8],
    encoded_array: &[u8],
) where
    T: Decode<()> + Encode + Copy + Default,
{
    const ITERATIONS: u64 = 25;
    let config = config::standard();
    let mut old_encoded = Vec::with_capacity(encoded.len());
    let mut new_encoded = Vec::with_capacity(encoded.len());
    let mut old_reused = Vec::<T>::with_capacity(values.len());
    let mut new_reused = Vec::<T>::with_capacity(values.len());

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
    let old_array = cycles_per(ITERATIONS, || {
        black_box(bincode::decode_from_slice::<OldArray<T, N>, _>(encoded_array, config).unwrap());
    });
    let new_array = cycles_per(ITERATIONS, || {
        black_box(bincode::decode_from_slice::<[T; N], _>(encoded_array, config).unwrap());
    });

    eprintln!(
        "{label} thread cycles per operation:\n\
         slice encode  old/new {old_encode:?} / {new_encode:?}\n\
         reused decode old/new {old_decode:?} / {new_decode:?}\n\
         array decode  old/new {old_array:?} / {new_array:?}"
    );
}

fn bench_encode<T: Encode>(c: &mut Criterion, name: &str, values: &[T], encoded_len: usize) {
    let config = config::standard();
    let mut old = Vec::with_capacity(encoded_len);
    let mut new = Vec::with_capacity(encoded_len);
    let mut group = c.benchmark_group(name);
    group.throughput(Throughput::Bytes(encoded_len as u64));
    group.bench_function(BenchmarkId::new("old_scalar", encoded_len), |b| {
        b.iter(|| {
            bincode::encode_into_vec(OldSlice(black_box(values)), black_box(&mut old), config)
                .unwrap();
            black_box(old.len());
        });
    });
    group.bench_function(BenchmarkId::new("new_batched", encoded_len), |b| {
        b.iter(|| {
            bincode::encode_into_vec(black_box(values), black_box(&mut new), config).unwrap();
            black_box(new.len());
        });
    });
    group.finish();
}

fn bench_reused_decode<T: Decode<()>>(
    c: &mut Criterion,
    name: &str,
    encoded: &[u8],
    capacity: usize,
) {
    let config = config::standard();
    let mut old = Vec::<T>::with_capacity(capacity);
    let mut new = Vec::<T>::with_capacity(capacity);
    let mut group = c.benchmark_group(name);
    group.throughput(Throughput::Bytes(encoded.len() as u64));
    group.bench_function(BenchmarkId::new("old_scalar", encoded.len()), |b| {
        b.iter(|| {
            old_decode_vec_into(black_box(encoded), black_box(&mut old), config).unwrap();
            black_box(old.len());
        });
    });
    group.bench_function(BenchmarkId::new("new_batched", encoded.len()), |b| {
        b.iter(|| {
            bincode::decode_from_slice_into_vec(black_box(encoded), black_box(&mut new), config)
                .unwrap();
            black_box(new.len());
        });
    });
    group.finish();
}

fn bench_array_decode<T, const N: usize>(c: &mut Criterion, name: &str, encoded: &[u8])
where
    T: Decode<()> + Copy + Default,
{
    let config = config::standard();
    let mut group = c.benchmark_group(name);
    group.throughput(Throughput::Bytes(encoded.len() as u64));
    group.bench_function(BenchmarkId::new("old_scalar", encoded.len()), |b| {
        b.iter(|| {
            black_box(
                bincode::decode_from_slice::<OldArray<T, N>, _>(black_box(encoded), config)
                    .unwrap(),
            );
        });
    });
    group.bench_function(BenchmarkId::new("new_batched", encoded.len()), |b| {
        b.iter(|| {
            black_box(bincode::decode_from_slice::<[T; N], _>(black_box(encoded), config).unwrap());
        });
    });
    group.finish();
}

fn ninth_pass_performance(c: &mut Criterion) {
    let config = config::standard();
    let bools = bool_values();
    let signed = signed_bytes();
    let bool_array: [bool; ARRAY_COUNT] = std::array::from_fn(|index| bools[index]);
    let signed_array: [i8; ARRAY_COUNT] = std::array::from_fn(|index| signed[index]);
    let encoded_bools = bincode::encode_to_vec(&bools, config).unwrap();
    let encoded_signed = bincode::encode_to_vec(&signed, config).unwrap();
    let encoded_bool_array = bincode::encode_to_vec(bool_array, config).unwrap();
    let encoded_signed_array = bincode::encode_to_vec(signed_array, config).unwrap();

    assert_eq!(
        bincode::encode_to_vec(OldSlice(&bools), config).unwrap(),
        encoded_bools
    );
    assert_eq!(
        bincode::encode_to_vec(OldSlice(&signed), config).unwrap(),
        encoded_signed
    );
    report_type_allocations(
        "bool",
        &bools,
        &bool_array,
        &encoded_bools,
        &encoded_bool_array,
    );
    report_type_allocations(
        "i8",
        &signed,
        &signed_array,
        &encoded_signed,
        &encoded_signed_array,
    );
    #[cfg(windows)]
    {
        report_type_cycles::<bool, ARRAY_COUNT>(
            "bool",
            &bools,
            &encoded_bools,
            &encoded_bool_array,
        );
        report_type_cycles::<i8, ARRAY_COUNT>(
            "i8",
            &signed,
            &encoded_signed,
            &encoded_signed_array,
        );
    }

    bench_encode(c, "batched_bool_slice_encode", &bools, encoded_bools.len());
    bench_reused_decode::<bool>(c, "batched_bool_vector_decode", &encoded_bools, bools.len());
    bench_array_decode::<bool, ARRAY_COUNT>(c, "batched_bool_array_decode", &encoded_bool_array);
    bench_encode(c, "batched_i8_slice_encode", &signed, encoded_signed.len());
    bench_reused_decode::<i8>(c, "batched_i8_vector_decode", &encoded_signed, signed.len());
    bench_array_decode::<i8, ARRAY_COUNT>(c, "batched_i8_array_decode", &encoded_signed_array);
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = ninth_pass_performance
}
criterion_main!(benches);
