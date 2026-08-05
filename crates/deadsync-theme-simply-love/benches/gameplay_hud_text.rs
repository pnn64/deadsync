use deadlib_present::actors::TextContent;
use deadsync_theme_simply_love::screens::components::gameplay::gameplay_stats::{
    benchmark_game_time, benchmark_game_time_cached, benchmark_game_time_legacy,
    benchmark_judgment_rows, benchmark_judgment_rows_legacy, benchmark_live_timing,
    benchmark_live_timing_cached, benchmark_live_timing_legacy, benchmark_padded_runs,
    benchmark_padded_runs_cached, benchmark_padded_runs_legacy,
};
use deadsync_theme_simply_love::screens::components::gameplay::notefield::{
    benchmark_combo_text, benchmark_combo_text_legacy, benchmark_error_bar_label,
    benchmark_error_bar_label_legacy, benchmark_offset_ms, benchmark_offset_ms_legacy,
    prepare_combo_text_benchmark,
};
use deadsync_theme_simply_love::screens::gameplay::{
    GameplayHudTextBenchmarkCache, GameplayHudTextBenchmarkSnapshot,
    benchmark_gameplay_hud_text_legacy,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 20_000;
const MEASURE_FRAMES: usize = 2_000_000;
const SMX_SENSOR_VALUES_PER_FRAME: usize = 8;
const COMBO_VALUES_PER_FRAME: usize = 2;
const DYNAMIC_TEXT_OPS: usize = 1_000_000;
const TIMING_SONG_FRAMES: usize = 24_000;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: all operations delegate to `System` with their original layouts;
// the atomics only observe successful allocation calls.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied this layout to the global allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the pointer/layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

fn inputs(frame: usize) -> (f64, f32) {
    let bpm = if frame % 20_000 < 10_000 {
        150.0
    } else {
        175.25
    };
    let life = if frame % 120 < 60 { 87.3 } else { 85.2 };
    (bpm, life)
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    checksum: usize,
}

fn checksum(snapshot: &GameplayHudTextBenchmarkSnapshot) -> usize {
    snapshot
        .bpm
        .len()
        .wrapping_add(snapshot.life.len())
        .wrapping_add(snapshot.overlay.len())
        .wrapping_add(snapshot.overlay_line_count)
}

fn measure(mut frame: impl FnMut(usize) -> GameplayHudTextBenchmarkSnapshot) -> BenchResult {
    for index in 0..WARMUP_FRAMES {
        let snapshot = frame(index);
        assert_eq!(snapshot.overlay.as_ref(), "AutoPlay");
    }
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut output_checksum = 0usize;
    for index in 0..MEASURE_FRAMES {
        output_checksum = output_checksum.wrapping_add(checksum(&black_box(frame(index))));
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum: output_checksum,
    }
}

fn smx_sensor_value(frame: usize, slot: usize) -> u16 {
    ((frame * 17 + slot * 61) % 501) as u16
}

fn text_checksum(text: &TextContent) -> usize {
    text_checksum_str(text.as_str())
}

fn text_checksum_str(text: &str) -> usize {
    text.bytes().fold(0usize, |checksum, byte| {
        checksum.rotate_left(5) ^ byte as usize
    })
}

fn measure_dynamic_text(mut text: impl FnMut(usize) -> usize) -> BenchResult {
    for index in 0..WARMUP_FRAMES {
        black_box(text(index));
    }
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut output_checksum = 0usize;
    for index in 0..DYNAMIC_TEXT_OPS {
        output_checksum = output_checksum.rotate_left(7) ^ black_box(text(index));
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum: output_checksum,
    }
}

fn measure_text_frames(frames: usize, mut text: impl FnMut(usize) -> usize) -> BenchResult {
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut output_checksum = 0usize;
    for index in 0..frames {
        output_checksum = output_checksum.rotate_left(7) ^ black_box(text(index));
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum: output_checksum,
    }
}

fn measure_smx_sensor_text(mut text: impl FnMut(u16) -> TextContent) -> BenchResult {
    for frame in 0..WARMUP_FRAMES {
        for slot in 0..SMX_SENSOR_VALUES_PER_FRAME {
            let value = smx_sensor_value(frame, slot);
            assert_eq!(text(value).as_str(), value.to_string());
        }
    }
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut output_checksum = 0usize;
    for frame in 0..MEASURE_FRAMES {
        for slot in 0..SMX_SENSOR_VALUES_PER_FRAME {
            let value = black_box(smx_sensor_value(frame, slot));
            output_checksum =
                output_checksum.rotate_left(7) ^ text_checksum(&black_box(text(value)));
        }
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum: output_checksum,
    }
}

fn combo_value(frame: usize, player: usize) -> u32 {
    8_193 + (((frame / 12) % 4_096) * COMBO_VALUES_PER_FRAME + player) as u32
}

fn measure_combo_text(mut text: impl FnMut(u32) -> TextContent) -> BenchResult {
    for frame in 0..WARMUP_FRAMES {
        for player in 0..COMBO_VALUES_PER_FRAME {
            let value = combo_value(frame, player);
            assert_eq!(text(value).as_str(), value.to_string());
        }
    }
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut output_checksum = 0usize;
    for frame in 0..MEASURE_FRAMES {
        for player in 0..COMBO_VALUES_PER_FRAME {
            let value = black_box(combo_value(frame, player));
            output_checksum =
                output_checksum.rotate_left(7) ^ text_checksum(&black_box(text(value)));
        }
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum: output_checksum,
    }
}

fn measure_prompt_text(mut text: impl FnMut(&Arc<str>) -> TextContent) -> BenchResult {
    let prompts: [Arc<str>; 3] = [
        Arc::from("Continue holding START to give up"),
        Arc::from("Continue holding BACK to give up"),
        Arc::from("Don't go back"),
    ];
    for frame in 0..WARMUP_FRAMES {
        let source = &prompts[frame % prompts.len()];
        assert_eq!(text(source).as_str(), source.as_ref());
    }
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut output_checksum = 0usize;
    for frame in 0..MEASURE_FRAMES {
        let source = black_box(&prompts[frame % prompts.len()]);
        output_checksum = output_checksum.rotate_left(7) ^ text_checksum(&black_box(text(source)));
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum: output_checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    print_result_for(label, result, MEASURE_FRAMES);
}

fn print_result_for(label: &str, result: &BenchResult, operations: usize) {
    let frames = operations as f64;
    println!(
        "{label:<13} {:>9.2} ns/frame  {:>8.0} cycles/frame  {:>10.0} frames/s  \
         {:>5.2} allocs/frame  {:>7.1} bytes/frame  {:>5.2} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / frames,
        result.cycles as f64 / frames,
        frames / result.elapsed.as_secs_f64(),
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
        result.allocated.reallocs as f64 / frames,
    );
}

fn main() {
    let legacy = measure(|frame| {
        let (bpm, life) = inputs(frame);
        benchmark_gameplay_hud_text_legacy(bpm, true, life, "AutoPlay")
    });
    let mut cache = GameplayHudTextBenchmarkCache::new("AutoPlay");
    let optimized = measure(|frame| {
        let (bpm, life) = inputs(frame);
        let snapshot = cache.snapshot(bpm, true, life);
        assert_eq!(
            snapshot.bpm.as_ref(),
            if bpm == 150.0 { "150" } else { "175.25" }
        );
        assert_eq!(
            snapshot.life.as_ref(),
            if life == 87.3 { "87.3%" } else { "85.2%" }
        );
        assert_eq!(snapshot.overlay.as_ref(), "AutoPlay");
        assert_eq!(snapshot.overlay_line_count, 1);
        snapshot
    });
    black_box((legacy.checksum, optimized.checksum));

    println!("gameplay HUD text benchmark");
    print_result("legacy frame", &legacy);
    print_result("cached frame", &optimized);

    let owned_sensor_text = measure_smx_sensor_text(|value| TextContent::from(value.to_string()));
    let inline_sensor_text = measure_smx_sensor_text(TextContent::inline_u16);
    assert_eq!(owned_sensor_text.checksum, inline_sensor_text.checksum);
    black_box((owned_sensor_text.checksum, inline_sensor_text.checksum));

    println!(
        "\nSMX gameplay sensor text benchmark \
         ({SMX_SENSOR_VALUES_PER_FRAME} live values/frame)"
    );
    print_result("owned values", &owned_sensor_text);
    print_result("inline values", &inline_sensor_text);

    prepare_combo_text_benchmark();
    let saturated_combo_cache = measure_combo_text(benchmark_combo_text_legacy);
    prepare_combo_text_benchmark();
    let inline_combo_text = measure_combo_text(benchmark_combo_text);
    assert_eq!(saturated_combo_cache.checksum, inline_combo_text.checksum);
    black_box((saturated_combo_cache.checksum, inline_combo_text.checksum));

    println!(
        "\nsaturated combo text benchmark \
         ({COMBO_VALUES_PER_FRAME} players, combo changes every 12 frames)"
    );
    print_result("bounded cache", &saturated_combo_cache);
    print_result("inline values", &inline_combo_text);

    let owned_prompt = measure_prompt_text(|text| TextContent::Owned(text.to_string()));
    let shared_prompt = measure_prompt_text(|text| TextContent::Shared(Arc::clone(text)));
    assert_eq!(owned_prompt.checksum, shared_prompt.checksum);
    black_box((owned_prompt.checksum, shared_prompt.checksum));

    println!("\nactive gameplay exit-prompt text benchmark");
    print_result("owned prompt", &owned_prompt);
    print_result("shared prompt", &shared_prompt);

    let offset_legacy = measure_dynamic_text(|index| {
        let centi_ms = (index % 36_001) as i32 - 18_000;
        text_checksum_str(benchmark_offset_ms_legacy(centi_ms as f32 * 0.01).as_ref())
    });
    let offset_inline = measure_dynamic_text(|index| {
        let centi_ms = (index % 36_001) as i32 - 18_000;
        text_checksum(&benchmark_offset_ms(centi_ms as f32 * 0.01))
    });
    assert_eq!(offset_legacy.checksum, offset_inline.checksum);

    println!("\nsaturated gameplay offset-ms text benchmark");
    print_result_for("legacy cache", &offset_legacy, DYNAMIC_TEXT_OPS);
    print_result_for("inline value", &offset_inline, DYNAMIC_TEXT_OPS);

    let label_legacy = measure_dynamic_text(|index| {
        let early = index & 1 == 0;
        let scaled = index & 2 == 0;
        text_checksum_str(benchmark_error_bar_label_legacy(early, scaled).as_ref())
    });
    let label_static = measure_dynamic_text(|index| {
        let early = index & 1 == 0;
        let scaled = index & 2 == 0;
        text_checksum(&benchmark_error_bar_label(early, scaled))
    });
    assert_eq!(label_legacy.checksum, label_static.checksum);

    println!("\nwarmed gameplay error-label benchmark");
    print_result_for("legacy cache", &label_legacy, DYNAMIC_TEXT_OPS);
    print_result_for("static value", &label_static, DYNAMIC_TEXT_OPS);

    let judgment_labels: [Arc<str>; 6] = [
        Arc::from("Fantastic"),
        Arc::from("Excellent"),
        Arc::from("Great"),
        Arc::from("Decent"),
        Arc::from("Way Off"),
        Arc::from("Miss"),
    ];
    let rows_legacy =
        measure_dynamic_text(|_| benchmark_judgment_rows_legacy(black_box(&judgment_labels)));
    let rows_direct =
        measure_dynamic_text(|_| benchmark_judgment_rows(black_box(&judgment_labels)));
    assert_eq!(rows_legacy.checksum, rows_direct.checksum);

    println!("\nstandard step-stats judgment-row benchmark");
    print_result_for("heap rows", &rows_legacy, DYNAMIC_TEXT_OPS);
    print_result_for("direct rows", &rows_direct, DYNAMIC_TEXT_OPS);

    let padded_legacy = measure_dynamic_text(|index| {
        let count = 8_193 + (index % 100_000) as u32;
        let (dim, bright) = benchmark_padded_runs_legacy(count, 6);
        text_checksum_str(dim.as_ref()) ^ text_checksum_str(bright.as_ref())
    });
    let padded_inline = measure_dynamic_text(|index| {
        let count = 8_193 + (index % 100_000) as u32;
        let (dim, bright) = benchmark_padded_runs(count, 6);
        text_checksum(&dim) ^ text_checksum(&bright)
    });
    assert_eq!(padded_legacy.checksum, padded_inline.checksum);

    println!("\nsaturated padded judgment text benchmark");
    print_result_for("legacy miss", &padded_legacy, DYNAMIC_TEXT_OPS);
    print_result_for("inline value", &padded_inline, DYNAMIC_TEXT_OPS);

    let clock_legacy = measure_dynamic_text(|index| {
        text_checksum_str(benchmark_game_time_legacy(601 + index as u32, 1).as_ref())
    });
    let clock_inline =
        measure_dynamic_text(|index| text_checksum(&benchmark_game_time(601 + index as u32, 1)));
    assert_eq!(clock_legacy.checksum, clock_inline.checksum);

    println!("\nfirst-use gameplay clock text benchmark");
    print_result_for("legacy miss", &clock_legacy, DYNAMIC_TEXT_OPS);
    print_result_for("inline value", &clock_inline, DYNAMIC_TEXT_OPS);

    let timing_legacy = measure_dynamic_text(|index| {
        let recent = (index % 4_001) as f32 * 0.1 - 200.0;
        let all = (index.wrapping_mul(17) % 4_001) as f32 * 0.1 - 200.0;
        text_checksum_str(benchmark_live_timing_legacy(recent, all).as_ref())
    });
    let timing_inline = measure_dynamic_text(|index| {
        let recent = (index % 4_001) as f32 * 0.1 - 200.0;
        let all = (index.wrapping_mul(17) % 4_001) as f32 * 0.1 - 200.0;
        text_checksum(&benchmark_live_timing(recent, all))
    });
    assert_eq!(timing_legacy.checksum, timing_inline.checksum);

    println!("\nsaturated live timing-pair text benchmark");
    print_result_for("legacy miss", &timing_legacy, DYNAMIC_TEXT_OPS);
    print_result_for("inline value", &timing_inline, DYNAMIC_TEXT_OPS);

    let padded_key = (42u32, 4u8);
    let padded_hit = measure_dynamic_text(|_| {
        let (dim, bright) = benchmark_padded_runs_cached(padded_key.0, padded_key.1 as usize);
        text_checksum_str(dim.as_ref()) ^ text_checksum_str(bright.as_ref())
    });
    let padded_inline_steady = measure_dynamic_text(|_| {
        let (dim, bright) = benchmark_padded_runs(padded_key.0, padded_key.1 as usize);
        text_checksum(&dim) ^ text_checksum(&bright)
    });

    let clock_key = (599u32, 2u8);
    let clock_hit = measure_dynamic_text(|_| {
        let text = benchmark_game_time_cached(clock_key.0, clock_key.1);
        text_checksum_str(text.as_ref())
    });
    let clock_inline_steady =
        measure_dynamic_text(|_| text_checksum(&benchmark_game_time(clock_key.0, clock_key.1)));

    let timing_key = (12.3f32, -5.7f32);
    let timing_hit = measure_dynamic_text(|_| {
        let text = benchmark_live_timing_cached(timing_key.0, timing_key.1);
        text_checksum_str(text.as_ref())
    });
    let timing_inline_steady =
        measure_dynamic_text(|_| text_checksum(&benchmark_live_timing(timing_key.0, timing_key.1)));

    println!("\nwarmed repeated-value CPU guard (allocation-free on both paths)");
    print_result_for("padded hit", &padded_hit, DYNAMIC_TEXT_OPS);
    print_result_for("padded inline", &padded_inline_steady, DYNAMIC_TEXT_OPS);
    print_result_for("clock hit", &clock_hit, DYNAMIC_TEXT_OPS);
    print_result_for("clock inline", &clock_inline_steady, DYNAMIC_TEXT_OPS);
    print_result_for("timing hit", &timing_hit, DYNAMIC_TEXT_OPS);
    print_result_for("timing inline", &timing_inline_steady, DYNAMIC_TEXT_OPS);

    let timing_song_legacy = measure_text_frames(TIMING_SONG_FRAMES, |frame| {
        let update = frame / 6;
        let recent = update as f32 * 0.1 - 200.0;
        let all = update as f32 * -0.07 + 120.0;
        text_checksum_str(benchmark_live_timing_cached(recent, all).as_ref())
    });
    let timing_song_inline = measure_text_frames(TIMING_SONG_FRAMES, |frame| {
        let update = frame / 6;
        let recent = update as f32 * 0.1 - 200.0;
        let all = update as f32 * -0.07 + 120.0;
        text_checksum(&benchmark_live_timing(recent, all))
    });
    assert_eq!(timing_song_legacy.checksum, timing_song_inline.checksum);

    println!("\nlive timing song workload (value changes every 6 frames)");
    print_result_for("legacy cache", &timing_song_legacy, TIMING_SONG_FRAMES);
    print_result_for("inline slots", &timing_song_inline, TIMING_SONG_FRAMES);
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: LFENCE/RDTSC only serialize and read this thread's timestamp
    // counter; they do not dereference memory.
    unsafe {
        core::arch::x86_64::_mm_lfence();
        let cycles = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        cycles
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}
