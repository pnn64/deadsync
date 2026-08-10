use deadsync_core::input::MAX_PLAYERS;
use deadsync_gameplay::{
    AccelOverrides, ActiveAttackRefreshInput, ActiveAttackRefreshOutput, ActiveAttackRefreshState,
    AppearanceEffects, AppearanceOverrides, AttackBaseEffects, AttackMaskWindow,
    CROSSOVER_CUE_SEEK_GUARD_SECONDS, ChartAttackEffects, ColumnCue, ColumnCueColumn,
    ColumnCueColumns, GameplayAttackRuntimeState, GameplayCueRuntimeState, MiniAttackMode,
    PerspectiveOverrides, ScrollEffects, ScrollOverrides, SongLuaEase, SongLuaEaseMaskTarget,
    SongLuaEaseMaskWindow, SongLuaNoteHideWindowRuntime, SongLuaNoteHideWindows,
    SongLuaPlayerTransform, VisibilityOverrides, VisualEffects, VisualOverrides,
    column_cue_cursor_from_hint, partition_point_from_hint, refresh_active_attack_player,
    refresh_active_attack_player_indexed, refresh_active_attack_player_indexed_reference,
    row_entry_index_for_note, song_lua_ease_factor, song_lua_note_hidden,
    song_lua_note_hidden_reference,
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
const IDLE_ATTACK_FRAMES: usize = 2_000_000;
const SETTLED_STATE_FRAMES: usize = 1_000_000;
const SETTLED_STATE_SAMPLES: usize = 7;
const NOTE_HIDE_QUERIES: usize = 2_000_000;
const ROW_LOOKUPS: usize = 3_000_000;
const ROW_LOOKUP_SAMPLES: usize = 7;
const ROW_TYPICAL_COUNT: usize = 512;
const ROW_LARGE_COUNT: usize = 32_768;
// The local 2,234-chart corpus has a median 1.49 row slots per note. Two notes
// per populated row with a stride of three reproduces that density.
const ROW_STRIDE: usize = 3;
const CUE_BUILD_ITERATIONS: usize = 100_000;
const CUE_LOOKUPS: usize = 2_000_000;
const CUE_COUNT: usize = 4_096;
const CUE_ANCHOR_FRAMES: usize = 2_000_000;
const CUE_ANCHOR_SAMPLES: usize = 7;
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

fn idle_attack_benchmark() {
    let masks = (0..WINDOW_COUNT).map(mask_window).collect::<Vec<_>>();
    let eases = (0..WINDOW_COUNT).map(ease_window).collect::<Vec<_>>();
    let indexed = GameplayAttackRuntimeState::new(
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
    let (mask_indices, ease_indices) = indexed.active_window_indices(0);
    let input = || ActiveAttackRefreshInput {
        now: -10.0,
        delta_time: 1.0 / 120.0,
        attacks_cleared_for_outro: false,
        base_appearance: AppearanceEffects::default(),
        base_visual: VisualEffects::default(),
        base_scroll: ScrollEffects::default(),
        base_mini_percent: 0.0,
        attack_windows: &indexed.mask_windows[0],
        song_lua_ease_windows: &indexed.song_lua_ease_windows[0],
    };
    let reference = measure(IDLE_ATTACK_FRAMES, || {
        refresh_checksum(refresh_active_attack_player_indexed_reference(
            input(),
            refresh_state(),
            mask_indices,
            ease_indices,
        ))
    });
    let optimized = measure(IDLE_ATTACK_FRAMES, || {
        refresh_checksum(refresh_active_attack_player_indexed(
            input(),
            refresh_state(),
            mask_indices,
            ease_indices,
        ))
    });
    assert_eq!(reference.checksum, optimized.checksum);
    assert_zero_alloc(&reference);
    assert_zero_alloc(&optimized);

    println!("\nsettled idle attack evaluation ({WINDOW_COUNT} + {WINDOW_COUNT} compiled windows)");
    print_result("old selected pipeline", &reference);
    print_result("new settled fast path", &optimized);
    print_change(&reference, &optimized);
}

fn legacy_refresh_attack_state(
    attacks: &mut GameplayAttackRuntimeState,
    now: f32,
    delta_time: f32,
    base: AttackBaseEffects,
) -> SongLuaPlayerTransform {
    attacks.update_window_indices(0, now);
    let (attack_window_indices, ease_window_indices) = attacks.active_window_indices(0);
    let output = refresh_active_attack_player_indexed(
        ActiveAttackRefreshInput {
            now,
            delta_time,
            attacks_cleared_for_outro: attacks.cleared_for_outro,
            base_appearance: base.appearance,
            base_visual: base.visual,
            base_scroll: base.scroll,
            base_mini_percent: base.mini_percent,
            attack_windows: &attacks.mask_windows[0],
            song_lua_ease_windows: &attacks.song_lua_ease_windows[0],
        },
        ActiveAttackRefreshState {
            attack_current_appearance: attacks.current_appearance[0],
            active_attack_visual: attacks.visual[0],
            active_attack_visibility: attacks.visibility[0],
            active_attack_scroll: attacks.scroll[0],
            active_attack_mini_percent: attacks.mini_percent[0],
            outro_attack_visual: attacks.outro_visual[0],
        },
        attack_window_indices,
        ease_window_indices,
    );
    attacks.target_appearance[0] = output.attack_target_appearance;
    attacks.speed_appearance[0] = output.attack_speed_appearance;
    attacks.current_appearance[0] = output.attack_current_appearance;
    attacks.outro_visual[0] = output.outro_attack_visual;
    attacks.clear_all[0] = output.active_attack_clear_all;
    attacks.chart[0] = output.active_attack_chart;
    attacks.accel[0] = output.active_attack_accel;
    attacks.visual[0] = output.active_attack_visual;
    attacks.appearance[0] = output.active_attack_appearance;
    attacks.visibility[0] = output.active_attack_visibility;
    attacks.scroll[0] = output.active_attack_scroll;
    attacks.perspective[0] = output.active_attack_perspective;
    attacks.scroll_speed[0] = output.active_attack_scroll_speed;
    attacks.mini_percent[0] = output.active_attack_mini_percent;
    output.player_transform.resolve()
}

fn measure_legacy_attack_state(
    base: AttackBaseEffects,
) -> (
    BenchResult,
    GameplayAttackRuntimeState,
    SongLuaPlayerTransform,
) {
    let mut old_state = GameplayAttackRuntimeState::default();
    let mut old_transform = legacy_refresh_attack_state(&mut old_state, 0.0, 1.0 / 120.0, base);
    let result = measure(SETTLED_STATE_FRAMES, || {
        old_transform =
            legacy_refresh_attack_state(black_box(&mut old_state), 0.0, 1.0 / 120.0, base);
        black_box(&old_state);
        u64::from(old_transform.zoom_x.to_bits())
    });
    (result, old_state, old_transform)
}

fn measure_settled_attack_state(
    base: AttackBaseEffects,
) -> (
    BenchResult,
    GameplayAttackRuntimeState,
    SongLuaPlayerTransform,
) {
    let mut new_state = GameplayAttackRuntimeState::default();
    let mut new_transform = new_state
        .refresh_player(0, 0.0, 1.0 / 120.0, base, SongLuaPlayerTransform::default())
        .expect("first refresh canonicalizes attack state");
    let result = measure(SETTLED_STATE_FRAMES, || {
        if let Some(transform) = new_state.refresh_player(0, 0.0, 1.0 / 120.0, base, new_transform)
        {
            new_transform = transform;
        }
        black_box(&new_state);
        u64::from(new_transform.zoom_x.to_bits())
    });
    (result, new_state, new_transform)
}

fn settled_attack_state_benchmark() {
    let base = AttackBaseEffects::default();
    let mut samples = Vec::with_capacity(SETTLED_STATE_SAMPLES);
    for sample in 0..SETTLED_STATE_SAMPLES {
        let (old_run, new_run) = if sample % 2 == 0 {
            (
                measure_legacy_attack_state(base),
                measure_settled_attack_state(base),
            )
        } else {
            let new = measure_settled_attack_state(base);
            let old = measure_legacy_attack_state(base);
            (old, new)
        };
        assert_eq!(old_run.0.checksum, new_run.0.checksum);
        assert_eq!(old_run.1.clear_all, new_run.1.clear_all);
        assert_eq!(old_run.1.chart, new_run.1.chart);
        assert_eq!(old_run.1.visual, new_run.1.visual);
        assert_eq!(old_run.1.current_appearance, new_run.1.current_appearance);
        assert_eq!(old_run.1.appearance, new_run.1.appearance);
        assert_eq!(old_run.2, new_run.2);
        assert_zero_alloc(&old_run.0);
        assert_zero_alloc(&new_run.0);
        samples.push((old_run.0, new_run.0));
    }
    samples.sort_unstable_by(|(old_a, new_a), (old_b, new_b)| {
        paired_cycle_ratio(old_a, new_a).total_cmp(&paired_cycle_ratio(old_b, new_b))
    });
    let (old, new) = samples.remove(samples.len() / 2);

    println!(
        "\nsettled no-attack state refresh ({SETTLED_STATE_FRAMES} frames, median of {SETTLED_STATE_SAMPLES} paired samples)"
    );
    print_result("old rebuild + stores", &old);
    print_result("new unchanged check", &new);
    print_change(&old, &new);
}

fn note_hide_benchmark() {
    const HIDE_WINDOWS: usize = 1_024;
    let source = (0..HIDE_WINDOWS)
        .map(|index| SongLuaNoteHideWindowRuntime {
            column: 0,
            start_beat: index as f32 * 0.5,
            end_beat: index as f32 * 0.5 + 0.25,
        })
        .collect::<Vec<_>>();
    let indexed = SongLuaNoteHideWindows::new(source);
    let mut reference_query = 0usize;
    let reference = measure(NOTE_HIDE_QUERIES, || {
        let beat = (reference_query % (HIDE_WINDOWS * 4)) as f32 * 0.125;
        reference_query += 1;
        u64::from(song_lua_note_hidden_reference(
            black_box(&indexed),
            0,
            black_box(beat),
        ))
    });
    let mut indexed_query = 0usize;
    let optimized = measure(NOTE_HIDE_QUERIES, || {
        let beat = (indexed_query % (HIDE_WINDOWS * 4)) as f32 * 0.125;
        indexed_query += 1;
        u64::from(song_lua_note_hidden(
            black_box(&indexed),
            0,
            black_box(beat),
        ))
    });
    assert_eq!(reference.checksum, optimized.checksum);
    assert_zero_alloc(&reference);
    assert_zero_alloc(&optimized);

    let old_storage = HIDE_WINDOWS * std::mem::size_of::<SongLuaNoteHideWindowRuntime>();
    println!("\nSong-Lua note-hide lookup ({HIDE_WINDOWS} windows, {NOTE_HIDE_QUERIES} queries)");
    print_result("old lane scan", &reference);
    print_result("new prefix index", &optimized);
    print_change(&reference, &optimized);
    println!(
        "  retained index storage: old={old_storage} B / 1 allocation, new={} B / 2 allocations",
        indexed.storage_bytes(),
    );
}

#[inline(always)]
fn legacy_row_entry_index(row_map: &[u32], row_index: usize) -> Option<usize> {
    let index = *row_map.get(row_index)?;
    (index != u32::MAX).then_some(index as usize)
}

fn measure_dense_row_lookup(row_map: &[u32], row_count: usize) -> BenchResult {
    let mut query = 0usize;
    measure(ROW_LOOKUPS, || {
        query = (query + 17) & (row_count - 1);
        legacy_row_entry_index(black_box(row_map), black_box(query * ROW_STRIDE))
            .unwrap_or_default() as u64
    })
}

fn measure_note_row_lookup(note_row_entries: &[u32], row_count: usize) -> BenchResult {
    let mut query = 0usize;
    measure(ROW_LOOKUPS, || {
        query = (query + 17) & (row_count - 1);
        row_entry_index_for_note(black_box(note_row_entries), black_box(query * 2))
            .unwrap_or_default() as u64
    })
}

fn row_lookup_case(label: &str, row_count: usize) {
    assert!(row_count.is_power_of_two());
    let note_count = row_count * 2;
    let mut dense_row_maps: [Vec<u32>; MAX_PLAYERS] =
        std::array::from_fn(|_| vec![u32::MAX; (row_count - 1) * ROW_STRIDE + 1]);
    let note_row_entries = (0..note_count)
        .map(|note| (note / 2) as u32)
        .collect::<Vec<_>>();
    for row in 0..row_count {
        dense_row_maps[0][row * ROW_STRIDE] = row as u32;
    }

    let mut samples = Vec::with_capacity(ROW_LOOKUP_SAMPLES);
    for sample in 0..ROW_LOOKUP_SAMPLES {
        let (old, new) = if sample % 2 == 0 {
            (
                measure_dense_row_lookup(&dense_row_maps[0], row_count),
                measure_note_row_lookup(&note_row_entries, row_count),
            )
        } else {
            let new = measure_note_row_lookup(&note_row_entries, row_count);
            let old = measure_dense_row_lookup(&dense_row_maps[0], row_count);
            (old, new)
        };
        assert_eq!(old.checksum, new.checksum);
        assert_zero_alloc(&old);
        assert_zero_alloc(&new);
        samples.push((old, new));
    }
    samples.sort_unstable_by(|(old_a, new_a), (old_b, new_b)| {
        paired_cycle_ratio(old_a, new_a).total_cmp(&paired_cycle_ratio(old_b, new_b))
    });
    let (old, new) = samples.remove(samples.len() / 2);

    let old_bytes = dense_row_maps
        .iter()
        .map(|map| map.capacity() * std::mem::size_of::<u32>())
        .sum::<usize>()
        + note_row_entries.capacity() * std::mem::size_of::<u32>();
    let new_bytes = note_row_entries.capacity() * std::mem::size_of::<u32>();
    println!(
        "\nnote-to-row lookup ({label}: {row_count} populated rows, {note_count} notes, median of {ROW_LOOKUP_SAMPLES} paired samples)"
    );
    print_result("old dense row map", &old);
    print_result("new note-aligned map", &new);
    print_change(&old, &new);
    println!(
        "  retained lookup storage: old={old_bytes} B / 3 allocations, new={new_bytes} B / 1 allocation ({:.2}% bytes)",
        percent_change(old_bytes as f64, new_bytes as f64),
    );
}

fn row_lookup_benchmark() {
    row_lookup_case("representative", ROW_TYPICAL_COUNT);
    row_lookup_case("large", ROW_LARGE_COUNT);
}

const BENCH_CUE_COLUMNS: [ColumnCueColumn; 4] = [
    ColumnCueColumn {
        column: 0,
        is_mine: false,
    },
    ColumnCueColumn {
        column: 3,
        is_mine: true,
    },
    ColumnCueColumn {
        column: 7,
        is_mine: false,
    },
    ColumnCueColumn {
        column: 9,
        is_mine: true,
    },
];

#[inline(always)]
fn cue_checksum(columns: impl IntoIterator<Item = ColumnCueColumn>) -> u64 {
    columns.into_iter().fold(0u64, |sum, column| {
        sum.wrapping_add(column.column as u64 + u64::from(column.is_mine) * 31)
    })
}

fn cue_columns_benchmark() {
    let old_build = measure(CUE_BUILD_ITERATIONS, || {
        let columns = Vec::from(BENCH_CUE_COLUMNS);
        cue_checksum(columns)
    });
    let new_build = measure(CUE_BUILD_ITERATIONS, || {
        let columns = ColumnCueColumns::from(BENCH_CUE_COLUMNS);
        cue_checksum(&columns)
    });
    assert_eq!(old_build.checksum, new_build.checksum);
    assert_zero_alloc(&new_build);

    let old_columns = (0..CUE_COUNT)
        .map(|_| Vec::from(BENCH_CUE_COLUMNS))
        .collect::<Vec<_>>();
    let new_columns = vec![ColumnCueColumns::from(BENCH_CUE_COLUMNS); CUE_COUNT];
    let mut old_query = 0usize;
    let old_read = measure(CUE_LOOKUPS, || {
        old_query = (old_query + 17) & (CUE_COUNT - 1);
        cue_checksum(old_columns[old_query].iter().copied())
    });
    let mut new_query = 0usize;
    let new_read = measure(CUE_LOOKUPS, || {
        new_query = (new_query + 17) & (CUE_COUNT - 1);
        cue_checksum(&new_columns[new_query])
    });
    assert_eq!(old_read.checksum, new_read.checksum);
    assert_zero_alloc(&old_read);
    assert_zero_alloc(&new_read);

    let old_bytes = old_columns.capacity() * std::mem::size_of::<Vec<ColumnCueColumn>>()
        + old_columns
            .iter()
            .map(|columns| columns.capacity() * std::mem::size_of::<ColumnCueColumn>())
            .sum::<usize>();
    let new_bytes = new_columns.capacity() * std::mem::size_of::<ColumnCueColumns>();
    println!("\ncolumn-cue construction ({CUE_BUILD_ITERATIONS} four-lane cues)");
    print_result("old Vec columns", &old_build);
    print_result("new lane masks", &new_build);
    print_change(&old_build, &new_build);
    println!("\ncolumn-cue traversal ({CUE_COUNT} retained cues, {CUE_LOOKUPS} queries)");
    print_result("old Vec columns", &old_read);
    print_result("new lane masks", &new_read);
    print_change(&old_read, &new_read);
    println!(
        "  retained column storage: old={old_bytes} B / {} allocations, new={new_bytes} B / 1 allocation ({:.2}% bytes)",
        CUE_COUNT + 1,
        percent_change(old_bytes as f64, new_bytes as f64),
    );
}

fn benchmark_cues() -> Vec<ColumnCue> {
    (0..CUE_COUNT)
        .map(|index| ColumnCue {
            start_time: index as f32 * 0.25,
            duration: 0.5,
            columns: ColumnCueColumns::from(BENCH_CUE_COLUMNS),
        })
        .collect()
}

fn cue_state(cues: Vec<ColumnCue>) -> GameplayCueRuntimeState {
    let mut crossover_cues = std::array::from_fn(|_| Vec::new());
    crossover_cues[0] = cues;
    GameplayCueRuntimeState::new(
        std::array::from_fn(|_| Vec::new()),
        std::array::from_fn(|_| Vec::new()),
        crossover_cues,
    )
}

struct LegacyCueAnchorState {
    cues: [Vec<ColumnCue>; MAX_PLAYERS],
    entries: [Vec<Option<f32>>; MAX_PLAYERS],
    cursors: [usize; MAX_PLAYERS],
}

impl LegacyCueAnchorState {
    fn new(cues: Vec<ColumnCue>) -> Self {
        let mut player_cues = std::array::from_fn(|_| Vec::new());
        player_cues[0] = cues;
        Self {
            cues: player_cues,
            entries: std::array::from_fn(|_| Vec::new()),
            cursors: [0; MAX_PLAYERS],
        }
    }

    fn prewarmed(cues: Vec<ColumnCue>) -> Self {
        let mut state = Self::new(cues);
        state.entries[0] = vec![None; state.cues[0].len()];
        state
    }

    fn update(&mut self, player: usize, current_time: f32) {
        let Some(cues) = self.cues.get(player) else {
            return;
        };
        let Some(entries) = self.entries.get_mut(player) else {
            return;
        };
        if entries.len() != cues.len() {
            entries.clear();
            entries.resize(cues.len(), None);
            self.cursors[player] = 0;
        }
        let cursor = self.cursors[player];
        let target = column_cue_cursor_from_hint(cues, current_time, cursor);
        if target > cursor {
            for index in cursor..target {
                let start = cues[index].start_time;
                entries[index] = Some(if current_time - start < CROSSOVER_CUE_SEEK_GUARD_SECONDS {
                    start
                } else {
                    current_time
                });
            }
        } else if target < cursor {
            entries[target..cursor].fill(None);
        }
        self.cursors[player] = target;
    }

    fn cursor(&self, player: usize) -> usize {
        self.cursors.get(player).copied().unwrap_or_default()
    }

    fn entry_time(&self, player: usize, index: usize) -> Option<f32> {
        self.entries
            .get(player)
            .and_then(|entries| entries.get(index).copied())
            .flatten()
    }
}

fn allocation_delta(operation: impl FnOnce()) -> AllocSnapshot {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    operation();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    ALLOC.snapshot().delta(before)
}

fn cue_anchor_checksum(cursor: usize, anchor: Option<f32>) -> u64 {
    cursor as u64 ^ u64::from(anchor.unwrap_or(f32::NAN).to_bits())
}

fn measure_legacy_anchors(
    cues: &[ColumnCue],
    cycle_seconds: f32,
) -> (BenchResult, LegacyCueAnchorState) {
    let mut state = LegacyCueAnchorState::prewarmed(cues.to_vec());
    let mut frame = 0usize;
    let result = measure(CUE_ANCHOR_FRAMES, || {
        let now = (frame as f32 * 0.02) % cycle_seconds;
        frame += 1;
        state.update(0, now);
        let cursor = state.cursor(0);
        cue_anchor_checksum(
            cursor,
            cursor
                .checked_sub(1)
                .and_then(|index| state.entry_time(0, index)),
        )
    });
    (result, state)
}

fn measure_compact_anchors(
    cues: &[ColumnCue],
    cycle_seconds: f32,
) -> (BenchResult, GameplayCueRuntimeState) {
    let mut state = cue_state(cues.to_vec());
    let mut frame = 0usize;
    let result = measure(CUE_ANCHOR_FRAMES, || {
        let now = (frame as f32 * 0.02) % cycle_seconds;
        frame += 1;
        state.update_crossover_cue_anchors(0, now);
        let cursor = state.crossover_cue_cursor(0);
        cue_anchor_checksum(
            cursor,
            cursor
                .checked_sub(1)
                .and_then(|index| state.crossover_cue_entry_time(0, index)),
        )
    });
    (result, state)
}

fn assert_anchor_parity(
    cues: &[ColumnCue],
    old: &LegacyCueAnchorState,
    new: &GameplayCueRuntimeState,
) {
    assert_eq!(old.cursor(0), new.crossover_cue_cursor(0));
    for index in 0..cues.len() {
        assert_eq!(
            old.entry_time(0, index),
            new.crossover_cue_entry_time(0, index),
            "anchor mismatch at cue {index}",
        );
    }
}

fn paired_cycle_ratio(old: &BenchResult, new: &BenchResult) -> f64 {
    match (old.cycles_per_item, new.cycles_per_item) {
        (Some(old), Some(new)) => new / old,
        _ => new.ns_per_item / old.ns_per_item,
    }
}

fn cue_anchor_benchmark() {
    let cues = benchmark_cues();
    let mut old_first_state = LegacyCueAnchorState::new(cues.clone());
    let old_first = allocation_delta(|| old_first_state.update(0, 0.0));
    let mut new_first_state = cue_state(cues.clone());
    let new_first = allocation_delta(|| new_first_state.update_crossover_cue_anchors(0, 0.0));
    assert_eq!(old_first.allocs, 1);
    assert_eq!(new_first.allocs, 0);
    assert_eq!(new_first.reallocs, 0);
    assert_eq!(new_first.bytes, 0);

    let cycle_seconds = CUE_COUNT as f32 * 0.25 + 1.0;
    let mut samples = Vec::with_capacity(CUE_ANCHOR_SAMPLES);
    for sample in 0..CUE_ANCHOR_SAMPLES {
        let (old_run, new_run) = if sample % 2 == 0 {
            (
                measure_legacy_anchors(&cues, cycle_seconds),
                measure_compact_anchors(&cues, cycle_seconds),
            )
        } else {
            let new = measure_compact_anchors(&cues, cycle_seconds);
            let old = measure_legacy_anchors(&cues, cycle_seconds);
            (old, new)
        };
        assert_eq!(old_run.0.checksum, new_run.0.checksum);
        assert_anchor_parity(&cues, &old_run.1, &new_run.1);
        assert_zero_alloc(&old_run.0);
        assert_zero_alloc(&new_run.0);
        samples.push((old_run.0, new_run.0));
    }
    samples.sort_unstable_by(|(old_a, new_a), (old_b, new_b)| {
        paired_cycle_ratio(old_a, new_a).total_cmp(&paired_cycle_ratio(old_b, new_b))
    });
    let (old, new) = samples.remove(samples.len() / 2);

    let old_bytes = cues.len() * std::mem::size_of::<Option<f32>>();
    let new_bytes = cues.len() * std::mem::size_of::<f32>();
    println!(
        "\ncrossover cue anchors ({CUE_COUNT} cues, {CUE_ANCHOR_FRAMES} frames, median of {CUE_ANCHOR_SAMPLES} paired samples)"
    );
    print_result("old Option Vec", &old);
    print_result("new boxed f32", &new);
    print_change(&old, &new);
    println!(
        "  first gameplay update: old={} alloc / {} B, new={} alloc / {} B",
        old_first.allocs, old_first.bytes, new_first.allocs, new_first.bytes,
    );
    println!(
        "  retained anchor storage: old={old_bytes} B, new={new_bytes} B ({:.2}% bytes)",
        percent_change(old_bytes as f64, new_bytes as f64),
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
    idle_attack_benchmark();
    settled_attack_state_benchmark();
    note_hide_benchmark();
    row_lookup_benchmark();
    cue_columns_benchmark();
    cue_anchor_benchmark();
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
