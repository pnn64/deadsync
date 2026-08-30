use deadlib_present::actors::Actor;
use deadsync_online::lobbies::{JoinedLobby, LobbyPlayer};
use deadsync_profile::PlayerSide;
use deadsync_score::Grade;
use deadsync_theme_simply_love::screens::components::evaluation::{
    eval_grades::{self, EvalGradeParams},
    pane_modifiers::{benchmark_build_modifiers_pane, benchmark_push_modifiers_pane},
    pane_percentage::PercentagePaneAppendBenchmark,
    pane_timing::TimingPaneAppendBenchmark,
    pane_timing_arrows::TimingArrowsPaneAppendBenchmark,
};
use deadsync_theme_simply_love::screens::components::shared::lobby_hud::{
    CachedRenderParams, LobbyHudCache, RenderParams, build_panel, push_cached_panel,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const GRADE_FRAMES: usize = 200_000;
const MODIFIER_FRAMES: usize = 500_000;
const LOBBY_FRAMES: usize = 30_000;
const PANE_FRAMES: usize = 50_000;
const SAMPLE_OPS: usize = 500;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    churn_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            churn_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            churn_bytes: self.churn_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocation calls delegate unchanged to `System`; relaxed counters
// only observe successful calls while the single benchmark thread measures.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied a valid allocation layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.churn_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.churn_bytes
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
            self.churn_bytes
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
    churn_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            churn_bytes: self.churn_bytes - before.churn_bytes,
        }
    }
}

struct BenchResult {
    ns_per_frame: f64,
    worst_sample_ns: f64,
    cycles_per_frame: Option<f64>,
    frames_per_second: f64,
    allocations: AllocSnapshot,
    checksum: u64,
}

fn measure(frames: usize, mut frame: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..frames.min(2_000) {
        black_box(frame());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0_u64;
    let mut worst_sample_ns = 0.0_f64;
    for _ in 0..frames / SAMPLE_OPS {
        let sample_started = Instant::now();
        for _ in 0..SAMPLE_OPS {
            checksum = checksum.wrapping_add(black_box(frame()));
        }
        worst_sample_ns = worst_sample_ns
            .max(sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / SAMPLE_OPS as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0_u64;
    for _ in 0..frames {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(frame()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    black_box(allocation_checksum);

    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_frame: seconds * 1_000_000_000.0 / frames as f64,
        worst_sample_ns,
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / frames as f64),
        frames_per_second: frames as f64 / seconds,
        allocations: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn report_pair(
    title: &str,
    frames: usize,
    old: &BenchResult,
    new: &BenchResult,
    expect_zero_churn: bool,
) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    if expect_zero_churn {
        assert_eq!(new.allocations.allocs, 0, "{title} still allocates");
        assert_eq!(new.allocations.reallocs, 0, "{title} still reallocates");
        assert_eq!(new.allocations.frees, 0, "{title} still frees");
        assert_eq!(new.allocations.churn_bytes, 0, "{title} still churns bytes");
    } else {
        assert!(
            new.allocations.allocs < old.allocations.allocs,
            "{title} did not reduce allocations"
        );
        assert!(
            new.allocations.churn_bytes < old.allocations.churn_bytes,
            "{title} did not reduce byte churn"
        );
    }
    assert!(
        new.ns_per_frame < old.ns_per_frame,
        "{title} did not improve measured latency"
    );

    println!("\n{title} ({frames} frames)");
    print_result("old", frames, old);
    print_result("new", frames, new);
    println!(
        "  change: {:+.2}% latency  {:+.2}% cycles  {:+.2}% throughput  {:+.2}% churn",
        percent_change(old.ns_per_frame, new.ns_per_frame),
        percent_change(
            old.cycles_per_frame.unwrap_or(f64::NAN),
            new.cycles_per_frame.unwrap_or(f64::NAN),
        ),
        percent_change(old.frames_per_second, new.frames_per_second),
        percent_change(
            old.allocations.churn_bytes as f64,
            new.allocations.churn_bytes as f64,
        ),
    );
}

fn print_result(label: &str, frames: usize, result: &BenchResult) {
    let frames = frames as f64;
    println!(
        "  {label:<3} {:>10.2} ns/frame  {:>10.2} cycles/frame  {:>10.2} worst ns  \
         {:>8.2} Mframe/s  {:>5.2} alloc  {:>5.2} realloc  {:>5.2} free  \
         {:>9.1} churn B/frame",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        result.worst_sample_ns,
        result.frames_per_second / 1_000_000.0,
        result.allocations.allocs as f64 / frames,
        result.allocations.reallocs as f64 / frames,
        result.allocations.frees as f64 / frames,
        result.allocations.churn_bytes as f64 / frames,
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn actor_count_checksum(actors: &[Actor]) -> u64 {
    actors.len() as u64
}

fn grade_benchmark() {
    let params = EvalGradeParams {
        x: 250.0,
        y: 106.0,
        z: 101,
        zoom: 0.4,
        elapsed: 3.5,
        easter_eggs: false,
        ..EvalGradeParams::default()
    };
    let mut legacy = Vec::with_capacity(16);
    let mut direct = Vec::with_capacity(16);
    legacy.extend(eval_grades::actors(Grade::Quint, params));
    eval_grades::push_actors(&mut direct, Grade::Quint, params);
    assert_eq!(format!("{legacy:#?}"), format!("{direct:#?}"));

    let old = measure(GRADE_FRAMES, || {
        legacy.clear();
        legacy.extend(eval_grades::actors(black_box(Grade::Quint), params));
        actor_count_checksum(black_box(&legacy))
    });
    let new = measure(GRADE_FRAMES, || {
        direct.clear();
        eval_grades::push_actors(&mut direct, black_box(Grade::Quint), params);
        actor_count_checksum(black_box(&direct))
    });
    report_pair("quint grade actor append", GRADE_FRAMES, &old, &new, true);
}

fn modifiers_benchmark() {
    let text: Arc<str> = Arc::from("M700, 40% Mini, Overhead, cel");
    let mut legacy = benchmark_build_modifiers_pane(Arc::clone(&text));
    let mut direct = Vec::with_capacity(2);
    benchmark_push_modifiers_pane(&mut direct, Arc::clone(&text));
    assert_eq!(format!("{legacy:#?}"), format!("{direct:#?}"));

    let old = measure(MODIFIER_FRAMES, || {
        legacy.clear();
        legacy.extend(benchmark_build_modifiers_pane(Arc::clone(&text)));
        actor_count_checksum(black_box(&legacy))
    });
    let new = measure(MODIFIER_FRAMES, || {
        direct.clear();
        benchmark_push_modifiers_pane(&mut direct, Arc::clone(&text));
        actor_count_checksum(black_box(&direct))
    });
    report_pair(
        "modifiers bar actor append",
        MODIFIER_FRAMES,
        &old,
        &new,
        true,
    );
}

fn lobby_player(index: usize) -> LobbyPlayer {
    LobbyPlayer {
        label: format!("Remote Player {index:02}"),
        ready: index % 2 == 0,
        screen_name: if index % 3 == 0 {
            "ScreenEvaluationStage".to_string()
        } else {
            "ScreenGameplay".to_string()
        },
        judgments: None,
        score: Some(99.0 - index as f32 * 0.125),
        ex_score: Some(98.0 - index as f32 * 0.25),
    }
}

fn lobby_text(actors: &[Actor]) -> &str {
    match actors.get(1) {
        Some(Actor::Text { content, .. }) => content.as_str(),
        other => panic!("expected lobby text actor, got {other:?}"),
    }
}

fn lobby_benchmark() {
    const STATUS: &str = "Waiting for players to finish gameplay...\n\
                          Hold &START; to disconnect from the lobby.";
    let joined = JoinedLobby {
        code: "BENCHMARK".to_string(),
        players: (0..12).map(lobby_player).collect(),
        song_info: None,
    };
    let mut legacy = build_panel(RenderParams {
        screen_name: "ScreenEvaluationStage",
        joined: &joined,
        z: 121,
        show_song_info: false,
        status_text: Some(STATUS.to_string()),
        joined_sides: [true, false],
        player_side: PlayerSide::P1,
    });
    let mut cache = LobbyHudCache::default();
    let mut cached = Vec::with_capacity(2);
    let cached_params = || CachedRenderParams {
        screen_name: "ScreenEvaluationStage",
        joined: &joined,
        z: 121,
        show_song_info: false,
        status_text: Some(STATUS),
        joined_sides: [true, false],
        player_side: PlayerSide::P1,
    };
    push_cached_panel(&mut cached, &mut cache, cached_params());
    assert_eq!(legacy.len(), cached.len());
    assert_eq!(lobby_text(&legacy), lobby_text(&cached));

    let old = measure(LOBBY_FRAMES, || {
        legacy.clear();
        legacy.extend(build_panel(RenderParams {
            screen_name: "ScreenEvaluationStage",
            joined: &joined,
            z: 121,
            show_song_info: false,
            status_text: Some(STATUS.to_string()),
            joined_sides: [true, false],
            player_side: PlayerSide::P1,
        }));
        (actor_count_checksum(black_box(&legacy)) << 32) | lobby_text(&legacy).len() as u64
    });
    let new = measure(LOBBY_FRAMES, || {
        cached.clear();
        push_cached_panel(&mut cached, &mut cache, cached_params());
        (actor_count_checksum(black_box(&cached)) << 32) | lobby_text(&cached).len() as u64
    });
    assert_eq!(
        cache.stats().misses,
        1,
        "stable lobby cache unexpectedly missed"
    );
    report_pair(
        "cached evaluation lobby HUD",
        LOBBY_FRAMES,
        &old,
        &new,
        false,
    );
}

fn percentage_pane_benchmark() {
    let fixture = PercentagePaneAppendBenchmark::new();
    let mut legacy = Vec::with_capacity(1);
    let mut direct = Vec::with_capacity(1);
    let _ = fixture.legacy_frame(&mut legacy);
    let _ = fixture.direct_frame(&mut direct);
    assert_eq!(format!("{legacy:#?}"), format!("{direct:#?}"));

    let old = measure(PANE_FRAMES, || fixture.legacy_frame(&mut legacy));
    let new = measure(PANE_FRAMES, || fixture.direct_frame(&mut direct));
    report_pair(
        "percentage pane actor staging",
        PANE_FRAMES,
        &old,
        &new,
        false,
    );
}

fn timing_pane_benchmark() {
    let fixture = TimingPaneAppendBenchmark::new();
    let mut legacy = Vec::with_capacity(1);
    let mut direct = Vec::with_capacity(1);
    let _ = fixture.legacy_frame(&mut legacy);
    let _ = fixture.direct_frame(&mut direct);
    assert_eq!(format!("{legacy:#?}"), format!("{direct:#?}"));

    let old = measure(PANE_FRAMES, || fixture.legacy_frame(&mut legacy));
    let new = measure(PANE_FRAMES, || fixture.direct_frame(&mut direct));
    report_pair(
        "aggregate timing pane actor staging",
        PANE_FRAMES,
        &old,
        &new,
        false,
    );
}

fn timing_arrows_pane_benchmark() {
    let fixture = TimingArrowsPaneAppendBenchmark::new();
    let mut legacy = Vec::with_capacity(1);
    let mut direct = Vec::with_capacity(1);
    let _ = fixture.legacy_frame(&mut legacy);
    let _ = fixture.direct_frame(&mut direct);
    assert_eq!(format!("{legacy:#?}"), format!("{direct:#?}"));

    let old = measure(PANE_FRAMES, || fixture.legacy_frame(&mut legacy));
    let new = measure(PANE_FRAMES, || fixture.direct_frame(&mut direct));
    report_pair(
        "per-arrow timing pane actor staging",
        PANE_FRAMES,
        &old,
        &new,
        false,
    );
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
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
    grade_benchmark();
    modifiers_benchmark();
    lobby_benchmark();
    percentage_pane_benchmark();
    timing_pane_benchmark();
    timing_arrows_pane_benchmark();
}
