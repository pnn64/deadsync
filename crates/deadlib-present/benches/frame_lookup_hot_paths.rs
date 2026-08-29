use deadlib_present::compose::{FrameInlineByteIndexBench, TextureLookupBenchState};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHasher};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 21;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every request is delegated unchanged to `System`; relaxed counters
// observe only this single-threaded benchmark while their gate is enabled.
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

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the pointer-layout pair came from the allocator caller.
        let new_ptr = unsafe { System.realloc(ptr, old, new_size) };
        if !new_ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(new_size as u64, Ordering::Relaxed);
            self.free_bytes
                .fetch_add(old.size() as u64, Ordering::Relaxed);
        }
        new_ptr
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    alloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn churn(self) -> u64 {
        self.alloc_bytes + self.free_bytes
    }
}

struct Row {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
    ops: usize,
    items: usize,
}

fn measure(ops: usize, items: usize, mut op: impl FnMut() -> u64) -> Row {
    for _ in 0..ops.min(8) {
        black_box(op());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        let mut sample_checksum = 0u64;
        for _ in 0..ops {
            sample_checksum = sample_checksum.wrapping_add(black_box(op()));
        }
        let ns = started.elapsed().as_secs_f64() * 1e9 / ops as f64;
        let cycle_end = cycle_counter();
        times.push(ns);
        if let Some(sample_cycles) = cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / ops as f64)
        {
            cycles.push(sample_cycles);
        }
        checksum ^= sample_checksum;
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..ops {
        black_box(op());
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
        ops,
        items,
    }
}

#[derive(Clone, Copy)]
struct LegacyTextureEntry<T> {
    fingerprint: u64,
    validated_frame: u64,
    value: T,
}

type LegacyTextureMap<T> = FxHashMap<usize, LegacyTextureEntry<T>>;

struct LegacyTextureLookupState {
    frame: u64,
    dims: LegacyTextureMap<(u32, u32)>,
    sheets: LegacyTextureMap<(u32, u32)>,
    handles: LegacyTextureMap<u64>,
}

impl LegacyTextureLookupState {
    fn new(capacity: usize) -> Self {
        Self {
            frame: 0,
            dims: FxHashMap::with_capacity_and_hasher(capacity, FxBuildHasher),
            sheets: FxHashMap::with_capacity_and_hasher(capacity, FxBuildHasher),
            handles: FxHashMap::with_capacity_and_hasher(capacity, FxBuildHasher),
        }
    }

    fn key_fingerprint(key: &str) -> u64 {
        let mut hasher = FxHasher::default();
        key.hash(&mut hasher);
        hasher.finish()
    }

    fn value<T: Copy>(
        frame: u64,
        entries: &mut LegacyTextureMap<T>,
        key_ptr: usize,
        key: &str,
        build: impl FnOnce(&str) -> T,
    ) -> T {
        if let Some(entry) = entries.get_mut(&key_ptr) {
            if entry.validated_frame == frame {
                return entry.value;
            }
            let fingerprint = Self::key_fingerprint(key);
            if entry.fingerprint == fingerprint {
                entry.validated_frame = frame;
                return entry.value;
            }
            let value = build(key);
            *entry = LegacyTextureEntry {
                fingerprint,
                validated_frame: frame,
                value,
            };
            return value;
        }
        let fingerprint = Self::key_fingerprint(key);
        let value = build(key);
        entries.insert(
            key_ptr,
            LegacyTextureEntry {
                fingerprint,
                validated_frame: frame,
                value,
            },
        );
        value
    }

    fn lookup_frame(&mut self, keys: &[Arc<str>]) -> u64 {
        self.frame = self.frame.wrapping_add(1).max(1);
        keys.iter().fold(0u64, |sum, key| {
            let key_ptr = Arc::as_ptr(key) as *const () as usize;
            let dims = Self::value(self.frame, &mut self.dims, key_ptr, key, |key| {
                (
                    key.len() as u32,
                    u32::from(key.as_bytes().first().copied().unwrap_or_default()) + 1,
                )
            });
            let sheet = Self::value(self.frame, &mut self.sheets, key_ptr, key, |key| {
                ((key.len() as u32 % 8) + 1, 2)
            });
            let handle = Self::value(self.frame, &mut self.handles, key_ptr, key, |key| {
                key.len() as u64 + 1
            });
            sum.wrapping_mul(131)
                .wrapping_add(u64::from(dims.0))
                .wrapping_add(u64::from(dims.1) << 8)
                .wrapping_add(u64::from(sheet.0) << 16)
                .wrapping_add(u64::from(sheet.1) << 24)
                .wrapping_add(handle << 32)
        })
    }
}

struct LegacyInlineGlyphIndex {
    values: Vec<(u8, u8)>,
}

impl LegacyInlineGlyphIndex {
    fn new(domain: &[u8]) -> Self {
        let mut values = Vec::with_capacity(domain.len());
        for &byte in domain {
            if values.iter().any(|(known, _)| *known == byte) {
                continue;
            }
            values.push((byte, byte));
        }
        Self { values }
    }

    fn checksum(&self, text: &[u8]) -> Option<u64> {
        text.iter().try_fold(0u64, |sum, byte| {
            let (_, value) = self.values.iter().find(|(known, _)| known == byte)?;
            Some(sum.wrapping_mul(131).wrapping_add(u64::from(*value)))
        })
    }
}

fn texture_keys(count: usize) -> Vec<Arc<str>> {
    (0..count)
        .map(|index| {
            Arc::<str>::from(format!(
                "noteskins/default/player_{}/texture_{index:04} 4x1.png",
                index % 2
            ))
        })
        .collect()
}

fn main() {
    const ACTOR_BUILD_OPS: usize = 1_024;
    const ACTOR_LOOKUP_OPS: usize = 8_192;
    const INLINE_OPS: usize = 16_384;
    const TEXTURE_COLD_OPS: usize = 512;
    const TEXTURE_FRAME_OPS: usize = 2_048;

    let pointer_keys = (0..256)
        .map(|index| 0x1_0000usize + index * 64)
        .collect::<Vec<_>>();
    let old_actor_build = measure(ACTOR_BUILD_OPS, pointer_keys.len() * 2, || {
        let mut ids = HashMap::with_capacity(pointer_keys.len());
        for (id, &key) in pointer_keys.iter().enumerate() {
            ids.insert(key, id as u32);
        }
        pointer_keys
            .iter()
            .fold(0u64, |sum, key| sum.wrapping_add(u64::from(ids[key])))
    });
    let new_actor_build = measure(ACTOR_BUILD_OPS, pointer_keys.len() * 2, || {
        let mut ids = FxHashMap::with_capacity_and_hasher(pointer_keys.len(), FxBuildHasher);
        for (id, &key) in pointer_keys.iter().enumerate() {
            ids.insert(key, id as u32);
        }
        pointer_keys
            .iter()
            .fold(0u64, |sum, key| sum.wrapping_add(u64::from(ids[key])))
    });
    assert_eq!(old_actor_build.checksum, new_actor_build.checksum);
    print_pair(
        "actor texture-pointer prewarm (256 insert + 256 lookup)",
        &old_actor_build,
        &new_actor_build,
    );

    let old_actor_ids = HashMap::<usize, u32>::from_iter(
        pointer_keys
            .iter()
            .enumerate()
            .map(|(id, &key)| (key, id as u32)),
    );
    let new_actor_ids = FxHashMap::<usize, u32>::from_iter(
        pointer_keys
            .iter()
            .enumerate()
            .map(|(id, &key)| (key, id as u32)),
    );
    let actor_probes = (0..512)
        .map(|index| pointer_keys[(index * 73) % pointer_keys.len()])
        .collect::<Vec<_>>();
    let old_actor_lookup = measure(ACTOR_LOOKUP_OPS, actor_probes.len(), || {
        actor_probes.iter().fold(0u64, |sum, key| {
            sum.wrapping_add(u64::from(old_actor_ids[key]))
        })
    });
    let new_actor_lookup = measure(ACTOR_LOOKUP_OPS, actor_probes.len(), || {
        actor_probes.iter().fold(0u64, |sum, key| {
            sum.wrapping_add(u64::from(new_actor_ids[key]))
        })
    });
    assert_eq!(old_actor_lookup.checksum, new_actor_lookup.checksum);
    print_pair(
        "actor texture-pointer fallback (512 lookups)",
        &old_actor_lookup,
        &new_actor_lookup,
    );

    let glyph_domain = b"0123456789-.ms";
    let values: [&[u8]; 8] = [
        b"0",
        b"123456789",
        b"-12.34ms",
        b"9999.99ms",
        b"0.00ms",
        b"1234567890",
        b"-0.01ms",
        b"42ms",
    ];
    let old_inline = LegacyInlineGlyphIndex::new(glyph_domain);
    let new_inline = FrameInlineByteIndexBench::new(glyph_domain);
    let old_glyph_lookup = measure(INLINE_OPS, values.iter().map(|v| v.len()).sum(), || {
        values.iter().fold(0u64, |sum, value| {
            sum.wrapping_add(
                old_inline
                    .checksum(black_box(value))
                    .expect("domain covers text"),
            )
        })
    });
    let new_glyph_lookup = measure(INLINE_OPS, values.iter().map(|v| v.len()).sum(), || {
        values.iter().fold(0u64, |sum, value| {
            sum.wrapping_add(
                new_inline
                    .checksum(black_box(value))
                    .expect("domain covers text"),
            )
        })
    });
    assert_eq!(old_glyph_lookup.checksum, new_glyph_lookup.checksum);
    print_pair(
        "changing inline-text glyph resolution (8 values)",
        &old_glyph_lookup,
        &new_glyph_lookup,
    );

    let keys = texture_keys(128);
    let old_texture_cold = measure(TEXTURE_COLD_OPS, keys.len() * 3, || {
        LegacyTextureLookupState::new(keys.len()).lookup_frame(black_box(&keys))
    });
    let new_texture_cold = measure(TEXTURE_COLD_OPS, keys.len() * 3, || {
        TextureLookupBenchState::new(keys.len()).lookup_frame(black_box(&keys))
    });
    assert_eq!(old_texture_cold.checksum, new_texture_cold.checksum);
    print_pair(
        "texture lookup cache warmup (128 keys x 3 fields)",
        &old_texture_cold,
        &new_texture_cold,
    );

    let mut old_texture_state = LegacyTextureLookupState::new(keys.len());
    let mut new_texture_state = TextureLookupBenchState::new(keys.len());
    assert_eq!(
        old_texture_state.lookup_frame(&keys),
        new_texture_state.lookup_frame(&keys)
    );
    let old_texture_frame = measure(TEXTURE_FRAME_OPS, keys.len() * 3, || {
        old_texture_state.lookup_frame(black_box(&keys))
    });
    let new_texture_frame = measure(TEXTURE_FRAME_OPS, keys.len() * 3, || {
        new_texture_state.lookup_frame(black_box(&keys))
    });
    assert_eq!(old_texture_frame.checksum, new_texture_frame.checksum);
    print_pair(
        "texture lookup cache revalidation (128 keys x 3 fields)",
        &old_texture_frame,
        &new_texture_frame,
    );
}

fn print_pair(name: &str, old: &Row, new: &Row) {
    println!("{name}");
    print_row("old", old);
    print_row("new", new);
    println!(
        "  change: {:+.2}% median  {:+.2}% p95  {:+.2}% cycles  {:+.2}% allocs  {:+.2}% churn",
        change(old.median_ns, new.median_ns),
        change(old.p95_ns, new.p95_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN)
        ),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, row: &Row) {
    let allocs = row.alloc.allocs as f64 / row.ops as f64;
    let reallocs = row.alloc.reallocs as f64 / row.ops as f64;
    let frees = row.alloc.frees as f64 / row.ops as f64;
    let churn = row.alloc.churn() as f64 / row.ops as f64;
    let throughput = row.items as f64 * 1e9 / row.median_ns;
    println!(
        "  {label:<3} {:>11.1} ns  p95 {:>11.1} ns  {:>11.1} cycles  {:>10.0} item/s  \
         {:>7.1} alloc  {:>6.1} realloc  {:>7.1} free  {:>12.1} churn B/op",
        row.median_ns,
        row.p95_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        throughput,
        allocs,
        reallocs,
        frees,
        churn,
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
