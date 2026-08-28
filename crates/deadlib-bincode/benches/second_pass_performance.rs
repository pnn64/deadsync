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

const TEXT_BYTES: usize = 1 << 20;
const MAP_ENTRIES: usize = 65_536;
const SET_ENTRIES: usize = 131_072;

type BenchHasher = BuildHasherDefault<DefaultHasher>;
type BenchMap = HashMap<u64, u64, BenchHasher>;
type BenchSet = HashSet<u64, BenchHasher>;

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

fn map_values() -> BenchMap {
    let mut values = BenchMap::with_capacity_and_hasher(MAP_ENTRIES, BenchHasher::default());
    for index in 0..MAP_ENTRIES as u64 {
        values.insert(
            index.wrapping_mul(1_000_003),
            index.rotate_left(17).wrapping_add(251),
        );
    }
    values
}

fn set_values() -> BenchSet {
    let mut values = BenchSet::with_capacity_and_hasher(SET_ENTRIES, BenchHasher::default());
    for index in 0..SET_ENTRIES as u64 {
        values.insert(
            index
                .wrapping_mul(1_000_003)
                .rotate_left((index & 63) as u32),
        );
    }
    values
}

fn report_allocations(text: &[u8], map: &[u8], set: &[u8]) {
    let config = config::standard();
    let mut reused_text = String::with_capacity(TEXT_BYTES);
    let mut reused_map = BenchMap::with_capacity_and_hasher(MAP_ENTRIES, BenchHasher::default());
    let mut reused_set = BenchSet::with_capacity_and_hasher(SET_ENTRIES, BenchHasher::default());

    bincode::decode_from_slice_into_string(text, &mut reused_text, config).unwrap();
    bincode::decode_from_slice_into_hash_map(map, &mut reused_map, config).unwrap();
    bincode::decode_from_slice_into_hash_set(set, &mut reused_set, config).unwrap();

    let old_text = allocations(|| {
        drop(bincode::decode_from_slice::<String, _>(text, config).unwrap());
    });
    let new_text = allocations(|| {
        bincode::decode_from_slice_into_string(text, &mut reused_text, config).unwrap();
    });
    let old_map = allocations(|| {
        drop(bincode::decode_from_slice::<BenchMap, _>(map, config).unwrap());
    });
    let new_map = allocations(|| {
        bincode::decode_from_slice_into_hash_map(map, &mut reused_map, config).unwrap();
    });
    let old_set = allocations(|| {
        drop(bincode::decode_from_slice::<BenchSet, _>(set, config).unwrap());
    });
    let new_set = allocations(|| {
        bincode::decode_from_slice_into_hash_set(set, &mut reused_set, config).unwrap();
    });

    assert!(old_text.allocs + old_text.zeroed_allocs > 0);
    assert!(old_map.allocs + old_map.zeroed_allocs > 0);
    assert!(old_set.allocs + old_set.zeroed_allocs > 0);
    for stats in [new_text, new_map, new_set] {
        assert_eq!(
            (
                stats.allocs,
                stats.zeroed_allocs,
                stats.reallocs,
                stats.deallocs
            ),
            (0, 0, 0, 0)
        );
        assert_eq!((stats.allocated, stats.deallocated), (0, 0));
    }
    eprintln!(
        "second-pass allocation profile per operation:\n\
         allocating string {old_text:?}\n\
         reused string     {new_text:?}\n\
         allocating map    {old_map:?}\n\
         reused map        {new_map:?}\n\
         allocating set    {old_set:?}\n\
         reused set        {new_set:?}"
    );
    black_box((
        old_text.deallocs,
        old_text.allocated,
        old_text.deallocated,
        old_map.deallocs,
        old_map.allocated,
        old_map.deallocated,
        old_set.deallocs,
        old_set.allocated,
        old_set.deallocated,
    ));
}

#[cfg(windows)]
fn report_cycles(text: &[u8], map: &[u8], set: &[u8]) {
    const ITERATIONS: u64 = 50;
    let config = config::standard();
    let mut reused_text = String::with_capacity(TEXT_BYTES);
    let mut reused_map = BenchMap::with_capacity_and_hasher(MAP_ENTRIES, BenchHasher::default());
    let mut reused_set = BenchSet::with_capacity_and_hasher(SET_ENTRIES, BenchHasher::default());
    let old_text = cycles_per(ITERATIONS, || {
        black_box(bincode::decode_from_slice::<String, _>(text, config).unwrap());
    });
    let new_text = cycles_per(ITERATIONS, || {
        bincode::decode_from_slice_into_string(text, black_box(&mut reused_text), config).unwrap();
        black_box(reused_text.len());
    });
    let old_map = cycles_per(ITERATIONS, || {
        black_box(bincode::decode_from_slice::<BenchMap, _>(map, config).unwrap());
    });
    let new_map = cycles_per(ITERATIONS, || {
        bincode::decode_from_slice_into_hash_map(map, black_box(&mut reused_map), config).unwrap();
        black_box(reused_map.len());
    });
    let old_set = cycles_per(ITERATIONS, || {
        black_box(bincode::decode_from_slice::<BenchSet, _>(set, config).unwrap());
    });
    let new_set = cycles_per(ITERATIONS, || {
        bincode::decode_from_slice_into_hash_set(set, black_box(&mut reused_set), config).unwrap();
        black_box(reused_set.len());
    });
    eprintln!(
        "second-pass thread cycles per operation:\n\
         allocating string {old_text}\n\
         reused string     {new_text}\n\
         allocating map    {old_map}\n\
         reused map        {new_map}\n\
         allocating set    {old_set}\n\
         reused set        {new_set}"
    );
}

fn second_pass_performance(c: &mut Criterion) {
    let config = config::standard();
    let text_value = "DeadSync".repeat(TEXT_BYTES / 8);
    let map_value = map_values();
    let set_value = set_values();
    let text = bincode::encode_to_vec(&text_value, config).unwrap();
    let map = bincode::encode_to_vec(&map_value, config).unwrap();
    let set = bincode::encode_to_vec(&set_value, config).unwrap();

    let mut reused_text = String::with_capacity(text_value.len());
    let mut reused_map =
        BenchMap::with_capacity_and_hasher(map_value.len(), BenchHasher::default());
    let mut reused_set =
        BenchSet::with_capacity_and_hasher(set_value.len(), BenchHasher::default());
    assert_eq!(
        bincode::decode_from_slice_into_string(&text, &mut reused_text, config).unwrap(),
        text.len()
    );
    assert_eq!(reused_text, text_value);
    assert_eq!(
        bincode::decode_from_slice_into_hash_map(&map, &mut reused_map, config).unwrap(),
        map.len()
    );
    assert_eq!(reused_map, map_value);
    assert_eq!(
        bincode::decode_from_slice_into_hash_set(&set, &mut reused_set, config).unwrap(),
        set.len()
    );
    assert_eq!(reused_set, set_value);

    report_allocations(&text, &map, &set);
    #[cfg(windows)]
    report_cycles(&text, &map, &set);

    let mut strings = c.benchmark_group("reusable_string_decode");
    strings.throughput(Throughput::Bytes(text.len() as u64));
    strings.bench_function(BenchmarkId::new("old_allocating", text.len()), |b| {
        b.iter(|| {
            black_box(bincode::decode_from_slice::<String, _>(black_box(&text), config).unwrap());
        });
    });
    strings.bench_function(BenchmarkId::new("new_reused", text.len()), |b| {
        b.iter(|| {
            bincode::decode_from_slice_into_string(
                black_box(&text),
                black_box(&mut reused_text),
                config,
            )
            .unwrap();
            black_box(reused_text.len());
        });
    });
    strings.finish();

    let mut maps = c.benchmark_group("reusable_hash_map_decode");
    maps.throughput(Throughput::Bytes(map.len() as u64));
    maps.bench_function(BenchmarkId::new("old_allocating", map.len()), |b| {
        b.iter(|| {
            black_box(bincode::decode_from_slice::<BenchMap, _>(black_box(&map), config).unwrap());
        });
    });
    maps.bench_function(BenchmarkId::new("new_reused", map.len()), |b| {
        b.iter(|| {
            bincode::decode_from_slice_into_hash_map(
                black_box(&map),
                black_box(&mut reused_map),
                config,
            )
            .unwrap();
            black_box(reused_map.len());
        });
    });
    maps.finish();

    let mut sets = c.benchmark_group("reusable_hash_set_decode");
    sets.throughput(Throughput::Bytes(set.len() as u64));
    sets.bench_function(BenchmarkId::new("old_allocating", set.len()), |b| {
        b.iter(|| {
            black_box(bincode::decode_from_slice::<BenchSet, _>(black_box(&set), config).unwrap());
        });
    });
    sets.bench_function(BenchmarkId::new("new_reused", set.len()), |b| {
        b.iter(|| {
            bincode::decode_from_slice_into_hash_set(
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
    targets = second_pass_performance
}
criterion_main!(benches);
