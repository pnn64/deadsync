use deadlib_assets::dynamic::benchmark_write_raw_cached_banner;
use deadlib_assets::registry::GeneratedTexturePendingBench;
use deadlib_assets::{TextureHints, parse_texture_hints};
use deadlib_render_core::{SamplerFilter, SamplerWrap};
use image::RgbaImage;
use rustc_hash::FxHashMap;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 21;
const CACHE_HEADER_SIZE: usize = 16;
const CACHE_MAGIC: [u8; 8] = *b"DSBNR02\0";

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

fn ascii_ci_contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

fn legacy_parse_texture_hints(raw: &str) -> TextureHints {
    let mut hints = TextureHints::default();
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("default") {
        return hints;
    }
    hints.non_default = true;
    let has = |needle| ascii_ci_contains(trimmed.as_bytes(), needle);
    if has(b"32bpp") {
        hints.color_depth = Some(32);
    } else if has(b"16bpp") {
        hints.color_depth = Some(16);
    }
    hints.dither = has(b"dither");
    hints.stretch = has(b"stretch");
    if has(b"mipmaps") {
        hints.mipmaps = Some(true);
    }
    if has(b"nomipmaps") {
        hints.mipmaps = Some(false);
    }
    hints.grayscale = has(b"grayscale");
    hints.alphamap = has(b"alphamap");
    hints.doubleres = has(b"doubleres");
    if has(b"nearest") || has(b"point") {
        hints.sampler_filter = Some(SamplerFilter::Nearest);
    }
    if has(b"linear") {
        hints.sampler_filter = Some(SamplerFilter::Linear);
    }
    if has(b"wrap") || has(b"repeat") {
        hints.sampler_wrap = Some(SamplerWrap::Repeat);
    }
    if has(b"clamp") {
        hints.sampler_wrap = Some(SamplerWrap::Clamp);
    }
    if hints.mipmaps == Some(true) && hints.sampler_wrap.is_none() {
        hints.sampler_wrap = Some(SamplerWrap::Repeat);
    }
    hints
}

fn hint_checksum(hints: TextureHints) -> u64 {
    let filter = match hints.sampler_filter {
        None => 0,
        Some(SamplerFilter::Nearest) => 1,
        Some(SamplerFilter::Linear) => 2,
    };
    let wrap = match hints.sampler_wrap {
        None => 0,
        Some(SamplerWrap::Repeat) => 1,
        Some(SamplerWrap::Clamp) => 2,
    };
    u64::from(hints.non_default)
        | (u64::from(hints.dither) << 1)
        | (u64::from(hints.stretch) << 2)
        | (u64::from(hints.grayscale) << 3)
        | (u64::from(hints.alphamap) << 4)
        | (u64::from(hints.doubleres) << 5)
        | (u64::from(hints.mipmaps == Some(true)) << 6)
        | (u64::from(hints.mipmaps == Some(false)) << 7)
        | (u64::from(hints.color_depth.unwrap_or_default()) << 8)
        | (filter << 16)
        | (wrap << 18)
}

struct LegacyGeneratedEntry {
    value: u64,
    pending: bool,
}

struct LegacyGeneratedRegistry {
    entries: FxHashMap<String, LegacyGeneratedEntry>,
    pending: usize,
}

impl LegacyGeneratedRegistry {
    fn new(keys: &[String]) -> Self {
        let mut registry = Self {
            entries: FxHashMap::default(),
            pending: 0,
        };
        for (index, key) in keys.iter().enumerate() {
            registry.register(key, index as u64);
        }
        drop(registry.take_pending_keys());
        registry
    }

    fn register(&mut self, key: &str, value: u64) {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.value = value;
            if !entry.pending {
                entry.pending = true;
                self.pending += 1;
            }
            return;
        }
        self.entries.insert(
            key.to_string(),
            LegacyGeneratedEntry {
                value,
                pending: true,
            },
        );
        self.pending += 1;
    }

    fn update_and_drain(&mut self, keys: &[String], indices: &[usize]) -> Vec<String> {
        for &index in indices {
            self.register(&keys[index], index as u64);
        }
        self.take_pending_keys()
    }

    fn take_pending_keys(&mut self) -> Vec<String> {
        if self.pending == 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(self.pending);
        for (key, entry) in &mut self.entries {
            if entry.pending {
                entry.pending = false;
                out.push(key.clone());
            }
        }
        self.pending = 0;
        out
    }
}

fn pending_checksum(keys: Vec<String>) -> u64 {
    keys.into_iter().fold(0u64, |sum, key| {
        let hash = key.bytes().fold(key.len() as u64, |hash, byte| {
            hash.wrapping_mul(131) ^ u64::from(byte)
        });
        sum.wrapping_add(hash)
    })
}

fn legacy_write_raw_cached_banner(writer: impl Write, rgba: &RgbaImage) -> io::Result<usize> {
    let raw = rgba.as_raw();
    let mut out = Vec::<u8>::with_capacity(CACHE_HEADER_SIZE.saturating_add(raw.len()));
    out.extend_from_slice(&CACHE_MAGIC);
    out.extend_from_slice(&rgba.width().to_le_bytes());
    out.extend_from_slice(&rgba.height().to_le_bytes());
    out.extend_from_slice(raw);
    let mut writer = writer;
    writer.write_all(&out)?;
    Ok(out.len())
}

#[derive(Default)]
struct TouchWriter {
    bytes: usize,
    checksum: u64,
}

impl TouchWriter {
    fn finish(self, written: usize) -> u64 {
        debug_assert_eq!(self.bytes, written);
        self.checksum ^ written as u64
    }
}

impl Write for TouchWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut index = (64 - self.bytes % 64) % 64;
        while index < buf.len() {
            self.checksum = self
                .checksum
                .wrapping_mul(131)
                .wrapping_add(u64::from(black_box(buf[index])));
            index += 64;
        }
        self.bytes += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn source_image(width: u32, height: u32) -> RgbaImage {
    let raw = (0..width * height)
        .flat_map(|index| {
            [
                index.wrapping_mul(17) as u8,
                index.wrapping_mul(67).wrapping_add(3) as u8,
                index.wrapping_mul(131).wrapping_add(11) as u8,
                255,
            ]
        })
        .collect();
    RgbaImage::from_raw(width, height, raw).expect("valid benchmark image")
}

fn main() {
    const HINT_OPS: usize = 16_384;
    const GENERATED_OPS: usize = 512;
    const CACHE_WRITE_OPS: usize = 32;

    let hint_cases = [
        "",
        "default",
        "Tap Note 4x1 (doubleres).png",
        "sheet (32bpp dither mipmaps nearest wrap)",
        "font page (16BPP grayscale linear clamp)",
        "mask (alphamap nomipmaps point repeat)",
        "texture (stretch DOUBLEres)",
        "long/path/to/a/plain/texture-without-any-special-options.png",
    ];
    let hint_bytes = hint_cases.iter().map(|case| case.len()).sum();
    let old_hints = measure(HINT_OPS, hint_bytes, || {
        hint_cases.iter().fold(0u64, |sum, raw| {
            sum.wrapping_mul(131)
                .wrapping_add(hint_checksum(legacy_parse_texture_hints(black_box(raw))))
        })
    });
    let new_hints = measure(HINT_OPS, hint_bytes, || {
        hint_cases.iter().fold(0u64, |sum, raw| {
            sum.wrapping_mul(131)
                .wrapping_add(hint_checksum(parse_texture_hints(black_box(raw))))
        })
    });
    assert_eq!(old_hints.checksum, new_hints.checksum);
    print_pair(
        "texture-hint classification (8 keys)",
        &old_hints,
        &new_hints,
    );

    let generated_keys = (0..4_096)
        .map(|index| format!("generated/session/texture_{index:04}"))
        .collect::<Vec<_>>();
    let updates = [7, 1_023, 2_048, 4_095];
    let mut old_generated = LegacyGeneratedRegistry::new(&generated_keys);
    let old_generated_row = measure(GENERATED_OPS, updates.len(), || {
        pending_checksum(old_generated.update_and_drain(&generated_keys, &updates))
    });
    let mut new_generated = GeneratedTexturePendingBench::new(&generated_keys);
    let new_generated_row = measure(GENERATED_OPS, updates.len(), || {
        pending_checksum(new_generated.update_and_drain(&generated_keys, &updates))
    });
    assert_eq!(old_generated_row.checksum, new_generated_row.checksum);
    print_pair(
        "generated-texture sparse drain (4 of 4096 keys)",
        &old_generated_row,
        &new_generated_row,
    );

    let rgba = source_image(1_024, 512);
    let cache_bytes = CACHE_HEADER_SIZE + rgba.as_raw().len();
    let old_cache_write = measure(CACHE_WRITE_OPS, cache_bytes, || {
        let mut writer = TouchWriter::default();
        let written = legacy_write_raw_cached_banner(&mut writer, black_box(&rgba))
            .expect("legacy memory write");
        writer.finish(written)
    });
    let new_cache_write = measure(CACHE_WRITE_OPS, cache_bytes, || {
        let mut writer = TouchWriter::default();
        let written = benchmark_write_raw_cached_banner(&mut writer, black_box(&rgba))
            .expect("streamed memory write");
        writer.finish(written)
    });
    assert_eq!(old_cache_write.checksum, new_cache_write.checksum);
    print_pair(
        "raw artwork-cache staging (2 MiB RGBA)",
        &old_cache_write,
        &new_cache_write,
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
