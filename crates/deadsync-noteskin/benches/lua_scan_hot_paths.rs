use deadsync_noteskin::actor::{
    itg_has_beat_fade_glow_signature_for_bench,
    itg_has_beat_fade_glow_signature_reference_for_bench, itg_has_beat_update_marker_for_bench,
    itg_has_beat_update_marker_reference_for_bench, itg_update_function_name_for_bench,
    itg_update_function_name_reference_for_bench,
};
use deadsync_noteskin::compiled::compiled_key_bench_support::{
    actor_file_visit_current, actor_file_visit_reference, actor_manifest_current,
    actor_manifest_reference, actor_visit_current, actor_visit_reference,
};
use deadsync_noteskin::compiler::compiler_bench_support::{
    cache_key_current, cache_key_reference, source_label_current, source_label_reference,
    source_order_current, source_order_reference,
};
use deadsync_noteskin::itg::{IniData, NoteskinData};
use deadsync_noteskin::lua::{
    itg_extract_quoted_strings_reference_for_bench, itg_parse_self_chain_commands,
    itg_parse_self_chain_commands_reference_for_bench, itg_quoted_strings,
};
use deadsync_noteskin::model::model_scan_bench_support::{
    animated_texture_keys_current, animated_texture_keys_reference, auto_rot_keys_current,
    auto_rot_keys_reference, derived_texture_stem_current, derived_texture_stem_reference,
    extension_kind_current, extension_kind_reference, material_nomove_current,
    material_nomove_reference, milkshape_signature_current, milkshape_signature_reference,
};
use deadsync_noteskin::runtime::{
    itg_has_receptor_actor_effect_command_for_bench,
    itg_has_receptor_actor_effect_command_reference_for_bench,
    itg_is_common_fallback_hold_explosion_key,
    itg_is_common_fallback_hold_explosion_key_reference_for_bench, itg_is_common_noteskin_key,
    itg_is_common_noteskin_key_reference_for_bench,
};
use deadsync_noteskin::script::{
    bench_support::{
        borrowed_argument_slices_new, borrowed_blend_scan_new, borrowed_color_wrapper_new,
        classified_command_new, classified_effect_clock_new, classified_judgment_key_new,
        classified_vertalign_new, fixed_rgba_components_new, heap_argument_storage_old,
        heap_rgba_components_old, inline_argument_storage_new, lowercase_blend_scan_old,
        lowercase_color_wrapper_old, lowercase_command_old, lowercase_effect_clock_old,
        lowercase_judgment_key_old, lowercase_vertalign_old, owned_argument_slices_old,
    },
    parse_linear_frames_expr, parse_linear_frames_expr_reference_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 31;
type LinearFrames = Option<(usize, Vec<f32>)>;
type LinearParser = fn(&str) -> LinearFrames;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: all requests are delegated unchanged to `System`; the relaxed
// counters observe only this single-threaded benchmark while gated.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.freed_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: the pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the allocator request is forwarded unchanged to `System`.
        let new_ptr = unsafe { System.realloc(ptr, old, new_size) };
        if !new_ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(new_size as u64, Ordering::Relaxed);
            self.freed_bytes
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
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            freed_bytes: self.freed_bytes - before.freed_bytes,
        }
    }

    const fn churn(self) -> u64 {
        self.allocated_bytes + self.freed_bytes
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

fn timed_sample(ops: usize, op: &mut impl FnMut() -> u64) -> (f64, Option<f64>, u64) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..ops {
        checksum = checksum.wrapping_add(black_box(op()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    (
        elapsed.as_secs_f64() * 1e9 / ops as f64,
        cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / ops as f64),
        checksum,
    )
}

fn allocation_sample(ops: usize, op: &mut impl FnMut() -> u64) -> (AllocSnapshot, u64) {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut checksum = 0u64;
    for _ in 0..ops {
        checksum = checksum.wrapping_add(black_box(op()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    (ALLOC.snapshot().delta(before), checksum)
}

fn measure_pair(
    ops: usize,
    items: usize,
    mut old_op: impl FnMut() -> u64,
    mut new_op: impl FnMut() -> u64,
) -> (Row, Row) {
    for _ in 0..8 {
        black_box(old_op());
        black_box(new_op());
    }

    let mut old_times = Vec::with_capacity(SAMPLES);
    let mut new_times = Vec::with_capacity(SAMPLES);
    let mut old_cycles = Vec::with_capacity(SAMPLES);
    let mut new_cycles = Vec::with_capacity(SAMPLES);
    let mut old_checksum = 0u64;
    let mut new_checksum = 0u64;
    for sample in 0..SAMPLES {
        let ((old_ns, old_cycle, old_sum), (new_ns, new_cycle, new_sum)) = if sample % 2 == 0 {
            let old = timed_sample(ops, &mut old_op);
            let new = timed_sample(ops, &mut new_op);
            (old, new)
        } else {
            let new = timed_sample(ops, &mut new_op);
            let old = timed_sample(ops, &mut old_op);
            (old, new)
        };
        old_times.push(old_ns);
        new_times.push(new_ns);
        if let Some(cycles) = old_cycle {
            old_cycles.push(cycles);
        }
        if let Some(cycles) = new_cycle {
            new_cycles.push(cycles);
        }
        old_checksum ^= old_sum;
        new_checksum ^= new_sum;
    }
    old_times.sort_by(f64::total_cmp);
    new_times.sort_by(f64::total_cmp);
    old_cycles.sort_by(f64::total_cmp);
    new_cycles.sort_by(f64::total_cmp);

    let (old_alloc, old_alloc_sum) = allocation_sample(ops, &mut old_op);
    let (new_alloc, new_alloc_sum) = allocation_sample(ops, &mut new_op);
    old_checksum ^= old_alloc_sum;
    new_checksum ^= new_alloc_sum;

    let row = |times: Vec<f64>, cycles: Vec<f64>, alloc, checksum| Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc,
        checksum,
        ops,
        items,
    };
    (
        row(old_times, old_cycles, old_alloc, old_checksum),
        row(new_times, new_cycles, new_alloc, new_checksum),
    )
}

fn mix(checksum: u64, value: u64) -> u64 {
    checksum.wrapping_mul(1_099_511_628_211).wrapping_add(value)
}

fn text_checksum(mut checksum: u64, value: &str) -> u64 {
    checksum = mix(checksum, value.len() as u64);
    value
        .bytes()
        .fold(checksum, |sum, byte| mix(sum, u64::from(byte)))
}

fn linear_checksum(value: LinearFrames) -> u64 {
    let Some((frames, delays)) = value else {
        return 1;
    };
    delays.iter().fold(frames as u64 + 2, |checksum, delay| {
        mix(checksum, u64::from(delay.to_bits()))
    })
}

const fn bool_checksum(value: bool) -> u64 {
    if value { 2 } else { 1 }
}

fn assert_improved(name: &str, old: &Row, new: &Row) {
    assert_eq!(old.checksum, new.checksum, "{name} behavior diverged");
    assert!(
        new.median_ns < old.median_ns,
        "{name} median regressed: old={} ns new={} ns",
        old.median_ns,
        new.median_ns
    );
    assert!(
        new.p95_ns < old.p95_ns,
        "{name} p95 regressed: old={} ns new={} ns",
        old.p95_ns,
        new.p95_ns
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(
            new_cycles < old_cycles,
            "{name} cycles regressed: old={old_cycles} new={new_cycles}"
        );
    }
    assert!(
        new.alloc.allocs < old.alloc.allocs,
        "{name} allocs did not fall"
    );
    assert!(new.alloc.reallocs <= old.alloc.reallocs);
    assert!(
        new.alloc.frees < old.alloc.frees,
        "{name} frees did not fall"
    );
    assert!(new.alloc.allocated_bytes < old.alloc.allocated_bytes);
    assert!(new.alloc.churn() < old.alloc.churn());
}

fn print_pair(name: &str, old: &Row, new: &Row) {
    println!("{name}");
    print_row("old", old);
    print_row("new", new);
    println!(
        "  change: {:+.2}% median  {:+.2}% p95  {:+.2}% cycles  {:+.2}% throughput  {:+.2}% allocs  {:+.2}% churn",
        change(old.median_ns, new.median_ns),
        change(old.p95_ns, new.p95_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN)
        ),
        change(throughput(old), throughput(new)),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, row: &Row) {
    let divisor = row.ops as f64;
    println!(
        "  {label:<3} {:>10.1} ns  p95 {:>10.1} ns  {:>10.1} cycles  {:>12.0} item/s  \
         {:>7.1} alloc  {:>6.1} realloc  {:>7.1} free  {:>10.1} allocated B  {:>10.1} churn B/op",
        row.median_ns,
        row.p95_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        throughput(row),
        row.alloc.allocs as f64 / divisor,
        row.alloc.reallocs as f64 / divisor,
        row.alloc.frees as f64 / divisor,
        row.alloc.allocated_bytes as f64 / divisor,
        row.alloc.churn() as f64 / divisor,
    );
}

fn throughput(row: &Row) -> f64 {
    row.items as f64 * 1e9 / row.median_ns
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

fn main() {
    const REPEATS: usize = 32;

    let token_cases = [
        "Diffuse, 1, 0.5, 0.25, 1",
        "EffectTiming, 0.2, 0.1, 0.3, 0.05, 0.4",
        "SetStateProperties, Sprite.LinearFrames(8, 0.5)",
        "PlayCommand, 'Ready, Set'",
        "Blend, BlendMode_Add",
        "UnknownCommand, nested(1, 2), { 3, 4 }, 'five,six'",
    ];
    let token_suite = |parse: fn(&str) -> u64| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for token in token_cases {
                checksum = mix(checksum, black_box(parse(black_box(token))));
            }
        }
        checksum
    };

    let (old_borrowing, new_borrowing) = measure_pair(
        512,
        token_cases.len() * REPEATS,
        || token_suite(owned_argument_slices_old),
        || token_suite(borrowed_argument_slices_new),
    );
    assert_improved(
        "borrowed command argument slices",
        &old_borrowing,
        &new_borrowing,
    );
    print_pair(
        "borrowed command argument slices (6 representative tokens)",
        &old_borrowing,
        &new_borrowing,
    );

    let (old_storage, new_storage) = measure_pair(
        512,
        token_cases.len() * REPEATS,
        || token_suite(heap_argument_storage_old),
        || token_suite(inline_argument_storage_new),
    );
    assert_improved(
        "inline command argument storage",
        &old_storage,
        &new_storage,
    );
    print_pair(
        "inline command argument storage (6 representative tokens)",
        &old_storage,
        &new_storage,
    );

    let command_cases = [
        "Diffuse",
        "EFFECTTIMING",
        "setStateProperties",
        "AddRotationZ",
        "SetTextureFiltering",
        "Visible",
    ];
    let command_suite = |parse: fn(&str) -> u64| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for command in command_cases {
                checksum = mix(checksum, black_box(parse(black_box(command))));
            }
        }
        checksum
    };
    let (old_commands, new_commands) = measure_pair(
        1_024,
        command_cases.len() * REPEATS,
        || command_suite(lowercase_command_old),
        || command_suite(classified_command_new),
    );
    assert_improved(
        "allocation-free command classification",
        &old_commands,
        &new_commands,
    );
    print_pair(
        "allocation-free command classification (6 mixed-case commands)",
        &old_commands,
        &new_commands,
    );

    let color_wrapper_cases = [
        "COLOR('1,0.5,0.25,1')",
        "color(\"#ff008080\")",
        " Color(0.25,0.5,0.75,1) ",
        "#00ff00",
        "0.1,0.2,0.3,0.4",
        "Colorful(1,2,3,4)",
        "Cölör(1,2,3,4)",
    ];
    let color_wrapper_suite = |parse: fn(&str) -> u64| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for color in color_wrapper_cases {
                checksum = mix(checksum, black_box(parse(black_box(color))));
            }
        }
        checksum
    };
    let (old_wrapper, new_wrapper) = measure_pair(
        1_024,
        color_wrapper_cases.len() * REPEATS,
        || color_wrapper_suite(lowercase_color_wrapper_old),
        || color_wrapper_suite(borrowed_color_wrapper_new),
    );
    assert_improved(
        "allocation-free Color wrapper recognition",
        &old_wrapper,
        &new_wrapper,
    );
    assert_eq!(new_wrapper.alloc.allocs, 0);
    assert_eq!(new_wrapper.alloc.reallocs, 0);
    assert_eq!(new_wrapper.alloc.frees, 0);
    assert_eq!(new_wrapper.alloc.churn(), 0);
    print_pair(
        "allocation-free Color wrapper recognition (7 representative values)",
        &old_wrapper,
        &new_wrapper,
    );

    let judgment_color_cases = [
        "JudgmentLineToColor('judgmentline_w1')",
        "judgmentlinetocolor(\"JUDGMENTLINE_W3\")",
        "JUDGMENTLINETOCOLOR('JudgmentLine_W5')",
        "JudgmentLineToColor('judgmentline_held')",
        "JudgmentLineToColor('judgmentline_miss')",
        "JudgmentLineToColor('judgmentline_maxcombo')",
        "JudgmentLineToStrokeColor('JUDGMENTLINE_W2')",
        "JudgmentLineToColor('unknown')",
    ];
    let judgment_color_suite = |parse: fn(&str) -> u64| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for color in judgment_color_cases {
                checksum = mix(checksum, black_box(parse(black_box(color))));
            }
        }
        checksum
    };
    let (old_judgment, new_judgment) = measure_pair(
        1_024,
        judgment_color_cases.len() * REPEATS,
        || judgment_color_suite(lowercase_judgment_key_old),
        || judgment_color_suite(classified_judgment_key_new),
    );
    assert_improved(
        "allocation-free judgment color classification",
        &old_judgment,
        &new_judgment,
    );
    assert_eq!(new_judgment.alloc.allocs, 0);
    assert_eq!(new_judgment.alloc.reallocs, 0);
    assert_eq!(new_judgment.alloc.frees, 0);
    assert_eq!(new_judgment.alloc.churn(), 0);
    print_pair(
        "allocation-free judgment color classification (8 representative calls)",
        &old_judgment,
        &new_judgment,
    );

    let rgba_cases = [
        "1,0.5,0.25,1",
        "bad,1,0.5,0.25,1",
        "0,0.1,0.2,0.3,99,100",
        "(1/2),(3/4),0.25,1",
        "1,2,3",
        "bad,worse,1,2,3,4",
    ];
    let rgba_suite = |parse: fn(&str) -> u64| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for color in rgba_cases {
                checksum = mix(checksum, black_box(parse(black_box(color))));
            }
        }
        checksum
    };
    let (old_rgba, new_rgba) = measure_pair(
        1_024,
        rgba_cases.len() * REPEATS,
        || rgba_suite(heap_rgba_components_old),
        || rgba_suite(fixed_rgba_components_new),
    );
    assert_improved("fixed-size RGBA component parsing", &old_rgba, &new_rgba);
    assert_eq!(new_rgba.alloc.allocs, 0);
    assert_eq!(new_rgba.alloc.reallocs, 0);
    assert_eq!(new_rgba.alloc.frees, 0);
    assert_eq!(new_rgba.alloc.churn(), 0);
    print_pair(
        "fixed-size RGBA component parsing (6 representative lists)",
        &old_rgba,
        &new_rgba,
    );

    let vertalign_cases = [
        "top",
        "MIDDLE",
        "'Center'",
        "\"BOTTOM\"",
        "0.375",
        "unknown",
        "Cënter",
    ];
    let vertalign_suite = |parse: fn(&str) -> u64| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for value in vertalign_cases {
                checksum = mix(checksum, black_box(parse(black_box(value))));
            }
        }
        checksum
    };
    let (old_vertalign, new_vertalign) = measure_pair(
        1_024,
        vertalign_cases.len() * REPEATS,
        || vertalign_suite(lowercase_vertalign_old),
        || vertalign_suite(classified_vertalign_new),
    );
    assert_improved(
        "allocation-free vertical alignment parsing",
        &old_vertalign,
        &new_vertalign,
    );
    assert_eq!(new_vertalign.alloc.allocs, 0);
    assert_eq!(new_vertalign.alloc.reallocs, 0);
    assert_eq!(new_vertalign.alloc.frees, 0);
    assert_eq!(new_vertalign.alloc.churn(), 0);
    print_pair(
        "allocation-free vertical alignment parsing (7 representative values)",
        &old_vertalign,
        &new_vertalign,
    );

    let blend_cases = [
        "BlendMode_Add",
        "prefixBLENDMODE_ADDsuffix",
        "Blend.Add",
        "prefix.bLeNd.AdD.suffix",
        "BlendMode_Normal",
        "add",
        "BlëndMode_Add",
    ];
    let blend_suite = |scan: fn(&str) -> u64| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for value in blend_cases {
                checksum = mix(checksum, black_box(scan(black_box(value))));
            }
        }
        checksum
    };
    let (old_blend, new_blend) = measure_pair(
        1_024,
        blend_cases.len() * REPEATS,
        || blend_suite(lowercase_blend_scan_old),
        || blend_suite(borrowed_blend_scan_new),
    );
    assert_improved(
        "allocation-free additive blend scan",
        &old_blend,
        &new_blend,
    );
    assert_eq!(new_blend.alloc.allocs, 0);
    assert_eq!(new_blend.alloc.reallocs, 0);
    assert_eq!(new_blend.alloc.frees, 0);
    assert_eq!(new_blend.alloc.churn(), 0);
    print_pair(
        "allocation-free additive blend scan (7 representative values)",
        &old_blend,
        &new_blend,
    );

    let effect_clock_cases = [
        "beat",
        "'BEATNOOFFSET'",
        "\"Bgm\"",
        "timer",
        "Time",
        "MUSICNOOFFSET",
        "customBeatClock",
        "seconds",
        "unknown",
    ];
    let effect_clock_suite = |parse: fn(&str) -> u64| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for value in effect_clock_cases {
                checksum = mix(checksum, black_box(parse(black_box(value))));
            }
        }
        checksum
    };
    let (old_clock, new_clock) = measure_pair(
        1_024,
        effect_clock_cases.len() * REPEATS,
        || effect_clock_suite(lowercase_effect_clock_old),
        || effect_clock_suite(classified_effect_clock_new),
    );
    assert_improved(
        "allocation-free effect clock parsing",
        &old_clock,
        &new_clock,
    );
    assert_eq!(new_clock.alloc.allocs, 0);
    assert_eq!(new_clock.alloc.reallocs, 0);
    assert_eq!(new_clock.alloc.frees, 0);
    assert_eq!(new_clock.alloc.churn(), 0);
    print_pair(
        "allocation-free effect clock parsing (9 representative values)",
        &old_clock,
        &new_clock,
    );

    let linear_cases = [
        "Sprite.LinearFrames(64,(64/60))",
        "Sprite.LinearFrames(4, 1.5)",
        "sprite.linearframes((8),(2/4))",
        "Sprite.LinearFrames(1, 0.25, ignored)",
        "Sprite.LinearFrames(0, -1)",
        "Sprite.LinearFrames(1)",
        "Other.LinearFrames(4, 1)",
    ];
    let linear_suite = |parse: LinearParser| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for expression in linear_cases {
                checksum = mix(
                    checksum,
                    linear_checksum(black_box(parse(black_box(expression)))),
                );
            }
        }
        checksum
    };
    let (old_linear, new_linear) = measure_pair(
        256,
        linear_cases.len() * REPEATS,
        || linear_suite(parse_linear_frames_expr_reference_for_bench),
        || linear_suite(parse_linear_frames_expr),
    );
    assert_improved(
        "borrowed LinearFrames argument scan",
        &old_linear,
        &new_linear,
    );
    print_pair(
        "borrowed LinearFrames argument scan (7 representative expressions)",
        &old_linear,
        &new_linear,
    );

    let chain_cases = [
        "self:zoom(1):diffuse(1, 0.5, 0, 1):xy(12, 34)",
        "function(self) self:x(10); self:y(20):sleep(0.5) end",
        "self:playcommand('Ready,Set'):visible():rotationz(90)",
        "self:queuecommand(\"On\"):linear(0.25):zoomto(64, 64)",
        "self:broken; self:rotationz(90)",
        "no actor commands",
    ];
    let chain_suite = |normalize: fn(&str) -> Option<String>| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for script in chain_cases {
                let command = black_box(normalize(black_box(script)));
                checksum = match command {
                    Some(command) => text_checksum(checksum, &command),
                    None => mix(checksum, 1),
                };
            }
        }
        checksum
    };
    let (old_chains, new_chains) = measure_pair(
        256,
        chain_cases.len() * REPEATS,
        || chain_suite(itg_parse_self_chain_commands_reference_for_bench),
        || chain_suite(itg_parse_self_chain_commands),
    );
    assert_improved("direct self-chain assembly", &old_chains, &new_chains);
    print_pair(
        "direct self-chain assembly (6 representative scripts)",
        &old_chains,
        &new_chains,
    );

    let quoted_cases = [
        "NOTESKIN:GetPath('Down', 'Tap Note')",
        "LoadActor(\"Fallback Receptor\") .. { Texture='Down Tap Note' }",
        "'one' \"two\" 'three' \"four\" 'five' \"six\"",
        "prefix 'unterminated",
        "'' \"\" 'final'",
        "no quoted paths",
    ];
    let old_quoted_suite = || {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for source in quoted_cases {
                for value in itg_extract_quoted_strings_reference_for_bench(black_box(source)) {
                    checksum = text_checksum(checksum, black_box(&value));
                }
            }
        }
        checksum
    };
    let new_quoted_suite = || {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for source in quoted_cases {
                for value in itg_quoted_strings(black_box(source)) {
                    checksum = text_checksum(checksum, black_box(value));
                }
            }
        }
        checksum
    };
    let (old_quoted, new_quoted) = measure_pair(
        256,
        quoted_cases.len() * REPEATS,
        old_quoted_suite,
        new_quoted_suite,
    );
    assert_improved("borrowed quoted-path scan", &old_quoted, &new_quoted);
    assert_eq!(new_quoted.alloc.allocs, 0);
    assert_eq!(new_quoted.alloc.reallocs, 0);
    assert_eq!(new_quoted.alloc.frees, 0);
    assert_eq!(new_quoted.alloc.churn(), 0);
    print_pair(
        "borrowed quoted-path scan (6 representative sources)",
        &old_quoted,
        &new_quoted,
    );

    let update_marker_cases = [
        "InitCommand=cmd(SetUpdateFunction,Beat);",
        "OnCommand = cmd( SET UPDATE FUNCTION , Pulse );",
        "prefix\u{2003}SET\nUPDATE\tFUNCTION,Glow suffix",
        "InitCommand=cmd(SetUpdateFunction);",
        "InitCommand=cmd(SetUpdaterFunction,Beat);",
        "local function SetUpdateFunctionAlias(self) end",
    ];
    let marker_suite = |scan: fn(&str) -> bool| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for source in update_marker_cases {
                checksum = mix(checksum, bool_checksum(black_box(scan(black_box(source)))));
            }
        }
        checksum
    };
    let (old_marker, new_marker) = measure_pair(
        256,
        update_marker_cases.len() * REPEATS,
        || marker_suite(itg_has_beat_update_marker_reference_for_bench),
        || marker_suite(itg_has_beat_update_marker_for_bench),
    );
    assert_improved(
        "allocation-free update-marker scan",
        &old_marker,
        &new_marker,
    );
    assert_eq!(new_marker.alloc.allocs, 0);
    assert_eq!(new_marker.alloc.reallocs, 0);
    assert_eq!(new_marker.alloc.frees, 0);
    assert_eq!(new_marker.alloc.churn(), 0);
    print_pair(
        "allocation-free update-marker scan (6 representative actor fragments)",
        &old_marker,
        &new_marker,
    );

    let update_function_cases = [
        "beat);",
        "pulse_2,",
        "9tick",
        "_update)",
        "missingcallbackend",
        "--nocallback",
    ];
    let function_suite = |scan: fn(&str) -> Option<u64>| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for source in update_function_cases {
                checksum = mix(
                    checksum,
                    black_box(scan(black_box(source))).unwrap_or(u64::MAX),
                );
            }
        }
        checksum
    };
    let (old_function, new_function) = measure_pair(
        256,
        update_function_cases.len() * REPEATS,
        || function_suite(itg_update_function_name_reference_for_bench),
        || function_suite(itg_update_function_name_for_bench),
    );
    assert_improved(
        "allocation-free update-function key extraction",
        &old_function,
        &new_function,
    );
    assert_eq!(new_function.alloc.allocs, 0);
    assert_eq!(new_function.alloc.reallocs, 0);
    assert_eq!(new_function.alloc.frees, 0);
    assert_eq!(new_function.alloc.churn(), 0);
    print_pair(
        "allocation-free update-function key extraction (6 representative tails)",
        &old_function,
        &new_function,
    );

    let beat_fade_cases = [
        "part=beat%1; part=clamp(part,0,0.5); eff=scale(part,0,0.5,1,0); \
         this.Glow:diffusealpha(eff)",
        "PART = BEAT % 1\u{2003}PART = CLAMP(PART, 0, 0.5)\n\
         EFF = SCALE(PART, 0, 0.5, 1, 0)\nTHIS.GLOW : DIFFUSEALPHA(EFF)",
        "local part=beat%1; part=clamp(part,0,0.5); eff=scale(part,0,0.5,1,0)",
        "part=beat%1; part=clamp(part,0,1); eff=scale(part,0,0.5,1,0); \
         this.Glow:diffusealpha(eff)",
        "part=beat%2; part=clamp(part,0,0.5); eff=scale(part,0,0.5,1,0); \
         this.Glow:diffusealpha(eff)",
        "ordinary update body without the pump beat fade pattern",
    ];
    let signature_suite = |scan: fn(&str) -> bool| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for body in beat_fade_cases {
                checksum = mix(checksum, bool_checksum(black_box(scan(black_box(body)))));
            }
        }
        checksum
    };
    let (old_signature, new_signature) = measure_pair(
        256,
        beat_fade_cases.len() * REPEATS,
        || signature_suite(itg_has_beat_fade_glow_signature_reference_for_bench),
        || signature_suite(itg_has_beat_fade_glow_signature_for_bench),
    );
    assert_improved(
        "allocation-free beat-fade body signature scan",
        &old_signature,
        &new_signature,
    );
    assert_eq!(new_signature.alloc.allocs, 0);
    assert_eq!(new_signature.alloc.reallocs, 0);
    assert_eq!(new_signature.alloc.frees, 0);
    assert_eq!(new_signature.alloc.churn(), 0);
    print_pair(
        "allocation-free beat-fade body signature scan (6 representative bodies)",
        &old_signature,
        &new_signature,
    );

    let common_key_cases = [
        "NoteSkins/common/common/Fallback Hold Explosion.png",
        "prefix/NOTESKINS/COMMON/COMMON/FALLBACK HOLD EXPLOSION.png",
        "noteskins/common/common/Fallback Receptor.png",
        "noteskins/dance/default/Down Hold Explosion.png",
        "nöteskins/common/common/fallback hold explosion",
        "",
    ];
    let common_key_suite = |classify: fn(&str) -> bool| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for key in common_key_cases {
                checksum = mix(checksum, bool_checksum(black_box(classify(black_box(key)))));
            }
        }
        checksum
    };
    let (old_fallback_key, new_fallback_key) = measure_pair(
        1_024,
        common_key_cases.len() * REPEATS,
        || common_key_suite(itg_is_common_fallback_hold_explosion_key_reference_for_bench),
        || common_key_suite(itg_is_common_fallback_hold_explosion_key),
    );
    assert_improved(
        "allocation-free fallback explosion key scan",
        &old_fallback_key,
        &new_fallback_key,
    );
    assert_eq!(new_fallback_key.alloc.allocs, 0);
    assert_eq!(new_fallback_key.alloc.reallocs, 0);
    assert_eq!(new_fallback_key.alloc.frees, 0);
    assert_eq!(new_fallback_key.alloc.churn(), 0);
    print_pair(
        "allocation-free fallback explosion key scan (6 representative keys)",
        &old_fallback_key,
        &new_fallback_key,
    );

    let (old_common_key, new_common_key) = measure_pair(
        1_024,
        common_key_cases.len() * REPEATS,
        || common_key_suite(itg_is_common_noteskin_key_reference_for_bench),
        || common_key_suite(itg_is_common_noteskin_key),
    );
    assert_improved(
        "allocation-free common noteskin key scan",
        &old_common_key,
        &new_common_key,
    );
    assert_eq!(new_common_key.alloc.allocs, 0);
    assert_eq!(new_common_key.alloc.reallocs, 0);
    assert_eq!(new_common_key.alloc.frees, 0);
    assert_eq!(new_common_key.alloc.churn(), 0);
    print_pair(
        "allocation-free common noteskin key scan (6 representative keys)",
        &old_common_key,
        &new_common_key,
    );

    let receptor_effect_cases = [
        "effectclock,bgm;DiffuseRamp",
        "linear,0.2;DIFFUSESHIFT",
        "sleep,0.1;glOwShIfT",
        "prefixdiffuseshiftsuffix",
        "diffuse,1,1,1,1",
        "glöwshift",
        "",
    ];
    let receptor_effect_suite = |scan: fn(&str) -> bool| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for command in receptor_effect_cases {
                checksum = mix(checksum, bool_checksum(black_box(scan(black_box(command)))));
            }
        }
        checksum
    };
    let (old_receptor_effect, new_receptor_effect) = measure_pair(
        1_024,
        receptor_effect_cases.len() * REPEATS,
        || receptor_effect_suite(itg_has_receptor_actor_effect_command_reference_for_bench),
        || receptor_effect_suite(itg_has_receptor_actor_effect_command_for_bench),
    );
    assert_improved(
        "allocation-free receptor actor-effect scan",
        &old_receptor_effect,
        &new_receptor_effect,
    );
    assert_eq!(new_receptor_effect.alloc.allocs, 0);
    assert_eq!(new_receptor_effect.alloc.reallocs, 0);
    assert_eq!(new_receptor_effect.alloc.frees, 0);
    assert_eq!(new_receptor_effect.alloc.churn(), 0);
    print_pair(
        "allocation-free receptor actor-effect scan (7 representative commands)",
        &old_receptor_effect,
        &new_receptor_effect,
    );

    let actor_visit_cases = [
        ("Down", "Tap Note"),
        ("PUMP-CENTER", "Hold Head Active"),
        ("UpLeft", "Tap Explosion Bright W1"),
        ("MenuLeft", "Fallback Hold Explosion"),
        ("Café", "Éclair"),
        ("", ""),
    ];
    let actor_visit_suite = |build: fn(&str, &str) -> String| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for (button, element) in actor_visit_cases {
                let key = black_box(build(black_box(button), black_box(element)));
                checksum = text_checksum(checksum, black_box(&key));
            }
        }
        checksum
    };
    let (old_actor_visit, new_actor_visit) = measure_pair(
        1_024,
        actor_visit_cases.len() * REPEATS,
        || actor_visit_suite(actor_visit_reference),
        || actor_visit_suite(actor_visit_current),
    );
    assert_improved(
        "single-buffer actor recursion keys",
        &old_actor_visit,
        &new_actor_visit,
    );
    print_pair(
        "single-buffer actor recursion keys (6 representative pairs)",
        &old_actor_visit,
        &new_actor_visit,
    );

    let actor_file_cases = [
        Path::new("Dance/Default/Down Receptor.lua"),
        Path::new("NoteSkins/Pump/CENTER Tap Note.LUA"),
        Path::new("NoteSkins/Dance/Cel/Down Tap Mine.MODEL"),
        Path::new("ProgramData/ITGmania/NoteSkins/Common/Fallback.lua"),
        Path::new("Skins/Café/Éclair.lua"),
        Path::new(""),
    ];
    let actor_file_suite = |build: fn(&Path) -> String| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for path in actor_file_cases {
                let key = black_box(build(black_box(path)));
                checksum = text_checksum(checksum, black_box(&key));
            }
        }
        checksum
    };
    let (old_actor_file, new_actor_file) = measure_pair(
        1_024,
        actor_file_cases.len() * REPEATS,
        || actor_file_suite(actor_file_visit_reference),
        || actor_file_suite(actor_file_visit_current),
    );
    assert_improved(
        "single-buffer actor file recursion keys",
        &old_actor_file,
        &new_actor_file,
    );
    print_pair(
        "single-buffer actor file recursion keys (6 representative paths)",
        &old_actor_file,
        &new_actor_file,
    );

    let manifest_cases = [
        (
            Path::new("assets/noteskins/DANCE/DeFaUlT"),
            Path::new("assets/noteskins/DANCE/DeFaUlT/DOWN RECEPTOR.LUA"),
        ),
        (
            Path::new("assets/noteskins/Pump/CmdStack"),
            Path::new("assets/noteskins/Pump/CmdStack/Center Tap Note.lua"),
        ),
        (
            Path::new("ProgramData/ITGmania/NoteSkins/Common/Fallback"),
            Path::new("ProgramData/ITGmania/NoteSkins/Common/Fallback/Receptor.lua"),
        ),
        (
            Path::new("root/NoteSkins/PuMp/Café"),
            Path::new("root/NoteSkins/PuMp/Café/Éclair.lua"),
        ),
        (Path::new("dance/default"), Path::new("Tap Note.lua")),
    ];
    let manifest_suite = |build: fn(&Path, &Path) -> Option<String>| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for (dir, path) in manifest_cases {
                checksum = match black_box(build(black_box(dir), black_box(path))) {
                    Some(key) => text_checksum(checksum, black_box(&key)),
                    None => mix(checksum, 1),
                };
            }
        }
        checksum
    };
    let (old_manifest, new_manifest) = measure_pair(
        1_024,
        manifest_cases.len() * REPEATS,
        || manifest_suite(actor_manifest_reference),
        || manifest_suite(actor_manifest_current),
    );
    assert_improved(
        "single-buffer actor manifest keys",
        &old_manifest,
        &new_manifest,
    );
    print_pair(
        "single-buffer actor manifest keys (5 representative paths)",
        &old_manifest,
        &new_manifest,
    );

    let extension_cases = [
        "png", "PNG", "JpG", "jpeg", "BMP", "Gif", "WEBP", "ini", "INI", "txt", "model", "", "café",
    ];
    let extension_suite = |classify: fn(&str) -> u8| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for ext in extension_cases {
                checksum = mix(checksum, u64::from(black_box(classify(black_box(ext)))) + 1);
            }
        }
        checksum
    };
    let (old_extensions, new_extensions) = measure_pair(
        1_024,
        extension_cases.len() * REPEATS,
        || extension_suite(extension_kind_reference),
        || extension_suite(extension_kind_current),
    );
    assert_improved(
        "borrowed model texture extension classification",
        &old_extensions,
        &new_extensions,
    );
    print_pair(
        "borrowed model texture extension classification (13 representative extensions)",
        &old_extensions,
        &new_extensions,
    );

    let stem_cases = [
        "Down Tap Note Model",
        "Center Hold MODEL",
        "Up Lift model",
        "FallbackModel",
        "model",
        "Café Model",
        "Arrow",
        "",
    ];
    let stem_suite = |derive: fn(&str) -> String| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for stem in stem_cases {
                let derived = black_box(derive(black_box(stem)));
                checksum = text_checksum(checksum, black_box(&derived));
            }
        }
        checksum
    };
    let (old_stems, new_stems) = measure_pair(
        1_024,
        stem_cases.len() * REPEATS,
        || stem_suite(derived_texture_stem_reference),
        || stem_suite(derived_texture_stem_current),
    );
    assert_improved(
        "single-buffer model texture stem derivation",
        &old_stems,
        &new_stems,
    );
    print_pair(
        "single-buffer model texture stem derivation (8 representative stems)",
        &old_stems,
        &new_stems,
    );

    let material_cases = [
        "material",
        "NoMove",
        "tap NOMOVE glow",
        "xnomovey",
        "no move",
        "nømove",
        "",
        "  MixedNoMove  ",
    ];
    let material_suite = |parse: fn(&str) -> bool| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for line in material_cases {
                checksum = mix(checksum, bool_checksum(black_box(parse(black_box(line)))));
            }
        }
        checksum
    };
    let (old_materials, new_materials) = measure_pair(
        1_024,
        material_cases.len() * REPEATS,
        || material_suite(material_nomove_reference),
        || material_suite(material_nomove_current),
    );
    assert_improved(
        "borrowed model material flag scan",
        &old_materials,
        &new_materials,
    );
    print_pair(
        "borrowed model material flag scan (8 representative names)",
        &old_materials,
        &new_materials,
    );

    let signature_cases = [
        format!("{}mIlKsHaPe 3D aScIi\nMeshes: 1", "x".repeat(400)),
        format!("{}MILKSHAPE 3D ASCII", "x".repeat(247)),
        format!("{}not a model", "x".repeat(768)),
        format!("{}not a model", "cafÃ©".repeat(192)),
    ];
    let signature_suite = |scan: fn(&str) -> bool| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for content in &signature_cases {
                checksum = mix(checksum, bool_checksum(black_box(scan(black_box(content)))));
            }
        }
        checksum
    };
    let (old_signatures, new_signatures) = measure_pair(
        1_024,
        signature_cases.len() * REPEATS,
        || signature_suite(milkshape_signature_reference),
        || signature_suite(milkshape_signature_current),
    );
    assert_improved(
        "allocation-free full MilkShape signature scan",
        &old_signatures,
        &new_signatures,
    );
    print_pair(
        "allocation-free full MilkShape signature scan (4 long model sources)",
        &old_signatures,
        &new_signatures,
    );

    let animated_key_indices = [0, 1, 9, 10, 99, 100, 999];
    let animated_key_suite = |build: fn(usize) -> u64| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for index in animated_key_indices {
                checksum = mix(checksum, black_box(build(black_box(index))));
            }
        }
        checksum
    };
    let (old_animated_keys, new_animated_keys) = measure_pair(
        1_024,
        animated_key_indices.len() * REPEATS,
        || animated_key_suite(animated_texture_keys_reference),
        || animated_key_suite(animated_texture_keys_current),
    );
    assert_improved(
        "stack-built animated texture INI keys",
        &old_animated_keys,
        &new_animated_keys,
    );
    print_pair(
        "stack-built animated texture INI keys (7 frame/delay pairs)",
        &old_animated_keys,
        &new_animated_keys,
    );

    let auto_rotations = [
        (230.0, -725.0),
        (220.0, 725.0),
        (210.0, 540.0),
        (200.0, -540.0),
        (190.0, 365.0),
        (180.0, -365.0),
        (170.0, 185.0),
        (160.0, -185.0),
        (150.0, 350.0),
        (140.0, -350.0),
        (130.0, 181.0),
        (120.0, -181.0),
        (110.0, 90.0),
        (100.0, -90.0),
        (90.0, 270.0),
        (80.0, -270.0),
        (70.0, 450.0),
        (60.0, -450.0),
        (50.0, 630.0),
        (40.0, -630.0),
        (30.0, 10.0),
        (20.0, -10.0),
        (10.0, 5.0),
        (0.0, 0.0),
    ];
    let auto_rot_suite = |build: fn(&[(f32, f32)]) -> u64| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            checksum = mix(checksum, black_box(build(black_box(&auto_rotations))));
        }
        checksum
    };
    let (old_auto_rot, new_auto_rot) = measure_pair(
        1_024,
        auto_rotations.len() * REPEATS,
        || auto_rot_suite(auto_rot_keys_reference),
        || auto_rot_suite(auto_rot_keys_current),
    );
    assert_improved(
        "single-buffer model auto-rotation keys",
        &old_auto_rot,
        &new_auto_rot,
    );
    print_pair(
        "single-buffer model auto-rotation keys (24 first-bone rotations)",
        &old_auto_rot,
        &new_auto_rot,
    );

    let cache_key_cases = [
        (" Dance ", " Default "),
        ("PUMP", "CeL"),
        ("techno", "Café"),
        (" KBX ", " Delta "),
        ("", ""),
    ];
    let cache_key_suite = |build: fn(&str, &str) -> String| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for (game, skin) in cache_key_cases {
                let key = black_box(build(black_box(game), black_box(skin)));
                checksum = text_checksum(checksum, black_box(&key));
            }
        }
        checksum
    };
    let (old_cache_keys, new_cache_keys) = measure_pair(
        1_024,
        cache_key_cases.len() * REPEATS,
        || cache_key_suite(cache_key_reference),
        || cache_key_suite(cache_key_current),
    );
    assert_improved(
        "single-buffer compiler hash-cache keys",
        &old_cache_keys,
        &new_cache_keys,
    );
    print_pair(
        "single-buffer compiler hash-cache keys (5 representative pairs)",
        &old_cache_keys,
        &new_cache_keys,
    );

    let compiler_dir = PathBuf::from("Assets")
        .join("NoteSkins")
        .join("DaNcE")
        .join("DeFaUlT");
    let compiler_data = NoteskinData {
        name: "Default".to_string(),
        metrics: IniData::default(),
        search_dirs: vec![compiler_dir.clone()],
    };
    let compiler_paths = vec![
        compiler_dir.join("Zeta.lua"),
        compiler_dir.join("alpha.lua"),
        compiler_dir.join("Down Receptor.LUA"),
        compiler_dir.join("Café Tap Note.lua"),
        compiler_dir.join("NoteSkin.lua"),
        compiler_dir.join("metrics.ini"),
        PathBuf::from("External\\Fallback Actor.lua"),
        PathBuf::from("ProgramData/ITGmania/Global.lua"),
    ];
    let source_label_suite = |build: fn(&NoteskinData, &Path) -> String| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for path in &compiler_paths {
                let label = black_box(build(black_box(&compiler_data), black_box(path.as_path())));
                checksum = text_checksum(checksum, black_box(&label));
            }
        }
        checksum
    };
    let (old_source_labels, new_source_labels) = measure_pair(
        1_024,
        compiler_paths.len() * REPEATS,
        || source_label_suite(source_label_reference),
        || source_label_suite(source_label_current),
    );
    assert_improved(
        "single-buffer compiler source labels",
        &old_source_labels,
        &new_source_labels,
    );
    print_pair(
        "single-buffer compiler source labels (8 representative paths)",
        &old_source_labels,
        &new_source_labels,
    );

    let source_order_suite = |order: fn(&NoteskinData, &[PathBuf]) -> u64| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            checksum = mix(
                checksum,
                black_box(order(black_box(&compiler_data), black_box(&compiler_paths))),
            );
        }
        checksum
    };
    let (old_source_order, new_source_order) = measure_pair(
        1_024,
        compiler_paths.len() * REPEATS,
        || source_order_suite(source_order_reference),
        || source_order_suite(source_order_current),
    );
    assert_improved(
        "cached compiler source-label ordering",
        &old_source_order,
        &new_source_order,
    );
    print_pair(
        "cached compiler source-label ordering (8 representative paths)",
        &old_source_order,
        &new_source_order,
    );
}
