use bincode::{
    config::{self, Config},
    de::Decoder,
    enc::write::Writer,
    Decode, Encode,
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

struct CountingAlloc;

static TRACK_ALLOC: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static DEALLOCS: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static ALLOCATOR: CountingAlloc = CountingAlloc;

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
            ALLOCS.fetch_add(1, Ordering::Relaxed);
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
    reallocs: u64,
    deallocs: u64,
    allocated: u64,
    deallocated: u64,
}

fn allocations(run: impl FnOnce()) -> AllocStats {
    ALLOCS.store(0, Ordering::Relaxed);
    REALLOCS.store(0, Ordering::Relaxed);
    DEALLOCS.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
    DEALLOC_BYTES.store(0, Ordering::Relaxed);
    TRACK_ALLOC.store(true, Ordering::SeqCst);
    run();
    TRACK_ALLOC.store(false, Ordering::SeqCst);
    AllocStats {
        allocs: ALLOCS.load(Ordering::Relaxed),
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

struct OldVecWriter {
    inner: Vec<u8>,
}

impl Writer for OldVecWriter {
    fn write(&mut self, bytes: &[u8]) -> Result<(), bincode::error::EncodeError> {
        self.inner.extend_from_slice(bytes);
        Ok(())
    }
}

fn old_encode_to_vec<E: Encode, C: Config>(
    value: E,
    config: C,
) -> Result<Vec<u8>, bincode::error::EncodeError> {
    let mut encoder =
        bincode::enc::EncoderImpl::<_, C>::new(bincode::enc::write::SizeWriter::default(), config);
    value.encode(&mut encoder)?;
    let size = encoder.into_writer().bytes_written;
    let writer = OldVecWriter {
        inner: Vec::with_capacity(size),
    };
    let mut encoder = bincode::enc::EncoderImpl::<_, C>::new(writer, config);
    value.encode(&mut encoder)?;
    Ok(encoder.into_writer().inner)
}

struct OldFloats<'a>(&'a [f32]);

impl Encode for OldFloats<'_> {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        (self.0.len() as u64).encode(encoder)?;
        for value in self.0 {
            value.encode(encoder)?;
        }
        Ok(())
    }
}

fn old_decode_floats(src: &[u8]) -> Vec<f32> {
    let config = config::standard();
    let reader = bincode::de::read::SliceReader::new(src);
    let mut decoder = bincode::de::DecoderImpl::new(reader, config, ());
    let len = usize::try_from(u64::decode(&mut decoder).unwrap()).unwrap();
    decoder.claim_container_read::<f32>(len).unwrap();
    let mut values = Vec::with_capacity(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<f32>());
        values.push(f32::decode(&mut decoder).unwrap());
    }
    values
}

#[derive(Encode)]
struct Payload {
    names: Vec<String>,
    samples: Vec<u64>,
}

fn payload() -> Payload {
    Payload {
        names: (0..2_048)
            .map(|index| format!("chart-{index:04}-{}", "dead-sync".repeat(8)))
            .collect(),
        samples: (0..65_536).map(|value| value * 1_000_003).collect(),
    }
}

fn report_allocations(data: &Payload, text: &[u8], floats: &[u8]) {
    let config = config::standard();
    let mut reused = bincode::encode_to_vec(data, config).unwrap();
    let old_encode = allocations(|| drop(old_encode_to_vec(data, config).unwrap()));
    let reused_encode =
        allocations(|| bincode::encode_into_vec(data, &mut reused, config).unwrap());
    let owned_text =
        allocations(|| drop(bincode::decode_from_slice::<String, _>(text, config).unwrap()));
    let borrowed_text = allocations(|| {
        black_box(bincode::borrow_decode_from_slice::<&str, _>(text, config).unwrap());
    });
    let old_floats = allocations(|| drop(old_decode_floats(floats)));
    let batch_floats =
        allocations(|| drop(bincode::decode_from_slice::<Vec<f32>, _>(floats, config).unwrap()));

    assert!(old_encode.allocs > 0);
    assert_eq!((reused_encode.allocs, reused_encode.reallocs), (0, 0));
    assert!(owned_text.allocs > 0);
    assert_eq!((borrowed_text.allocs, borrowed_text.reallocs), (0, 0));
    assert_eq!(old_floats.allocated, batch_floats.allocated);
    eprintln!(
        "allocation profile per operation:\n\
         old encode       {old_encode:?}\n\
         reused encode    {reused_encode:?}\n\
         owned text       {owned_text:?}\n\
         borrowed text    {borrowed_text:?}\n\
         scalar floats    {old_floats:?}\n\
         batched floats   {batch_floats:?}"
    );
    black_box((
        old_encode.deallocs,
        old_encode.deallocated,
        reused_encode.deallocs,
        reused_encode.deallocated,
    ));
}

#[cfg(windows)]
fn report_cycles(data: &Payload, text: &[u8], values: &[f32], floats: &[u8]) {
    const ITERATIONS: u64 = 100;
    let config = config::standard();
    let mut reused = bincode::encode_to_vec(data, config).unwrap();
    let old_encode = cycles_per(ITERATIONS, || {
        black_box(old_encode_to_vec(black_box(data), config).unwrap());
    });
    let reused_encode = cycles_per(ITERATIONS, || {
        bincode::encode_into_vec(black_box(data), black_box(&mut reused), config).unwrap();
        black_box(reused.len());
    });
    let owned_text = cycles_per(ITERATIONS, || {
        black_box(bincode::decode_from_slice::<String, _>(black_box(text), config).unwrap());
    });
    let borrowed_text = cycles_per(ITERATIONS, || {
        black_box(bincode::borrow_decode_from_slice::<&str, _>(black_box(text), config).unwrap());
    });
    let scalar_float_encode = cycles_per(ITERATIONS, || {
        black_box(old_encode_to_vec(OldFloats(black_box(values)), config).unwrap());
    });
    let batch_float_encode = cycles_per(ITERATIONS, || {
        black_box(bincode::encode_to_vec(black_box(values), config).unwrap());
    });
    let scalar_float_decode = cycles_per(ITERATIONS, || {
        black_box(old_decode_floats(black_box(floats)));
    });
    let batch_float_decode = cycles_per(ITERATIONS, || {
        black_box(bincode::decode_from_slice::<Vec<f32>, _>(black_box(floats), config).unwrap());
    });
    eprintln!(
        "thread cycles per operation:\n\
         old encode          {old_encode}\n\
         reused encode       {reused_encode}\n\
         owned text          {owned_text}\n\
         borrowed text       {borrowed_text}\n\
         scalar float encode {scalar_float_encode}\n\
         batch float encode  {batch_float_encode}\n\
         scalar float decode {scalar_float_decode}\n\
         batch float decode  {batch_float_decode}"
    );
}

fn performance(c: &mut Criterion) {
    const PAYLOAD_BYTES: usize = 1 << 20;
    let config = config::standard();
    let data = payload();
    let text_value = "DeadSync".repeat(PAYLOAD_BYTES / 8);
    let text = bincode::encode_to_vec(&text_value, config).unwrap();
    let float_value = (0..PAYLOAD_BYTES / 4)
        .map(|value| value as f32 * 0.125 - 16_384.0)
        .collect::<Vec<_>>();
    let floats = bincode::encode_to_vec(&float_value, config).unwrap();
    report_allocations(&data, &text, &floats);
    #[cfg(windows)]
    report_cycles(&data, &text, &float_value, &floats);

    let encoded_size = bincode::encode_to_vec(&data, config).unwrap().len();
    let mut reused = Vec::with_capacity(encoded_size);
    let mut encode = c.benchmark_group("reusable_encode");
    encode.throughput(Throughput::Bytes(encoded_size as u64));
    encode.bench_function(BenchmarkId::new("old_allocating", encoded_size), |b| {
        b.iter(|| drop(old_encode_to_vec(black_box(&data), config).unwrap()));
    });
    encode.bench_function(BenchmarkId::new("new_reused", encoded_size), |b| {
        b.iter(|| {
            bincode::encode_into_vec(black_box(&data), black_box(&mut reused), config).unwrap();
            black_box(reused.len());
        });
    });
    encode.finish();

    let mut borrow = c.benchmark_group("borrowed_string_decode");
    borrow.throughput(Throughput::Bytes(text.len() as u64));
    borrow.bench_function(BenchmarkId::new("old_owned", text.len()), |b| {
        b.iter(|| {
            let decoded =
                bincode::decode_from_slice::<String, _>(black_box(&text), config).unwrap();
            black_box(decoded);
        });
    });
    borrow.bench_function(BenchmarkId::new("new_borrowed", text.len()), |b| {
        b.iter(|| {
            let decoded =
                bincode::borrow_decode_from_slice::<&str, _>(black_box(&text), config).unwrap();
            black_box(decoded);
        });
    });
    borrow.finish();

    let mut numeric_encode = c.benchmark_group("numeric_slice_encode");
    numeric_encode.throughput(Throughput::Bytes(floats.len() as u64));
    numeric_encode.bench_function(BenchmarkId::new("old_scalar", floats.len()), |b| {
        b.iter(|| {
            black_box(old_encode_to_vec(OldFloats(black_box(&float_value)), config).unwrap())
        });
    });
    numeric_encode.bench_function(BenchmarkId::new("new_batched", floats.len()), |b| {
        b.iter(|| black_box(bincode::encode_to_vec(black_box(&float_value), config).unwrap()));
    });
    numeric_encode.finish();

    let mut numeric_decode = c.benchmark_group("numeric_vec_decode");
    numeric_decode.throughput(Throughput::Bytes(floats.len() as u64));
    numeric_decode.bench_function(BenchmarkId::new("old_scalar", floats.len()), |b| {
        b.iter(|| black_box(old_decode_floats(black_box(&floats))));
    });
    numeric_decode.bench_function(BenchmarkId::new("new_batched", floats.len()), |b| {
        b.iter(|| {
            let decoded =
                bincode::decode_from_slice::<Vec<f32>, _>(black_box(&floats), config).unwrap();
            black_box(decoded);
        });
    });
    numeric_decode.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = performance
}
criterion_main!(benches);
