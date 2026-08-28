use bincode::{
    config::{self, Config},
    de::{Decode, Decoder},
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::{HashMap, HashSet},
    hash::{BuildHasher, Hash},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

const ENTRY_COUNT: usize = 8_192;

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

// These are the reusable hash-collection implementations immediately before
// this pass. They retain buckets but drop every allocation owned by an entry.
fn old_decode_hash_map_into<K, V, S, C>(
    src: &[u8],
    map: &mut HashMap<K, V, S>,
    config: C,
) -> Result<(), bincode::error::DecodeError>
where
    K: Decode<()> + Eq + Hash,
    V: Decode<()>,
    S: BuildHasher,
    C: Config,
{
    map.clear();
    let reader = bincode::de::read::SliceReader::new(src);
    let mut decoder = bincode::de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    let encoded_len = u64::decode(&mut decoder)?;
    let len = usize::try_from(encoded_len)
        .map_err(|_| bincode::error::DecodeError::OutsideUsizeRange(encoded_len))?;
    decoder.claim_container_read::<(K, V)>(len)?;
    map.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<(K, V)>());
        map.insert(K::decode(&mut decoder)?, V::decode(&mut decoder)?);
    }
    Ok(())
}

fn old_decode_hash_set_into<T, S, C>(
    src: &[u8],
    set: &mut HashSet<T, S>,
    config: C,
) -> Result<(), bincode::error::DecodeError>
where
    T: Decode<()> + Eq + Hash,
    S: BuildHasher,
    C: Config,
{
    set.clear();
    let reader = bincode::de::read::SliceReader::new(src);
    let mut decoder = bincode::de::DecoderImpl::<_, C, ()>::new(reader, config, ());
    let encoded_len = u64::decode(&mut decoder)?;
    let len = usize::try_from(encoded_len)
        .map_err(|_| bincode::error::DecodeError::OutsideUsizeRange(encoded_len))?;
    decoder.claim_container_read::<T>(len)?;
    set.reserve(len);
    for _ in 0..len {
        decoder.unclaim_bytes_read(std::mem::size_of::<T>());
        set.insert(T::decode(&mut decoder)?);
    }
    Ok(())
}

fn string(index: usize) -> String {
    format!("chart-{index:05}-{}", "dead-sync".repeat(8))
}

fn string_key_map() -> HashMap<String, u64> {
    (0..ENTRY_COUNT)
        .map(|index| (string(index), (index as u64).wrapping_mul(1_000_003)))
        .collect()
}

fn string_value_map() -> HashMap<u64, String> {
    (0..ENTRY_COUNT)
        .map(|index| ((index as u64).wrapping_mul(1_000_003), string(index)))
        .collect()
}

fn string_set() -> HashSet<String> {
    (0..ENTRY_COUNT).map(string).collect()
}

fn assert_pool_churn(old: AllocStats, new: AllocStats, entry_count: usize) {
    assert!(old.allocs + old.zeroed_allocs >= entry_count as u64);
    assert!(old.deallocs >= entry_count as u64);
    assert_eq!((new.allocs + new.zeroed_allocs, new.reallocs), (1, 0));
    assert_eq!(new.deallocs, 1);
    assert!(new.allocated < old.allocated);
    assert!(new.deallocated < old.deallocated);
}

fn map_allocation_pair<K, V>(encoded: &[u8], expected: &HashMap<K, V>) -> (AllocStats, AllocStats)
where
    K: Decode<()> + Eq + Hash + std::fmt::Debug,
    V: Decode<()> + PartialEq + std::fmt::Debug,
{
    let config = config::standard();
    let mut old = HashMap::new();
    let mut new = HashMap::new();
    old_decode_hash_map_into(encoded, &mut old, config).unwrap();
    bincode::decode_from_slice_into_hash_map(encoded, &mut new, config).unwrap();
    let old_stats = allocations(|| old_decode_hash_map_into(encoded, &mut old, config).unwrap());
    let new_stats = allocations(|| {
        bincode::decode_from_slice_into_hash_map(encoded, &mut new, config).unwrap();
    });
    assert_eq!(&old, expected);
    assert_eq!(&new, expected);
    assert_pool_churn(old_stats, new_stats, expected.len());
    (old_stats, new_stats)
}

fn set_allocation_pair<T>(encoded: &[u8], expected: &HashSet<T>) -> (AllocStats, AllocStats)
where
    T: Decode<()> + Eq + Hash + std::fmt::Debug,
{
    let config = config::standard();
    let mut old = HashSet::new();
    let mut new = HashSet::new();
    old_decode_hash_set_into(encoded, &mut old, config).unwrap();
    bincode::decode_from_slice_into_hash_set(encoded, &mut new, config).unwrap();
    let old_stats = allocations(|| old_decode_hash_set_into(encoded, &mut old, config).unwrap());
    let new_stats = allocations(|| {
        bincode::decode_from_slice_into_hash_set(encoded, &mut new, config).unwrap();
    });
    assert_eq!(&old, expected);
    assert_eq!(&new, expected);
    assert_pool_churn(old_stats, new_stats, expected.len());
    (old_stats, new_stats)
}

fn report_allocations(
    encoded_keys: &[u8],
    keys: &HashMap<String, u64>,
    encoded_values: &[u8],
    values: &HashMap<u64, String>,
    encoded_set: &[u8],
    set: &HashSet<String>,
) {
    let key_stats = map_allocation_pair(encoded_keys, keys);
    let value_stats = map_allocation_pair(encoded_values, values);
    let set_stats = set_allocation_pair(encoded_set, set);
    eprintln!(
        "sixth-pass allocation profile per operation:\n\
         string map keys   old/new {key_stats:?}\n\
         string map values old/new {value_stats:?}\n\
         string set values old/new {set_stats:?}"
    );
}

#[cfg(windows)]
fn map_cycle_pair<K, V>(encoded: &[u8], iterations: u64) -> (u64, u64)
where
    K: Decode<()> + Eq + Hash,
    V: Decode<()>,
{
    let config = config::standard();
    let mut old: HashMap<K, V> = HashMap::new();
    let mut new: HashMap<K, V> = HashMap::new();
    old_decode_hash_map_into(encoded, &mut old, config).unwrap();
    bincode::decode_from_slice_into_hash_map(encoded, &mut new, config).unwrap();
    let old_cycles = cycles_per(iterations, || {
        old_decode_hash_map_into(black_box(encoded), black_box(&mut old), config).unwrap();
        black_box(old.len());
    });
    let new_cycles = cycles_per(iterations, || {
        bincode::decode_from_slice_into_hash_map(black_box(encoded), black_box(&mut new), config)
            .unwrap();
        black_box(new.len());
    });
    (old_cycles, new_cycles)
}

#[cfg(windows)]
fn set_cycle_pair<T>(encoded: &[u8], iterations: u64) -> (u64, u64)
where
    T: Decode<()> + Eq + Hash,
{
    let config = config::standard();
    let mut old: HashSet<T> = HashSet::new();
    let mut new: HashSet<T> = HashSet::new();
    old_decode_hash_set_into(encoded, &mut old, config).unwrap();
    bincode::decode_from_slice_into_hash_set(encoded, &mut new, config).unwrap();
    let old_cycles = cycles_per(iterations, || {
        old_decode_hash_set_into(black_box(encoded), black_box(&mut old), config).unwrap();
        black_box(old.len());
    });
    let new_cycles = cycles_per(iterations, || {
        bincode::decode_from_slice_into_hash_set(black_box(encoded), black_box(&mut new), config)
            .unwrap();
        black_box(new.len());
    });
    (old_cycles, new_cycles)
}

#[cfg(windows)]
fn report_cycles(encoded_keys: &[u8], encoded_values: &[u8], encoded_set: &[u8]) {
    let key_cycles = map_cycle_pair::<String, u64>(encoded_keys, 25);
    let value_cycles = map_cycle_pair::<u64, String>(encoded_values, 25);
    let set_cycles = set_cycle_pair::<String>(encoded_set, 25);
    eprintln!(
        "sixth-pass thread cycles per operation:\n\
         string map keys   old/new {key_cycles:?}\n\
         string map values old/new {value_cycles:?}\n\
         string set values old/new {set_cycles:?}"
    );
}

fn bench_map_pair<K, V>(
    c: &mut Criterion,
    group_name: &str,
    encoded: &[u8],
    mut old: HashMap<K, V>,
    mut new: HashMap<K, V>,
) where
    K: Decode<()> + Eq + Hash,
    V: Decode<()>,
{
    let config = config::standard();
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Bytes(encoded.len() as u64));
    group.bench_function(
        BenchmarkId::new("old_inner_allocating", encoded.len()),
        |b| {
            b.iter(|| {
                old_decode_hash_map_into(black_box(encoded), black_box(&mut old), config).unwrap();
                black_box(old.len());
            });
        },
    );
    group.bench_function(BenchmarkId::new("new_string_pool", encoded.len()), |b| {
        b.iter(|| {
            bincode::decode_from_slice_into_hash_map(
                black_box(encoded),
                black_box(&mut new),
                config,
            )
            .unwrap();
            black_box(new.len());
        });
    });
    group.finish();
}

fn bench_set_pair<T>(
    c: &mut Criterion,
    group_name: &str,
    encoded: &[u8],
    mut old: HashSet<T>,
    mut new: HashSet<T>,
) where
    T: Decode<()> + Eq + Hash,
{
    let config = config::standard();
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Bytes(encoded.len() as u64));
    group.bench_function(
        BenchmarkId::new("old_inner_allocating", encoded.len()),
        |b| {
            b.iter(|| {
                old_decode_hash_set_into(black_box(encoded), black_box(&mut old), config).unwrap();
                black_box(old.len());
            });
        },
    );
    group.bench_function(BenchmarkId::new("new_string_pool", encoded.len()), |b| {
        b.iter(|| {
            bincode::decode_from_slice_into_hash_set(
                black_box(encoded),
                black_box(&mut new),
                config,
            )
            .unwrap();
            black_box(new.len());
        });
    });
    group.finish();
}

fn sixth_pass_performance(c: &mut Criterion) {
    let config = config::standard();
    let keys = string_key_map();
    let values = string_value_map();
    let set = string_set();
    let encoded_keys = bincode::encode_to_vec(&keys, config).unwrap();
    let encoded_values = bincode::encode_to_vec(&values, config).unwrap();
    let encoded_set = bincode::encode_to_vec(&set, config).unwrap();

    report_allocations(
        &encoded_keys,
        &keys,
        &encoded_values,
        &values,
        &encoded_set,
        &set,
    );
    #[cfg(windows)]
    report_cycles(&encoded_keys, &encoded_values, &encoded_set);

    bench_map_pair(
        c,
        "deep_reuse_string_map_keys",
        &encoded_keys,
        keys.clone(),
        keys,
    );
    bench_map_pair(
        c,
        "deep_reuse_string_map_values",
        &encoded_values,
        values.clone(),
        values,
    );
    bench_set_pair(
        c,
        "deep_reuse_string_set_values",
        &encoded_set,
        set.clone(),
        set,
    );
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = sixth_pass_performance
}
criterion_main!(benches);
