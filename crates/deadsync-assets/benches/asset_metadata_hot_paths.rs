use deadlib_assets::{
    parse_texture_hints, strip_sprite_hints, texture_hint_doubleres, texture_hint_is_default,
};
use deadsync_assets::dynamic_media::{path_texture_key, texture_key_set};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::hint::black_box;
use std::path::{Path, PathBuf};
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

#[derive(Default)]
struct LegacyTextureHints {
    raw: String,
    mipmaps: Option<bool>,
    grayscale: bool,
    alphamap: bool,
    doubleres: bool,
    stretch: bool,
    dither: bool,
    color_depth: Option<u32>,
    sampler_filter: Option<u8>,
    sampler_wrap: Option<u8>,
}

impl LegacyTextureHints {
    fn is_default(&self) -> bool {
        self.raw.is_empty() || self.raw.eq_ignore_ascii_case("default")
    }
}

fn ascii_ci_contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

fn legacy_parse_texture_hints(raw: &str) -> LegacyTextureHints {
    let mut hints = LegacyTextureHints::default();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return hints;
    }
    hints.raw = trimmed.to_string();
    if trimmed.eq_ignore_ascii_case("default") {
        return hints;
    }

    let has = |sub: &[u8]| ascii_ci_contains(trimmed.as_bytes(), sub);
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
        hints.sampler_filter = Some(1);
    }
    if has(b"linear") {
        hints.sampler_filter = Some(2);
    }
    if has(b"wrap") || has(b"repeat") {
        hints.sampler_wrap = Some(1);
    }
    if has(b"clamp") {
        hints.sampler_wrap = Some(2);
    }
    if hints.mipmaps == Some(true) && hints.sampler_wrap.is_none() {
        hints.sampler_wrap = Some(1);
    }
    hints
}

fn legacy_hint_checksum(hints: &LegacyTextureHints) -> u64 {
    u64::from(!hints.is_default())
        | (u64::from(hints.doubleres) << 1)
        | (u64::from(hints.grayscale) << 2)
        | (u64::from(hints.alphamap) << 3)
        | (u64::from(hints.stretch) << 4)
        | (u64::from(hints.dither) << 5)
        | (u64::from(hints.mipmaps == Some(true)) << 6)
        | (u64::from(hints.mipmaps == Some(false)) << 7)
        | (u64::from(hints.color_depth.unwrap_or_default()) << 8)
        | (u64::from(hints.sampler_filter.unwrap_or_default()) << 16)
        | (u64::from(hints.sampler_wrap.unwrap_or_default()) << 18)
}

fn hint_checksum(hints: &deadlib_assets::TextureHints) -> u64 {
    use deadlib_render_core::{SamplerFilter, SamplerWrap};

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
    u64::from(!hints.is_default())
        | (u64::from(hints.doubleres) << 1)
        | (u64::from(hints.grayscale) << 2)
        | (u64::from(hints.alphamap) << 3)
        | (u64::from(hints.stretch) << 4)
        | (u64::from(hints.dither) << 5)
        | (u64::from(hints.mipmaps == Some(true)) << 6)
        | (u64::from(hints.mipmaps == Some(false)) << 7)
        | (u64::from(hints.color_depth.unwrap_or_default()) << 8)
        | (filter << 16)
        | (wrap << 18)
}

fn legacy_strip_sprite_hints(name: &str) -> String {
    let file_name = Path::new(name)
        .file_name()
        .and_then(|file| file.to_str())
        .unwrap_or(name);
    let without_ext = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    let bytes = without_ext.as_bytes();
    let mut out = String::with_capacity(without_ext.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b' ' {
            let mut left = i + 1;
            while left < bytes.len() && bytes[left].is_ascii_digit() {
                left += 1;
            }
            if left > i + 1 && left < bytes.len() && matches!(bytes[left], b'x' | b'X') {
                let mut right = left + 1;
                while right < bytes.len() && bytes[right].is_ascii_digit() {
                    right += 1;
                }
                if right > left + 1 {
                    i = right;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out.replace(" (doubleres)", "").trim().to_string()
}

fn string_checksum(value: &str) -> u64 {
    value.bytes().fold(value.len() as u64, |sum, byte| {
        sum.wrapping_mul(131).wrapping_add(u64::from(byte))
    })
}

fn media_paths() -> Vec<PathBuf> {
    (0..256)
        .map(|index| {
            PathBuf::from(format!(
                "Songs/Pack {:02}/Song {:03}/{}",
                index % 24,
                index % 160,
                if index % 5 == 0 {
                    "background movie.mp4"
                } else {
                    "banner image.png"
                }
            ))
        })
        .collect()
}

fn legacy_texture_key_set(paths: &[PathBuf]) -> HashSet<String> {
    paths.iter().map(|path| path_texture_key(path)).collect()
}

fn main() {
    const MEDIA_BUILD_OPS: usize = 256;
    const MEDIA_LOOKUP_OPS: usize = 4_096;
    const HINT_OPS: usize = 8_192;
    const STRIP_OPS: usize = 8_192;

    let paths = media_paths();
    let old_media = measure(MEDIA_BUILD_OPS, paths.len(), || {
        let keys = legacy_texture_key_set(black_box(&paths));
        let hits = paths
            .iter()
            .filter(|path| keys.contains(path.to_string_lossy().as_ref()))
            .count();
        ((keys.len() as u64) << 32) | hits as u64
    });
    let new_media = measure(MEDIA_BUILD_OPS, paths.len(), || {
        let keys = texture_key_set(black_box(paths.iter()));
        let hits = paths
            .iter()
            .filter(|path| keys.contains(path.to_string_lossy().as_ref()))
            .count();
        ((keys.len() as u64) << 32) | hits as u64
    });
    assert_eq!(old_media.checksum, new_media.checksum);
    print_pair(
        "dynamic-media key reconciliation (256 paths)",
        &old_media,
        &new_media,
    );

    let old_keys = legacy_texture_key_set(&paths);
    let new_keys = texture_key_set(paths.iter());
    let mut probes = paths
        .iter()
        .map(|path| path_texture_key(path))
        .collect::<Vec<_>>();
    probes.extend((0..128).map(|index| format!("missing/media/key_{index:03}.png")));
    let old_lookup = measure(MEDIA_LOOKUP_OPS, probes.len(), || {
        probes
            .iter()
            .fold(0u64, |sum, key| sum + u64::from(old_keys.contains(key)))
    });
    let new_lookup = measure(MEDIA_LOOKUP_OPS, probes.len(), || {
        probes
            .iter()
            .fold(0u64, |sum, key| sum + u64::from(new_keys.contains(key)))
    });
    assert_eq!(old_lookup.checksum, new_lookup.checksum);
    print_pair(
        "settled dynamic-media membership (384 probes)",
        &old_lookup,
        &new_lookup,
    );

    let hint_cases = [
        "",
        "default",
        "banner.png",
        "Tap Note 4x1 (doubleres).png",
        "sheet (32bpp dither mipmaps nearest wrap)",
        "font page (16BPP grayscale linear clamp)",
        "mask (alphamap nomipmaps point repeat)",
        "texture (stretch DOUBLEres)",
    ];
    let old_hints = measure(HINT_OPS, hint_cases.len(), || {
        hint_cases.iter().fold(0u64, |sum, raw| {
            sum.wrapping_mul(131)
                .wrapping_add(legacy_hint_checksum(&legacy_parse_texture_hints(
                    black_box(raw),
                )))
        })
    });
    let new_hints = measure(HINT_OPS, hint_cases.len(), || {
        hint_cases.iter().fold(0u64, |sum, raw| {
            sum.wrapping_mul(131)
                .wrapping_add(hint_checksum(&parse_texture_hints(black_box(raw))))
        })
    });
    assert_eq!(old_hints.checksum, new_hints.checksum);
    print_pair(
        "texture-hint parsing (8 representative strings)",
        &old_hints,
        &new_hints,
    );

    let old_predicates = measure(HINT_OPS, hint_cases.len(), || {
        hint_cases.iter().fold(0u64, |sum, raw| {
            let hints = legacy_parse_texture_hints(black_box(raw));
            sum.wrapping_mul(3)
                .wrapping_add(u64::from(!hints.is_default()))
                .wrapping_add(u64::from(hints.doubleres) << 1)
        })
    });
    let new_predicates = measure(HINT_OPS, hint_cases.len(), || {
        hint_cases.iter().fold(0u64, |sum, raw| {
            sum.wrapping_mul(3)
                .wrapping_add(u64::from(!texture_hint_is_default(black_box(raw))))
                .wrapping_add(u64::from(texture_hint_doubleres(black_box(raw))) << 1)
        })
    });
    assert_eq!(old_predicates.checksum, new_predicates.checksum);
    print_pair(
        "font/dimension hint predicates (8 strings)",
        &old_predicates,
        &new_predicates,
    );

    let sprite_names = [
        "grades/grades 1x19.png",
        "_miso light 16x7 doubleres.png",
        "practice/snap_display_icon_9x1 (doubleres).png",
        "menu/loading dots 8X1 (doubleres).png",
        "noteskins/default/Tap Note 4x1.png",
        "banner image.png",
        "  padded texture 2x2  ",
        "two 1x2 hints 3x4 (doubleres).png",
    ];
    let old_strip = measure(STRIP_OPS, sprite_names.len(), || {
        sprite_names.iter().fold(0u64, |sum, name| {
            sum.wrapping_mul(131)
                .wrapping_add(string_checksum(&legacy_strip_sprite_hints(black_box(name))))
        })
    });
    let new_strip = measure(STRIP_OPS, sprite_names.len(), || {
        sprite_names.iter().fold(0u64, |sum, name| {
            sum.wrapping_mul(131)
                .wrapping_add(string_checksum(&strip_sprite_hints(black_box(name))))
        })
    });
    assert_eq!(old_strip.checksum, new_strip.checksum);
    print_pair(
        "sprite display-name normalization (8 names)",
        &old_strip,
        &new_strip,
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
