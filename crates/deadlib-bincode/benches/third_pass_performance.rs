use bincode::config;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    hash::BuildHasherDefault,
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};

const VECTOR_ENTRIES: usize = 32_768;
const MAP_ENTRIES: usize = 32_768;
const SET_ENTRIES: usize = 65_536;

type BenchHasher = BuildHasherDefault<DefaultHasher>;
type OwnedMap = HashMap<String, u64, BenchHasher>;
type OwnedSet = HashSet<String, BenchHasher>;
type BorrowedMap<'a> = HashMap<&'a str, u64, BenchHasher>;
type BorrowedSet<'a> = HashSet<&'a str, BenchHasher>;

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

fn string_values(len: usize) -> Vec<String> {
    (0..len)
        .map(|index| format!("chart-{index:05}-dead-sync-persistence"))
        .collect()
}

fn map_values() -> OwnedMap {
    let mut values = OwnedMap::with_capacity_and_hasher(MAP_ENTRIES, BenchHasher::default());
    for index in 0..MAP_ENTRIES as u64 {
        values.insert(
            format!("profile-{index:05}-dead-sync"),
            index.rotate_left(17).wrapping_add(251),
        );
    }
    values
}

fn set_values() -> OwnedSet {
    let mut values = OwnedSet::with_capacity_and_hasher(SET_ENTRIES, BenchHasher::default());
    for index in 0..SET_ENTRIES as u64 {
        values.insert(format!("chart-{index:05}-dead-sync-score"));
    }
    values
}

fn report_allocations(vector: &[u8], map: &[u8], set: &[u8]) {
    let config = config::standard();
    let mut reused_vector = Vec::<&str>::with_capacity(VECTOR_ENTRIES);
    let mut reused_map = BorrowedMap::with_capacity_and_hasher(MAP_ENTRIES, BenchHasher::default());
    let mut reused_set = BorrowedSet::with_capacity_and_hasher(SET_ENTRIES, BenchHasher::default());

    bincode::borrow_decode_from_slice_into_vec(vector, &mut reused_vector, config).unwrap();
    bincode::borrow_decode_from_slice_into_hash_map(map, &mut reused_map, config).unwrap();
    bincode::borrow_decode_from_slice_into_hash_set(set, &mut reused_set, config).unwrap();

    let old_vector = allocations(|| {
        drop(bincode::borrow_decode_from_slice::<Vec<&str>, _>(vector, config).unwrap());
    });
    let new_vector = allocations(|| {
        bincode::borrow_decode_from_slice_into_vec(vector, &mut reused_vector, config).unwrap();
    });
    let old_map = allocations(|| {
        drop(bincode::borrow_decode_from_slice::<BorrowedMap<'_>, _>(map, config).unwrap());
    });
    let new_map = allocations(|| {
        bincode::borrow_decode_from_slice_into_hash_map(map, &mut reused_map, config).unwrap();
    });
    let old_set = allocations(|| {
        drop(bincode::borrow_decode_from_slice::<BorrowedSet<'_>, _>(set, config).unwrap());
    });
    let new_set = allocations(|| {
        bincode::borrow_decode_from_slice_into_hash_set(set, &mut reused_set, config).unwrap();
    });

    for stats in [old_vector, old_map, old_set] {
        assert!(stats.allocs + stats.zeroed_allocs > 0);
    }
    for stats in [new_vector, new_map, new_set] {
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
        "third-pass allocation profile per operation:\n\
         allocating borrowed vector {old_vector:?}\n\
         reused borrowed vector     {new_vector:?}\n\
         allocating borrowed map    {old_map:?}\n\
         reused borrowed map        {new_map:?}\n\
         allocating borrowed set    {old_set:?}\n\
         reused borrowed set        {new_set:?}"
    );
}

#[cfg(windows)]
fn report_cycles(vector: &[u8], map: &[u8], set: &[u8]) {
    const ITERATIONS: u64 = 200;
    let config = config::standard();
    let mut reused_vector = Vec::<&str>::with_capacity(VECTOR_ENTRIES);
    let mut reused_map = BorrowedMap::with_capacity_and_hasher(MAP_ENTRIES, BenchHasher::default());
    let mut reused_set = BorrowedSet::with_capacity_and_hasher(SET_ENTRIES, BenchHasher::default());

    let old_vector = cycles_per(ITERATIONS, || {
        black_box(bincode::borrow_decode_from_slice::<Vec<&str>, _>(vector, config).unwrap());
    });
    let new_vector = cycles_per(ITERATIONS, || {
        bincode::borrow_decode_from_slice_into_vec(vector, black_box(&mut reused_vector), config)
            .unwrap();
        black_box(reused_vector.len());
    });
    let old_map = cycles_per(ITERATIONS, || {
        black_box(bincode::borrow_decode_from_slice::<BorrowedMap<'_>, _>(map, config).unwrap());
    });
    let new_map = cycles_per(ITERATIONS, || {
        bincode::borrow_decode_from_slice_into_hash_map(map, black_box(&mut reused_map), config)
            .unwrap();
        black_box(reused_map.len());
    });
    let old_set = cycles_per(ITERATIONS, || {
        black_box(bincode::borrow_decode_from_slice::<BorrowedSet<'_>, _>(set, config).unwrap());
    });
    let new_set = cycles_per(ITERATIONS, || {
        bincode::borrow_decode_from_slice_into_hash_set(set, black_box(&mut reused_set), config)
            .unwrap();
        black_box(reused_set.len());
    });
    eprintln!(
        "third-pass thread cycles per operation:\n\
         allocating borrowed vector {old_vector}\n\
         reused borrowed vector     {new_vector}\n\
         allocating borrowed map    {old_map}\n\
         reused borrowed map        {new_map}\n\
         allocating borrowed set    {old_set}\n\
         reused borrowed set        {new_set}"
    );
}

fn third_pass_performance(c: &mut Criterion) {
    let config = config::standard();
    let vector_values = string_values(VECTOR_ENTRIES);
    let map_values = map_values();
    let set_values = set_values();
    let vector = bincode::encode_to_vec(&vector_values, config).unwrap();
    let map = bincode::encode_to_vec(&map_values, config).unwrap();
    let set = bincode::encode_to_vec(&set_values, config).unwrap();

    let mut reused_vector = Vec::<&str>::with_capacity(vector_values.len());
    let mut reused_map =
        BorrowedMap::with_capacity_and_hasher(map_values.len(), BenchHasher::default());
    let mut reused_set =
        BorrowedSet::with_capacity_and_hasher(set_values.len(), BenchHasher::default());
    assert_eq!(
        bincode::borrow_decode_from_slice_into_vec(&vector, &mut reused_vector, config).unwrap(),
        vector.len()
    );
    assert!(reused_vector
        .iter()
        .zip(&vector_values)
        .all(|(decoded, expected)| *decoded == expected));
    assert_eq!(
        bincode::borrow_decode_from_slice_into_hash_map(&map, &mut reused_map, config).unwrap(),
        map.len()
    );
    assert!(map_values
        .iter()
        .all(|(key, value)| reused_map.get(key.as_str()) == Some(value)));
    assert_eq!(
        bincode::borrow_decode_from_slice_into_hash_set(&set, &mut reused_set, config).unwrap(),
        set.len()
    );
    assert!(set_values
        .iter()
        .all(|value| reused_set.contains(value.as_str())));

    report_allocations(&vector, &map, &set);
    #[cfg(windows)]
    report_cycles(&vector, &map, &set);

    let mut vectors = c.benchmark_group("reusable_borrowed_vector_decode");
    vectors.throughput(Throughput::Bytes(vector.len() as u64));
    vectors.bench_function(BenchmarkId::new("old_allocating", vector.len()), |b| {
        b.iter(|| {
            black_box(
                bincode::borrow_decode_from_slice::<Vec<&str>, _>(black_box(&vector), config)
                    .unwrap(),
            );
        });
    });
    vectors.bench_function(BenchmarkId::new("new_reused", vector.len()), |b| {
        b.iter(|| {
            bincode::borrow_decode_from_slice_into_vec(
                black_box(&vector),
                black_box(&mut reused_vector),
                config,
            )
            .unwrap();
            black_box(reused_vector.len());
        });
    });
    vectors.finish();

    let mut maps = c.benchmark_group("reusable_borrowed_hash_map_decode");
    maps.throughput(Throughput::Bytes(map.len() as u64));
    maps.bench_function(BenchmarkId::new("old_allocating", map.len()), |b| {
        b.iter(|| {
            black_box(
                bincode::borrow_decode_from_slice::<BorrowedMap<'_>, _>(black_box(&map), config)
                    .unwrap(),
            );
        });
    });
    maps.bench_function(BenchmarkId::new("new_reused", map.len()), |b| {
        b.iter(|| {
            bincode::borrow_decode_from_slice_into_hash_map(
                black_box(&map),
                black_box(&mut reused_map),
                config,
            )
            .unwrap();
            black_box(reused_map.len());
        });
    });
    maps.finish();

    let mut sets = c.benchmark_group("reusable_borrowed_hash_set_decode");
    sets.throughput(Throughput::Bytes(set.len() as u64));
    sets.bench_function(BenchmarkId::new("old_allocating", set.len()), |b| {
        b.iter(|| {
            black_box(
                bincode::borrow_decode_from_slice::<BorrowedSet<'_>, _>(black_box(&set), config)
                    .unwrap(),
            );
        });
    });
    sets.bench_function(BenchmarkId::new("new_reused", set.len()), |b| {
        b.iter(|| {
            bincode::borrow_decode_from_slice_into_hash_set(
                black_box(&set),
                black_box(&mut reused_set),
                config,
            )
            .unwrap();
            black_box(reused_set.len());
        });
    });
    sets.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(30)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets = third_pass_performance
}
criterion_main!(benches);
