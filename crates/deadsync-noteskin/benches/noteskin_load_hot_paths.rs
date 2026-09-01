use deadsync_noteskin::itg::{IniData, bench_support};
use deadsync_noteskin::{
    NOTE_ANIM_PART_COUNT, NoteAnimPart, NoteColorType, NoteDisplayMetrics, NotePartAnimation,
    NotePartTextureTranslate, explosion_bench_support, mine_bench_support,
    model_draw_bench_support, receptor_bench_support, sprite_math_bench_support,
    tap_explosion_bench_support, uv_color_bench_support,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::fmt::Write;
use std::hint::black_box;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

const SAMPLES: usize = 21;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
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
            realloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocation requests are forwarded unchanged to `System`; the
// relaxed counters are observed only by this single-threaded benchmark.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
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
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.realloc_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn churn(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct Row {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
    units: usize,
}

fn measure(operations: usize, units: usize, mut op: impl FnMut() -> u64) -> Row {
    for _ in 0..4 {
        black_box(op());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0_u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..operations {
            checksum = checksum.wrapping_add(black_box(op()));
        }
        times.push(started.elapsed().as_secs_f64() * 1e9 / operations as f64);
        if let Some(elapsed) = cycle_start
            .zip(cycle_counter())
            .map(|(start, end)| end.wrapping_sub(start) as f64 / operations as f64)
        {
            cycles.push(elapsed);
        }
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
        units,
    }
}

fn print_pair(title: &str, old: &Row, new: &Row) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title} ({} units/op)", old.units);
    print_row("old", old);
    print_row("new", new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% allocs  {:>7.2}% reallocs  {:>7.2}% churn",
        change(old.median_ns, new.median_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(old), throughput(new)),
        change(old.p95_ns, new.p95_ns),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.reallocs as f64, new.alloc.reallocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, row: &Row) {
    println!(
        "  {label:<3} {:>10.1} ns/op  {:>10.1} cycles/op  {:>10.1} p95 ns  \
         {:>8.2} Munit/s  {:>4} alloc  {:>4} realloc  {:>4} free  {:>8} churn B",
        row.median_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        row.p95_ns,
        throughput(row) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.churn(),
    );
}

fn throughput(row: &Row) -> f64 {
    row.units as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn run_uv_case(title: &str, old_op: fn(usize) -> u64, new_op: fn(usize) -> u64) {
    const UV_EVALUATIONS: usize = 8_192;
    const UV_OPERATIONS: usize = 64;
    assert_eq!(
        old_op(UV_EVALUATIONS),
        new_op(UV_EVALUATIONS),
        "{title} behavior diverged before measurement"
    );
    let old = measure(UV_OPERATIONS, UV_EVALUATIONS, || old_op(UV_EVALUATIONS));
    let new = measure(UV_OPERATIONS, UV_EVALUATIONS, || new_op(UV_EVALUATIONS));
    print_pair(title, &old, &new);
    assert_eq!(old.alloc.churn(), 0, "{title} legacy path allocated");
    assert_eq!(new.alloc.churn(), 0, "{title} current path allocated");
    assert!(
        new.median_ns < old.median_ns,
        "{title} median latency did not improve"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(
            new_cycles < old_cycles,
            "{title} median CPU cycles did not improve"
        );
    }
}

fn run_allocation_case(title: &str, old_op: fn(usize) -> u64, new_op: fn(usize) -> u64) {
    const EVALUATIONS: usize = 256;
    const OPERATIONS: usize = 256;
    assert_eq!(
        old_op(EVALUATIONS),
        new_op(EVALUATIONS),
        "{title} behavior diverged before measurement"
    );
    let old = measure(OPERATIONS, EVALUATIONS, || old_op(EVALUATIONS));
    let new = measure(OPERATIONS, EVALUATIONS, || new_op(EVALUATIONS));
    print_pair(title, &old, &new);
    assert!(
        new.median_ns < old.median_ns,
        "{title} median latency did not improve"
    );
    assert!(
        new.p95_ns < old.p95_ns,
        "{title} p95 latency did not improve"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(
            new_cycles < old_cycles,
            "{title} median CPU cycles did not improve"
        );
    }
    assert!(
        new.alloc.allocs < old.alloc.allocs,
        "{title} allocations did not fall"
    );
    assert!(
        new.alloc.reallocs <= old.alloc.reallocs,
        "{title} reallocations increased"
    );
    assert!(
        new.alloc.frees < old.alloc.frees,
        "{title} frees did not fall"
    );
    assert!(
        new.alloc.churn() < old.alloc.churn(),
        "{title} memory churn did not fall"
    );
}

fn run_gradient_case(title: &str, old_op: fn(usize) -> u64, new_op: fn(usize) -> u64) {
    const EVALUATIONS: usize = 1;
    const OPERATIONS: usize = 8;
    assert_eq!(
        old_op(EVALUATIONS),
        new_op(EVALUATIONS),
        "{title} behavior diverged before measurement"
    );
    let old = measure(OPERATIONS, EVALUATIONS, || old_op(EVALUATIONS));
    let new = measure(OPERATIONS, EVALUATIONS, || new_op(EVALUATIONS));
    print_pair(title, &old, &new);
    assert!(
        new.median_ns < old.median_ns,
        "{title} median latency did not improve"
    );
    assert!(
        new.p95_ns < old.p95_ns,
        "{title} p95 latency did not improve"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(
            new_cycles < old_cycles,
            "{title} median CPU cycles did not improve"
        );
    }
    assert!(
        new.alloc.allocs <= old.alloc.allocs,
        "{title} allocations increased"
    );
    assert!(
        new.alloc.reallocs <= old.alloc.reallocs,
        "{title} reallocations increased"
    );
    assert!(
        new.alloc.frees <= old.alloc.frees,
        "{title} frees increased"
    );
    assert!(
        new.alloc.churn() <= old.alloc.churn(),
        "{title} memory churn increased"
    );
}

fn fixture() -> String {
    let mut raw = String::with_capacity(16 * 1024);
    raw.push_str("GlobalOnly = before-section\n[NoteDisplay]\n");
    raw.push_str(
        "DrawHoldHeadForTapsOnSameRow=0\n\
         DrawRollHeadForTapsOnSameRow=1\n\
         TapHoldRollOnRowMeansHold=1\n\
         HoldHeadIsAboveWavyParts=1\n\
         HoldTailIsAboveWavyParts=0\n\
         StartDrawingHoldBodyOffsetFromHead=1.25\n\
         StopDrawingHoldBodyOffsetFromTail=-0.5\n\
         HoldLetGoGrayPercent=0.4\n\
         FlipHeadAndTailWhenReverse=1\n\
         FlipHoldBodyWhenReverse=0\n\
         TopHoldAnchorWhenReverse=1\n\
         HoldActiveIsAddLayer=1\n",
    );
    for (index, part) in NoteAnimPart::ALL.into_iter().enumerate() {
        let prefix = part.metric_prefix();
        writeln!(raw, "{prefix}AnimationLength={}.25", index + 1).unwrap();
        writeln!(raw, "{prefix}AnimationIsVivid={}", index % 2).unwrap();
        writeln!(raw, "{prefix}AdditionTextureCoordOffsetX=0.{index}").unwrap();
        writeln!(raw, "{prefix}AdditionTextureCoordOffsetY=-0.{index}").unwrap();
        writeln!(raw, "{prefix}NoteColorTextureCoordSpacingX=0.125").unwrap();
        writeln!(raw, "{prefix}NoteColorTextureCoordSpacingY=0.25").unwrap();
        writeln!(raw, "{prefix}NoteColorCount={}", index + 4).unwrap();
        writeln!(raw, "{prefix}NoteColorType=ProgressAlternate").unwrap();
    }
    for section in ["Left", "Down", "Up", "Right", "Center", "Fallback"] {
        writeln!(raw, "[{section}]").unwrap();
        for index in 0..20 {
            writeln!(raw, "Metric{index:02}=value-{section}-{index:02}").unwrap();
        }
    }
    raw
}

fn parse_clean_int(raw: &str) -> Option<i32> {
    raw.trim().parse().ok()
}

fn parse_clean_float(raw: &str) -> Option<f32> {
    raw.trim().parse().ok()
}

fn legacy_metric_checksum(metrics: &IniData) -> u64 {
    let mut out = NoteDisplayMetrics::default();
    let read_bool = |key: &str, default: bool| {
        metrics
            .get("NoteDisplay", key)
            .and_then(parse_clean_int)
            .map_or(default, |value| value != 0)
    };
    let read_float = |key: &str, default: f32| {
        metrics
            .get("NoteDisplay", key)
            .and_then(parse_clean_float)
            .unwrap_or(default)
    };
    let read_int = |key: &str, default: i32| {
        metrics
            .get("NoteDisplay", key)
            .and_then(parse_clean_int)
            .unwrap_or(default)
    };
    out.draw_hold_head_for_taps_on_same_row = read_bool(
        "DrawHoldHeadForTapsOnSameRow",
        out.draw_hold_head_for_taps_on_same_row,
    );
    out.draw_roll_head_for_taps_on_same_row = read_bool(
        "DrawRollHeadForTapsOnSameRow",
        out.draw_roll_head_for_taps_on_same_row,
    );
    out.tap_hold_roll_on_row_means_hold = read_bool(
        "TapHoldRollOnRowMeansHold",
        out.tap_hold_roll_on_row_means_hold,
    );
    out.hold_head_is_above_wavy_parts = read_bool(
        "HoldHeadIsAboveWavyParts",
        out.hold_head_is_above_wavy_parts,
    );
    out.hold_tail_is_above_wavy_parts = read_bool(
        "HoldTailIsAboveWavyParts",
        out.hold_tail_is_above_wavy_parts,
    );
    out.start_drawing_hold_body_offset_from_head = read_float(
        "StartDrawingHoldBodyOffsetFromHead",
        out.start_drawing_hold_body_offset_from_head,
    );
    out.stop_drawing_hold_body_offset_from_tail = read_float(
        "StopDrawingHoldBodyOffsetFromTail",
        out.stop_drawing_hold_body_offset_from_tail,
    );
    out.hold_let_go_gray_percent = read_float("HoldLetGoGrayPercent", out.hold_let_go_gray_percent);
    out.flip_head_and_tail_when_reverse = read_bool(
        "FlipHeadAndTailWhenReverse",
        out.flip_head_and_tail_when_reverse,
    );
    out.flip_hold_body_when_reverse =
        read_bool("FlipHoldBodyWhenReverse", out.flip_hold_body_when_reverse);
    out.top_hold_anchor_when_reverse =
        read_bool("TopHoldAnchorWhenReverse", out.top_hold_anchor_when_reverse);
    out.hold_active_is_add_layer = read_bool("HoldActiveIsAddLayer", out.hold_active_is_add_layer);
    for part in NoteAnimPart::ALL {
        let prefix = part.metric_prefix();
        let length_key = format!("{prefix}AnimationLength");
        let vivid_key = format!("{prefix}AnimationIsVivid");
        let add_x_key = format!("{prefix}AdditionTextureCoordOffsetX");
        let add_y_key = format!("{prefix}AdditionTextureCoordOffsetY");
        let spacing_x_key = format!("{prefix}NoteColorTextureCoordSpacingX");
        let spacing_y_key = format!("{prefix}NoteColorTextureCoordSpacingY");
        let count_key = format!("{prefix}NoteColorCount");
        let color_type_key = format!("{prefix}NoteColorType");
        let default_anim = out.part_animation[part as usize];
        let length = read_float(&length_key, default_anim.length).abs().max(1e-6);
        let vivid = read_bool(&vivid_key, default_anim.vivid);
        out.part_animation[part as usize] = NotePartAnimation { length, vivid };
        let default_translate = out.part_texture_translate[part as usize];
        let addition_offset = [
            read_float(&add_x_key, default_translate.addition_offset[0]),
            read_float(&add_y_key, default_translate.addition_offset[1]),
        ];
        let note_color_spacing = [
            read_float(&spacing_x_key, default_translate.note_color_spacing[0]),
            read_float(&spacing_y_key, default_translate.note_color_spacing[1]),
        ];
        let note_color_count = read_int(&count_key, default_translate.note_color_count);
        let note_color_type = metrics
            .get("NoteDisplay", &color_type_key)
            .and_then(NoteColorType::from_metric)
            .unwrap_or(default_translate.note_color_type);
        out.part_texture_translate[part as usize] = NotePartTextureTranslate {
            addition_offset,
            note_color_spacing,
            note_color_count,
            note_color_type,
        };
    }
    metric_checksum(out)
}

fn metric_checksum(parsed: NoteDisplayMetrics) -> u64 {
    let mut checksum = u64::from(parsed.draw_hold_head_for_taps_on_same_row)
        | (u64::from(parsed.draw_roll_head_for_taps_on_same_row) << 1)
        | (u64::from(parsed.tap_hold_roll_on_row_means_hold) << 2)
        | (u64::from(parsed.hold_head_is_above_wavy_parts) << 3)
        | (u64::from(parsed.hold_tail_is_above_wavy_parts) << 4)
        | (u64::from(parsed.flip_head_and_tail_when_reverse) << 5)
        | (u64::from(parsed.flip_hold_body_when_reverse) << 6)
        | (u64::from(parsed.top_hold_anchor_when_reverse) << 7)
        | (u64::from(parsed.hold_active_is_add_layer) << 8);
    for value in [
        parsed.start_drawing_hold_body_offset_from_head,
        parsed.stop_drawing_hold_body_offset_from_tail,
        parsed.hold_let_go_gray_percent,
    ] {
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(u64::from(value.to_bits()));
    }
    for (animation, translate) in parsed
        .part_animation
        .into_iter()
        .zip(parsed.part_texture_translate)
    {
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(u64::from(animation.length.to_bits()))
            .wrapping_add(u64::from(animation.vivid));
        for value in [
            translate.addition_offset[0],
            translate.addition_offset[1],
            translate.note_color_spacing[0],
            translate.note_color_spacing[1],
        ] {
            checksum = checksum
                .wrapping_mul(131)
                .wrapping_add(u64::from(value.to_bits()));
        }
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(translate.note_color_count as u64)
            .wrapping_add(translate.note_color_type as u64);
    }
    checksum
}

#[derive(PartialEq, Eq, Hash)]
struct LegacyCacheKey {
    root: String,
    game: String,
    skin: String,
}

fn legacy_cache_key(root: &Path, game: &str, skin: &str) -> LegacyCacheKey {
    let skin = skin.trim();
    LegacyCacheKey {
        root: root.to_string_lossy().to_ascii_lowercase(),
        game: game.trim().to_ascii_lowercase(),
        skin: if skin.is_empty() {
            "default".to_owned()
        } else {
            skin.to_ascii_lowercase()
        },
    }
}

fn legacy_cache_hit_checksum(root: &Path, game: &str, skin: &str, hits: usize) -> u64 {
    let mut cache = HashMap::new();
    cache.insert(legacy_cache_key(root, game, skin), 17_u64);
    (0..hits).fold(0_u64, |checksum, index| {
        checksum.wrapping_add(
            cache
                .get(&legacy_cache_key(root, game, skin))
                .copied()
                .unwrap_or_default()
                .wrapping_add(index as u64),
        )
    })
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions have no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions have no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

fn main() {
    if std::env::var_os("DEADSYNC_EXPLOSION_SOURCE_ONLY").is_some() {
        run_allocation_case(
            "noteskin partitioned actor explosion sources",
            tap_explosion_bench_support::staged_actor_sources_old,
            tap_explosion_bench_support::partitioned_actor_sources_new,
        );
        run_allocation_case(
            "noteskin partitioned dim direct explosion sources",
            tap_explosion_bench_support::staged_dim_direct_sources_old,
            tap_explosion_bench_support::partitioned_dim_direct_sources_new,
        );
        run_allocation_case(
            "noteskin partitioned bright direct explosion sources",
            tap_explosion_bench_support::staged_bright_direct_sources_old,
            tap_explosion_bench_support::partitioned_bright_direct_sources_new,
        );
        return;
    }
    if std::env::var_os("DEADSYNC_MINE_GRADIENT_ONLY").is_some() {
        run_gradient_case(
            "noteskin cached mine-gradient radial geometry",
            mine_bench_support::repeated_radial_geometry_old,
            mine_bench_support::cached_radial_geometry_new,
        );
        run_gradient_case(
            "noteskin prequantized mine-gradient RGB",
            mine_bench_support::per_pixel_rgb_quantization_old,
            mine_bench_support::prequantized_rgb_new,
        );
        run_gradient_case(
            "noteskin prequantized mine-gradient interior alpha",
            mine_bench_support::per_pixel_alpha_quantization_old,
            mine_bench_support::prequantized_interior_alpha_new,
        );
        return;
    }
    if std::env::var_os("DEADSYNC_DIRECT_EXPLOSION_ONLY").is_some() {
        run_allocation_case(
            "noteskin stack-backed direct explosion names",
            explosion_bench_support::formatted_direct_element_names_old,
            explosion_bench_support::stack_direct_element_names_new,
        );
        run_allocation_case(
            "noteskin fused direct explosion resolution",
            explosion_bench_support::staged_direct_layer_resolution_old,
            explosion_bench_support::fused_direct_layer_resolution_new,
        );
        run_allocation_case(
            "noteskin reused direct explosion layer accumulator",
            explosion_bench_support::fresh_direct_layer_accumulator_old,
            explosion_bench_support::reused_direct_layer_accumulator_new,
        );
        return;
    }
    if std::env::var_os("DEADSYNC_EXPLOSION_ONLY").is_some() {
        run_uv_case(
            "noteskin canonical explosion fade",
            explosion_bench_support::canonical_fade_state_old,
            explosion_bench_support::canonical_fade_state_new,
        );
        run_uv_case(
            "noteskin opaque-white explosion glow",
            explosion_bench_support::opaque_white_glow_old,
            explosion_bench_support::opaque_white_glow_new,
        );
        run_uv_case(
            "noteskin constant binary-color explosion glow",
            explosion_bench_support::constant_binary_glow_old,
            explosion_bench_support::constant_binary_glow_new,
        );
        return;
    }
    if std::env::var_os("DEADSYNC_RECEPTOR_MODEL_ONLY").is_some() {
        run_uv_case(
            "noteskin discarded model effect timing",
            model_draw_bench_support::discarded_model_effect_old,
            model_draw_bench_support::discarded_model_effect_new,
        );
        run_uv_case(
            "noteskin opaque-white receptor pulse",
            receptor_bench_support::opaque_white_pulse_old,
            receptor_bench_support::opaque_white_pulse_new,
        );
        run_uv_case(
            "noteskin alpha-only receptor pulse",
            receptor_bench_support::alpha_only_pulse_old,
            receptor_bench_support::alpha_only_pulse_new,
        );
        return;
    }
    if std::env::var_os("DEADSYNC_MODEL_ENDPOINT_ONLY").is_some() {
        run_uv_case(
            "noteskin single-key model auto rotation",
            model_draw_bench_support::single_key_auto_rot_old,
            model_draw_bench_support::single_key_auto_rot_new,
        );
        run_uv_case(
            "noteskin two-key model auto rotation",
            model_draw_bench_support::two_key_auto_rot_old,
            model_draw_bench_support::two_key_auto_rot_new,
        );
        run_uv_case(
            "noteskin transparent static model glow",
            model_draw_bench_support::transparent_static_glow_old,
            model_draw_bench_support::transparent_static_glow_new,
        );
        return;
    }
    if std::env::var_os("DEADSYNC_MODEL_DRAW_CACHE_ONLY").is_some() {
        run_uv_case(
            "noteskin static model draw evaluation",
            model_draw_bench_support::static_model_draw_old,
            model_draw_bench_support::static_model_draw_new,
        );
        run_uv_case(
            "noteskin canonical model effect timing",
            model_draw_bench_support::canonical_effect_mix_old,
            model_draw_bench_support::canonical_effect_mix_new,
        );
        run_uv_case(
            "noteskin cached model atlas origin",
            model_draw_bench_support::cached_model_uv_old,
            model_draw_bench_support::cached_model_uv_new,
        );
        return;
    }
    if std::env::var_os("DEADSYNC_ANIMATED_SPRITE_CACHE_ONLY").is_some() {
        run_uv_case(
            "noteskin cached weighted animation total",
            sprite_math_bench_support::cached_weighted_frame_index_old,
            sprite_math_bench_support::cached_weighted_frame_index_new,
        );
        run_uv_case(
            "noteskin uniform weighted frame arithmetic",
            sprite_math_bench_support::uniform_weighted_frame_index_old,
            sprite_math_bench_support::uniform_weighted_frame_index_new,
        );
        run_uv_case(
            "noteskin cached animated atlas UV",
            sprite_math_bench_support::cached_animated_uv_old,
            sprite_math_bench_support::cached_animated_uv_new,
        );
        return;
    }
    if std::env::var_os("DEADSYNC_SPRITE_ADDRESS_ONLY").is_some() {
        run_uv_case(
            "noteskin uniform frame wrapping",
            sprite_math_bench_support::uniform_frame_index_old,
            sprite_math_bench_support::uniform_frame_index_new,
        );
        run_uv_case(
            "noteskin cached static atlas UV",
            sprite_math_bench_support::cached_atlas_uv_old,
            sprite_math_bench_support::cached_atlas_uv_new,
        );
        run_uv_case(
            "noteskin cached atlas UV scale",
            sprite_math_bench_support::atlas_uv_old,
            sprite_math_bench_support::atlas_uv_new,
        );
        return;
    }
    if std::env::var_os("DEADSYNC_SPRITE_MATH_ONLY").is_some() {
        run_uv_case(
            "noteskin normalized animation phase",
            sprite_math_bench_support::normalized_phase_old,
            sprite_math_bench_support::normalized_phase_new,
        );
        run_uv_case(
            "noteskin horizontal-only UV scrolling",
            sprite_math_bench_support::horizontal_scroll_old,
            sprite_math_bench_support::horizontal_scroll_new,
        );
        run_uv_case(
            "noteskin vertical-only UV scrolling",
            sprite_math_bench_support::vertical_scroll_old,
            sprite_math_bench_support::vertical_scroll_new,
        );
        return;
    }
    if std::env::var_os("DEADSYNC_UV_COLOR_ONLY").is_some() {
        run_uv_case(
            "noteskin denominator color lookup",
            uv_color_bench_support::denominator_legacy,
            uv_color_bench_support::denominator_current,
        );
        run_uv_case(
            "noteskin progress color wrapping",
            uv_color_bench_support::progress_legacy,
            uv_color_bench_support::progress_current,
        );
        run_uv_case(
            "noteskin alternate-progress color wrapping",
            uv_color_bench_support::progress_alternate_legacy,
            uv_color_bench_support::progress_alternate_current,
        );
        return;
    }
    let raw = fixture();
    let queries = [
        ("", "GlobalOnly"),
        ("NoteDisplay", "TapNoteAnimationLength"),
        ("notedisplay", "RollTailNoteColorType"),
        ("Left", "Metric00"),
        ("Down", "Metric03"),
        ("Up", "Metric06"),
        ("Right", "Metric09"),
        ("Center", "Metric12"),
        ("Fallback", "Metric19"),
    ];
    const INI_OPS: usize = 256;
    let old = measure(INI_OPS, raw.lines().count(), || {
        bench_support::legacy_ini_checksum(black_box(&raw), black_box(&queries))
    });
    let new = measure(INI_OPS, raw.lines().count(), || {
        bench_support::ini_checksum(black_box(&raw), black_box(&queries))
    });
    print_pair("INI section reuse", &old, &new);

    let metrics = bench_support::parse_ini(&raw);
    assert_eq!(
        legacy_metric_checksum(&metrics),
        bench_support::metric_checksum(&metrics)
    );
    const METRIC_OPS: usize = 2_048;
    let old = measure(METRIC_OPS, NOTE_ANIM_PART_COUNT * 8, || {
        legacy_metric_checksum(black_box(&metrics))
    });
    let new = measure(METRIC_OPS, NOTE_ANIM_PART_COUNT * 8, || {
        bench_support::metric_checksum(black_box(&metrics))
    });
    print_pair("precomputed NoteDisplay keys", &old, &new);

    const CACHE_OPS: usize = 1_024;
    const CACHE_HITS: usize = 128;
    let root = Path::new("C:/DeadSync/Assets/NoteSkins");
    let old = measure(CACHE_OPS, CACHE_HITS, || {
        legacy_cache_hit_checksum(
            black_box(root),
            black_box(" DANCE "),
            black_box(" CeL "),
            CACHE_HITS,
        )
    });
    let new = measure(CACHE_OPS, CACHE_HITS, || {
        bench_support::cache_hit_checksum(
            black_box(root),
            black_box(" DANCE "),
            black_box(" CeL "),
            CACHE_HITS,
        )
    });
    print_pair("borrowed noteskin data-cache hits", &old, &new);
}
