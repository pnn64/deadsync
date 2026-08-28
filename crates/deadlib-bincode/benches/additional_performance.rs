use bincode::{
    config::{self, Config, Endianness},
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

struct OldVarintVec(Vec<u64>);

impl<Context> Decode<Context> for OldVarintVec {
    fn decode<D: Decoder<Context = Context>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let encoded_len = u64::decode(decoder)?;
        let len = usize::try_from(encoded_len)
            .map_err(|_| bincode::error::DecodeError::OutsideUsizeRange(encoded_len))?;
        decoder.claim_container_read::<u64>(len)?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            decoder.unclaim_bytes_read(std::mem::size_of::<u64>());
            values.push(u64::decode(decoder)?);
        }
        Ok(Self(values))
    }
}

const SINGLE_BYTE_MAX: u64 = 250;
const U16_BYTE: u8 = 251;
const U32_BYTE: u8 = 252;
const U64_BYTE: u8 = 253;

fn old_varint_encode_u64<W: Writer>(
    writer: &mut W,
    endian: Endianness,
    value: u64,
) -> Result<(), bincode::error::EncodeError> {
    if value <= SINGLE_BYTE_MAX {
        writer.write(&[value as u8])
    } else if value <= u64::from(u16::MAX) {
        writer.write(&[U16_BYTE])?;
        match endian {
            Endianness::Big => writer.write(&(value as u16).to_be_bytes()),
            Endianness::Little => writer.write(&(value as u16).to_le_bytes()),
            _ => unreachable!("benchmark only supports bincode's known endian configurations"),
        }
    } else if value <= u64::from(u32::MAX) {
        writer.write(&[U32_BYTE])?;
        match endian {
            Endianness::Big => writer.write(&(value as u32).to_be_bytes()),
            Endianness::Little => writer.write(&(value as u32).to_le_bytes()),
            _ => unreachable!("benchmark only supports bincode's known endian configurations"),
        }
    } else {
        writer.write(&[U64_BYTE])?;
        match endian {
            Endianness::Big => writer.write(&value.to_be_bytes()),
            Endianness::Little => writer.write(&value.to_le_bytes()),
            _ => unreachable!("benchmark only supports bincode's known endian configurations"),
        }
    }
}

struct OldVarints<'a>(&'a [u64]);

impl Encode for OldVarints<'_> {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), bincode::error::EncodeError> {
        (self.0.len() as u64).encode(encoder)?;
        for &value in self.0 {
            let endian = encoder.config().endianness();
            old_varint_encode_u64(encoder.writer(), endian, value)?;
        }
        Ok(())
    }
}

fn varints() -> Vec<u64> {
    (0..131_072u64)
        .map(|index| match index & 3 {
            0 => index % 251,
            1 => 251 + index % (u64::from(u16::MAX) - 251),
            2 => u64::from(u16::MAX) + 1 + index,
            _ => u64::from(u32::MAX) + 1 + index * 1_000_003,
        })
        .collect()
}

fn report_allocations(encoded_varints: &[u8], values: &[u64]) {
    let config = config::standard();
    let old_vec = allocations(|| {
        drop(bincode::decode_from_slice::<Vec<u64>, _>(encoded_varints, config).unwrap());
    });
    let old_scalar_varint = allocations(|| {
        let (decoded, _) =
            bincode::decode_from_slice::<OldVarintVec, _>(encoded_varints, config).unwrap();
        black_box(decoded.0.len());
    });
    let mut reused = Vec::<u64>::with_capacity(values.len());
    let reused_vec = allocations(|| {
        bincode::decode_from_slice_into_vec(encoded_varints, &mut reused, config).unwrap();
    });
    let mut old_encoded = bincode::encode_to_vec(OldVarints(values), config).unwrap();
    let mut new_encoded = bincode::encode_to_vec(values, config).unwrap();
    let old_varint = allocations(|| {
        bincode::encode_into_vec(OldVarints(values), &mut old_encoded, config).unwrap();
    });
    let new_varint = allocations(|| {
        bincode::encode_into_vec(values, &mut new_encoded, config).unwrap();
    });

    assert_eq!(old_vec.allocs + old_vec.zeroed_allocs, 1);
    assert_eq!(old_scalar_varint.allocs, 1);
    assert_eq!(
        (
            reused_vec.allocs,
            reused_vec.zeroed_allocs,
            reused_vec.reallocs
        ),
        (0, 0, 0)
    );
    assert_eq!((old_varint.allocs, old_varint.reallocs), (0, 0));
    assert_eq!((new_varint.allocs, new_varint.reallocs), (0, 0));
    eprintln!(
        "additional allocation profile per operation:\n\
         old vec decode       {old_vec:?}\n\
         reused vec decode    {reused_vec:?}\n\
         scalar varint decode {old_scalar_varint:?}\n\
         two-write varints    {old_varint:?}\n\
         one-write varints    {new_varint:?}"
    );
    black_box((
        old_vec.deallocs,
        old_vec.allocated,
        old_vec.deallocated,
        reused_vec.deallocs,
        reused_vec.allocated,
        reused_vec.deallocated,
    ));
}

#[cfg(windows)]
fn report_cycles(encoded_varints: &[u8], values: &[u64]) {
    const ITERATIONS: u64 = 100;
    let config = config::standard();
    let mut reused = Vec::<u64>::with_capacity(values.len());
    let mut old_encoded = bincode::encode_to_vec(OldVarints(values), config).unwrap();
    let mut new_encoded = bincode::encode_to_vec(values, config).unwrap();
    let old_vec = cycles_per(ITERATIONS, || {
        black_box(bincode::decode_from_slice::<Vec<u64>, _>(encoded_varints, config).unwrap());
    });
    let old_scalar_varint = cycles_per(ITERATIONS, || {
        black_box(bincode::decode_from_slice::<OldVarintVec, _>(encoded_varints, config).unwrap());
    });
    let reused_vec = cycles_per(ITERATIONS, || {
        bincode::decode_from_slice_into_vec(encoded_varints, black_box(&mut reused), config)
            .unwrap();
        black_box(reused.len());
    });
    let old_varint = cycles_per(ITERATIONS, || {
        bincode::encode_into_vec(OldVarints(values), black_box(&mut old_encoded), config).unwrap();
        black_box(old_encoded.len());
    });
    let new_varint = cycles_per(ITERATIONS, || {
        bincode::encode_into_vec(values, black_box(&mut new_encoded), config).unwrap();
        black_box(new_encoded.len());
    });
    eprintln!(
        "additional thread cycles per operation:\n\
         old vec decode       {old_vec}\n\
         reused vec decode    {reused_vec}\n\
         scalar varint decode {old_scalar_varint}\n\
         two-write varints    {old_varint}\n\
         one-write varints    {new_varint}"
    );
}

fn additional_performance(c: &mut Criterion) {
    let config = config::standard();
    let values = varints();
    let encoded_varints = bincode::encode_to_vec(&values, config).unwrap();
    assert_eq!(
        bincode::encode_to_vec(OldVarints(&values), config).unwrap(),
        encoded_varints
    );
    report_allocations(&encoded_varints, &values);
    #[cfg(windows)]
    report_cycles(&encoded_varints, &values);

    let mut reused = Vec::<u64>::with_capacity(values.len());
    let mut decode = c.benchmark_group("reusable_vec_decode");
    decode.throughput(Throughput::Bytes(encoded_varints.len() as u64));
    decode.bench_function(
        BenchmarkId::new("old_allocating", encoded_varints.len()),
        |b| {
            b.iter(|| {
                black_box(
                    bincode::decode_from_slice::<Vec<u64>, _>(black_box(&encoded_varints), config)
                        .unwrap(),
                );
            });
        },
    );
    decode.bench_function(BenchmarkId::new("new_reused", encoded_varints.len()), |b| {
        b.iter(|| {
            bincode::decode_from_slice_into_vec(
                black_box(&encoded_varints),
                black_box(&mut reused),
                config,
            )
            .unwrap();
            black_box(reused.len());
        });
    });
    decode.finish();

    let mut varint_decode = c.benchmark_group("specialized_varint_decode");
    varint_decode.throughput(Throughput::Bytes(encoded_varints.len() as u64));
    varint_decode.bench_function(BenchmarkId::new("old_scalar", encoded_varints.len()), |b| {
        b.iter(|| {
            black_box(
                bincode::decode_from_slice::<OldVarintVec, _>(black_box(&encoded_varints), config)
                    .unwrap(),
            );
        });
    });
    varint_decode.bench_function(
        BenchmarkId::new("new_specialized", encoded_varints.len()),
        |b| {
            b.iter(|| {
                black_box(
                    bincode::decode_from_slice::<Vec<u64>, _>(black_box(&encoded_varints), config)
                        .unwrap(),
                );
            });
        },
    );
    varint_decode.finish();

    let mut old_encoded = Vec::with_capacity(encoded_varints.len());
    let mut new_encoded = Vec::with_capacity(encoded_varints.len());
    let mut varint = c.benchmark_group("coalesced_varint_encode");
    varint.throughput(Throughput::Bytes(encoded_varints.len() as u64));
    varint.bench_function(
        BenchmarkId::new("old_two_writes", encoded_varints.len()),
        |b| {
            b.iter(|| {
                bincode::encode_into_vec(
                    OldVarints(black_box(&values)),
                    black_box(&mut old_encoded),
                    config,
                )
                .unwrap();
                black_box(old_encoded.len());
            });
        },
    );
    varint.bench_function(
        BenchmarkId::new("new_one_write", encoded_varints.len()),
        |b| {
            b.iter(|| {
                bincode::encode_into_vec(black_box(&values), black_box(&mut new_encoded), config)
                    .unwrap();
                black_box(new_encoded.len());
            });
        },
    );
    varint.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = additional_performance
}
criterion_main!(benches);
