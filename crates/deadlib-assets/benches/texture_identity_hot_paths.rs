use deadlib_assets::upload::{TextureUploadBudget, TextureUploadQueue};
use deadlib_assets::{
    ascii_ci_hash, clear_texture_handles, register_texture_handle, remove_texture_handle,
    texture_handle,
};
use deadlib_render_core::{FastU64Map, INVALID_TEXTURE_HANDLE, SamplerDesc, TextureHandle};
use image::RgbaImage;
use rustc_hash::FxHashMap;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{VecDeque, hash_map::Entry};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocator requests are delegated unchanged to `System`; relaxed
// counters only observe the single-threaded benchmark's gated interval.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.free_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: the pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    frees: u64,
    alloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    fn churn(self) -> u64 {
        self.alloc_bytes + self.free_bytes
    }
}

struct ResultRow {
    ns: f64,
    cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
    ops: usize,
}

fn measure(ops: usize, mut op: impl FnMut() -> u64) -> ResultRow {
    for _ in 0..ops.min(4_096) {
        black_box(op());
    }
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..ops {
        checksum = checksum.wrapping_add(black_box(op()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..ops {
        black_box(op());
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);

    ResultRow {
        ns: elapsed.as_secs_f64() * 1e9 / ops as f64,
        cycles: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / ops as f64),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
        ops,
    }
}

struct LegacyVideoQueue {
    order: VecDeque<Arc<str>>,
    entries: FxHashMap<Arc<str>, Arc<RgbaImage>>,
}

impl LegacyVideoQueue {
    fn new() -> Self {
        Self {
            order: VecDeque::with_capacity(1),
            entries: FxHashMap::with_capacity_and_hasher(1, Default::default()),
        }
    }

    fn push(&mut self, key: Arc<str>, image: Arc<RgbaImage>) {
        match self.entries.entry(key) {
            Entry::Occupied(mut entry) => {
                entry.insert(image);
            }
            Entry::Vacant(entry) => {
                self.order.push_back(Arc::clone(entry.key()));
                entry.insert(image);
            }
        }
    }

    fn pop(&mut self) -> Option<(Arc<str>, Arc<RgbaImage>)> {
        let key = self.order.pop_front()?;
        self.entries.remove_entry(&key)
    }
}

fn video_identity_bench() {
    const OPS: usize = 1_000_000;
    let key: Arc<str> = Arc::from("gameplay/backgrounds/a-long-video-texture-key.mp4");
    let image = Arc::new(RgbaImage::new(4, 4));
    let mut old_queue = LegacyVideoQueue::new();
    let old = measure(OPS, || {
        old_queue.push(Arc::clone(&key), Arc::clone(&image));
        let (queued_key, queued_image) = old_queue.pop().unwrap();
        queued_key.len() as u64 + queued_image.width() as u64
    });

    let mut new_queue = TextureUploadQueue::default();
    let budget = TextureUploadBudget {
        max_uploads: 1,
        max_bytes: 64,
    };
    let new = measure(OPS, || {
        new_queue.push(42, Arc::clone(&image), SamplerDesc::default());
        let (handle, upload) = new_queue.pop_next(budget, 0, 0).unwrap();
        handle + upload.image().width() as u64 + key.len() as u64 - 42
    });
    assert_eq!(old.checksum, new.checksum);

    println!("video upload identity (steady same-size frame, GPU work excluded)");
    print("old", &old);
    print("new", &new);
    print_change(&old, &new);
}

struct LegacyRegistry {
    handles: RwLock<FxHashMap<String, TextureHandle>>,
    aliases: RwLock<FastU64Map<TextureHandle>>,
}

impl LegacyRegistry {
    fn new() -> Self {
        Self {
            handles: RwLock::new(FxHashMap::default()),
            aliases: RwLock::new(FastU64Map::default()),
        }
    }

    fn note(aliases: &mut FastU64Map<TextureHandle>, key: &str, handle: TextureHandle) {
        let folded = ascii_ci_hash(key);
        match aliases.get_mut(&folded) {
            Some(existing) if *existing != handle => *existing = INVALID_TEXTURE_HANDLE,
            Some(_) => {}
            None => {
                aliases.insert(folded, handle);
            }
        }
    }

    fn register(&self, key: &str, handle: TextureHandle) {
        let mut handles = self.handles.write().unwrap();
        let mut aliases = self.aliases.write().unwrap();
        let replaced = handles.insert(key.to_string(), handle);
        if replaced.is_some_and(|old| old != handle) {
            aliases.clear();
            aliases.reserve(handles.len());
            for (key, &handle) in handles.iter() {
                Self::note(&mut aliases, key, handle);
            }
        } else if replaced.is_none() {
            Self::note(&mut aliases, key, handle);
        }
    }

    fn remove(&self, key: &str) {
        let mut handles = self.handles.write().unwrap();
        if handles.remove(key).is_none() {
            return;
        }
        let mut aliases = self.aliases.write().unwrap();
        aliases.clear();
        aliases.reserve(handles.len());
        for (key, &handle) in handles.iter() {
            Self::note(&mut aliases, key, handle);
        }
    }

    fn lookup(&self, key: &str) -> TextureHandle {
        self.aliases
            .read()
            .unwrap()
            .get(&ascii_ci_hash(key))
            .copied()
            .filter(|&handle| handle != INVALID_TEXTURE_HANDLE)
            .unwrap_or(INVALID_TEXTURE_HANDLE)
    }
}

fn alias_removal_bench() {
    const KEYS: usize = 8_192;
    const OPS: usize = 10_000;
    let keys: Vec<String> = (0..KEYS)
        .map(|index| format!("Graphics/Texture-{index:05}.PNG"))
        .collect();
    let target = keys.last().unwrap();
    let query = target.to_ascii_lowercase();
    let handle = KEYS as u64;

    let legacy = LegacyRegistry::new();
    for (index, key) in keys.iter().enumerate() {
        legacy.register(key, index as u64 + 1);
    }
    let old = measure(OPS, || {
        legacy.remove(target);
        legacy.register(target, handle);
        legacy.lookup(&query)
    });

    clear_texture_handles();
    for (index, key) in keys.iter().enumerate() {
        register_texture_handle(key, index as u64 + 1);
    }
    let new = measure(OPS, || {
        remove_texture_handle(target);
        register_texture_handle(target, handle);
        texture_handle(&query)
    });
    clear_texture_handles();
    assert_eq!(old.checksum, new.checksum);

    println!("texture alias remove/reinsert ({KEYS} registered keys)");
    print("old", &old);
    print("new", &new);
    print_change(&old, &new);
}

fn main() {
    video_identity_bench();
    alias_removal_bench();
}

fn print(label: &str, row: &ResultRow) {
    println!(
        "  {label:<3} {:>10.2} ns/op  {:>10.2} cycles/op  {:>8.3} Mop/s  \
         {:>6.2} alloc/op  {:>6.2} free/op  {:>10.1} churn B/op",
        row.ns,
        row.cycles.unwrap_or(f64::NAN),
        1_000.0 / row.ns,
        row.alloc.allocs as f64 / row.ops as f64,
        row.alloc.frees as f64 / row.ops as f64,
        row.alloc.churn() as f64 / row.ops as f64,
    );
}

fn print_change(old: &ResultRow, new: &ResultRow) {
    println!(
        "  change: {:+.2}% latency  {:+.2}% cycles  {:+.2}% churn",
        change(old.ns, new.ns),
        change(
            old.cycles.unwrap_or(f64::NAN),
            new.cycles.unwrap_or(f64::NAN)
        ),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}
