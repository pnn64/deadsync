use deadsync_gameplay::{
    AccelOverrides, ActiveAttackRefreshInput, ActiveAttackRefreshOutput, ActiveAttackRefreshState,
    AppearanceEffects, AppearanceOverrides, AttackMaskWindow, ChartAttackEffects,
    GameplayAttackRuntimeState, MiniAttackMode, PerspectiveOverrides, ScrollEffects,
    ScrollOverrides, SongLuaEase, SongLuaEaseMaskTarget, SongLuaEaseMaskWindow,
    VisibilityOverrides, VisualEffects, VisualOverrides, partition_point_from_hint,
    refresh_active_attack_player, refresh_active_attack_player_indexed, song_lua_ease_factor,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const EASE_ITERATIONS: usize = 5_000_000;
const SEARCH_ITERATIONS: usize = 2_000_000;
const WINDOW_FRAMES: usize = 100_000;
const WINDOW_COUNT: usize = 512;
const WARMUP_DIVISOR: usize = 20;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every allocation operation delegates unchanged to `System`; the
// relaxed atomics only observe successful calls while measurement is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.deallocs.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    deallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    ns_per_item: f64,
    cycles_per_item: Option<f64>,
    items_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(iterations: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..iterations / WARMUP_DIVISOR {
        black_box(operation());
    }
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(operation()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_item: seconds * 1_000_000_000.0 / iterations as f64,
        cycles_per_item: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64),
        items_per_second: iterations as f64 / seconds,
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "{label:<25} {:>10.2} ns/item  {:>10.2} cycles/item  {:>9.2} Mitem/s  \
         {:>5} alloc  {:>5} realloc  {:>5} free  {:>8} bytes  {:016x}",
        result.ns_per_item,
        result.cycles_per_item.unwrap_or(f64::NAN),
        result.items_per_second / 1_000_000.0,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.deallocs,
        result.allocated.bytes,
        result.checksum,
    );
}

fn print_change(old: &BenchResult, new: &BenchResult) {
    println!(
        "  change: {:>7.2}% latency, {:>7.2}% cycles, {:>7.2}% throughput",
        percent_change(old.ns_per_item, new.ns_per_item),
        percent_change(
            old.cycles_per_item.unwrap_or(f64::NAN),
            new.cycles_per_item.unwrap_or(f64::NAN),
        ),
        percent_change(old.items_per_second, new.items_per_second),
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    (new / old - 1.0) * 100.0
}

fn easing_benchmark() {
    const NAMES: [&str; 8] = [
        "linear",
        "inOutQuad",
        "outCubic",
        "inOutSine",
        "outExpo",
        "inOutCirc",
        "outBack",
        "outBounce",
    ];
    let compiled = NAMES.map(|name| SongLuaEase::from_name(Some(name)));
    let mut old_index = 0usize;
    let mut old_t = 0.0f32;
    let old = measure(EASE_ITERATIONS, || {
        old_index = (old_index + 1) & (NAMES.len() - 1);
        old_t = (old_t + 0.000_173).fract();
        u64::from(
            song_lua_ease_factor(
                Some(black_box(NAMES[old_index])),
                black_box(old_t),
                Some(1.25),
                Some(0.2),
            )
            .to_bits(),
        )
    });
    let mut new_index = 0usize;
    let mut new_t = 0.0f32;
    let new = measure(EASE_ITERATIONS, || {
        new_index = (new_index + 1) & (compiled.len() - 1);
        new_t = (new_t + 0.000_173).fract();
        u64::from(
            black_box(compiled[new_index])
                .factor(black_box(new_t), Some(1.25), Some(0.2))
                .to_bits(),
        )
    });
    assert_eq!(old.checksum, new.checksum);
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    let count = 512usize;
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let legacy_storage = (0..count)
        .map(|index| Some(NAMES[index & (NAMES.len() - 1)].to_string()))
        .collect::<Vec<_>>();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let legacy_alloc = ALLOC.snapshot().delta(before);
    let legacy_bytes = legacy_storage.capacity() * std::mem::size_of::<Option<String>>()
        + legacy_storage
            .iter()
            .flatten()
            .map(String::capacity)
            .sum::<usize>();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let compiled_storage = (0..count)
        .map(|index| compiled[index & (compiled.len() - 1)])
        .collect::<Vec<_>>();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let compiled_alloc = ALLOC.snapshot().delta(before);
    let compiled_bytes = compiled_storage.capacity() * std::mem::size_of::<SongLuaEase>();
    black_box((&legacy_storage, &compiled_storage));

    println!("compiled Song-Lua easing ({EASE_ITERATIONS} samples)");
    print_result("old string dispatch", &old);
    print_result("new compiled enum", &new);
    print_change(&old, &new);
    println!(
        "  512-window storage: old={} B / {} allocs, new={} B / {} allocs ({:.2}% bytes)",
        legacy_bytes,
        legacy_alloc.allocs,
        compiled_bytes,
        compiled_alloc.allocs,
        percent_change(legacy_bytes as f64, compiled_bytes as f64),
    );
}

fn legacy_partition_point_from_hint<T>(
    values: &[T],
    hint: usize,
    mut predicate: impl FnMut(&T) -> bool,
) -> usize {
    let cursor = hint.min(values.len());
    if cursor < values.len() && predicate(&values[cursor]) {
        let next = cursor + 1;
        if next == values.len() || !predicate(&values[next]) {
            return next;
        }
    } else if cursor > 0 && !predicate(&values[cursor - 1]) {
        let previous = cursor - 1;
        if previous == 0 || predicate(&values[previous - 1]) {
            return previous;
        }
    } else {
        return cursor;
    }
    values.partition_point(predicate)
}

fn search_benchmark() {
    let values = (0..8_192u32).map(|value| value * 3).collect::<Vec<_>>();
    let mut old_hint = 0usize;
    let mut old_target = 0u32;
    let old = measure(SEARCH_ITERATIONS, || {
        old_target = (old_target + 12) % 24_576;
        old_hint = legacy_partition_point_from_hint(&values, old_hint, |&value| {
            value < black_box(old_target)
        });
        old_hint as u64
    });
    let mut new_hint = 0usize;
    let mut new_target = 0u32;
    let new = measure(SEARCH_ITERATIONS, || {
        new_target = (new_target + 12) % 24_576;
        new_hint =
            partition_point_from_hint(&values, new_hint, |&value| value < black_box(new_target));
        new_hint as u64
    });
    assert_eq!(old.checksum, new.checksum);
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!("\nadaptive visible/judgment boundary search ({SEARCH_ITERATIONS} queries)");
    print_result("old one-step hint", &old);
    print_result("new exponential hint", &new);
    print_change(&old, &new);
}

fn mask_window(index: usize) -> AttackMaskWindow {
    let start_second = index as f32 * 0.25;
    AttackMaskWindow {
        start_second,
        end_second: start_second + 0.4,
        sustain_end_second: start_second + 0.4,
        persist_after_end: false,
        clear_all: false,
        chart: ChartAttackEffects::default(),
        accel: AccelOverrides::default(),
        visual: VisualOverrides {
            drunk: Some((index % 100) as f32 / 100.0),
            ..VisualOverrides::default()
        },
        visual_speed: VisualOverrides::default(),
        appearance: AppearanceOverrides::default(),
        appearance_speed: AppearanceOverrides::default(),
        visibility: VisibilityOverrides::default(),
        scroll: ScrollOverrides::default(),
        scroll_approach_speed: ScrollOverrides::default(),
        perspective: PerspectiveOverrides::default(),
        scroll_speed: None,
        mini_percent: None,
        mini_mode: MiniAttackMode::Absolute,
        mini_speed: None,
    }
}

fn ease_window(index: usize) -> SongLuaEaseMaskWindow {
    let start_second = index as f32 * 0.25;
    SongLuaEaseMaskWindow {
        start_second,
        end_second: start_second + 0.4,
        sustain_end_second: start_second + 0.4,
        target: SongLuaEaseMaskTarget::PlayerRotationZ,
        from: index as f32,
        to: index as f32 + 45.0,
        easing: SongLuaEase::InOutQuad,
        opt1: None,
        opt2: None,
    }
}

fn refresh_state() -> ActiveAttackRefreshState {
    ActiveAttackRefreshState {
        attack_current_appearance: AppearanceEffects::default(),
        active_attack_visual: VisualOverrides::default(),
        active_attack_visibility: VisibilityOverrides::default(),
        active_attack_scroll: ScrollOverrides::default(),
        active_attack_mini_percent: None,
        outro_attack_visual: VisualOverrides::default(),
    }
}

fn next_refresh_state(output: ActiveAttackRefreshOutput) -> ActiveAttackRefreshState {
    ActiveAttackRefreshState {
        attack_current_appearance: output.attack_current_appearance,
        active_attack_visual: output.active_attack_visual,
        active_attack_visibility: output.active_attack_visibility,
        active_attack_scroll: output.active_attack_scroll,
        active_attack_mini_percent: output.active_attack_mini_percent,
        outro_attack_visual: output.outro_attack_visual,
    }
}

fn refresh_checksum(output: ActiveAttackRefreshOutput) -> u64 {
    let visual = output
        .active_attack_visual
        .drunk
        .unwrap_or_default()
        .to_bits();
    let rotation = output
        .player_transform
        .rotation_z
        .unwrap_or_default()
        .to_bits();
    (u64::from(visual) << 32) | u64::from(rotation)
}

fn frame_time(frame: usize) -> f32 {
    let song_seconds = WINDOW_COUNT as f32 * 0.25 + 1.0;
    (frame as f32 / 120.0) % song_seconds
}

fn window_benchmark() {
    let masks = (0..WINDOW_COUNT).map(mask_window).collect::<Vec<_>>();
    let eases = (0..WINDOW_COUNT).map(ease_window).collect::<Vec<_>>();
    let mut old_state = refresh_state();
    let mut old_frame = 0usize;
    let old = measure(WINDOW_FRAMES, || {
        let now = frame_time(old_frame);
        old_frame += 1;
        let output = refresh_active_attack_player(
            ActiveAttackRefreshInput {
                now,
                delta_time: 1.0 / 120.0,
                attacks_cleared_for_outro: false,
                base_appearance: AppearanceEffects::default(),
                base_visual: VisualEffects::default(),
                base_scroll: ScrollEffects::default(),
                base_mini_percent: 0.0,
                attack_windows: &masks,
                song_lua_ease_windows: &eases,
            },
            old_state,
        );
        old_state = next_refresh_state(output);
        refresh_checksum(output)
    });

    let mut indexed = GameplayAttackRuntimeState::new(
        std::array::from_fn(|player| {
            if player == 0 {
                masks.clone()
            } else {
                Vec::new()
            }
        }),
        std::array::from_fn(|player| {
            if player == 0 {
                eases.clone()
            } else {
                Vec::new()
            }
        }),
    );
    let mut new_state = refresh_state();
    let mut new_frame = 0usize;
    let new = measure(WINDOW_FRAMES, || {
        let now = frame_time(new_frame);
        new_frame += 1;
        indexed.update_window_indices(0, now);
        let (mask_indices, ease_indices) = indexed.active_window_indices(0);
        let output = refresh_active_attack_player_indexed(
            ActiveAttackRefreshInput {
                now,
                delta_time: 1.0 / 120.0,
                attacks_cleared_for_outro: false,
                base_appearance: AppearanceEffects::default(),
                base_visual: VisualEffects::default(),
                base_scroll: ScrollEffects::default(),
                base_mini_percent: 0.0,
                attack_windows: &indexed.mask_windows[0],
                song_lua_ease_windows: &indexed.song_lua_ease_windows[0],
            },
            new_state,
            mask_indices,
            ease_indices,
        );
        new_state = next_refresh_state(output);
        refresh_checksum(output)
    });
    assert_eq!(old.checksum, new.checksum);
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!(
        "\nactive attack/ease window evaluation ({WINDOW_COUNT} + {WINDOW_COUNT} windows, \
         {WINDOW_FRAMES} frames)"
    );
    print_result("old full scan", &old);
    print_result("new active index", &new);
    print_change(&old, &new);
    let (mask_stats, ease_stats) = indexed.window_index_stats(0).expect("player index");
    println!(
        "  index stats: masks max_active={} rebuilds={}, eases max_active={} rebuilds={}",
        mask_stats.max_active,
        mask_stats.time_rebuilds,
        ease_stats.max_active,
        ease_stats.time_rebuilds,
    );
}

fn assert_zero_alloc(result: &BenchResult) {
    assert_eq!(result.allocated.allocs, 0);
    assert_eq!(result.allocated.reallocs, 0);
    assert_eq!(result.allocated.deallocs, 0);
    assert_eq!(result.allocated.bytes, 0);
}

fn main() {
    easing_benchmark();
    search_benchmark();
    window_benchmark();
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}
