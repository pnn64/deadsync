use deadlib_present::actors::Actor;
use deadsync_theme_simply_love::screens::gameplay::{
    BENCH_NOTEFIELD_ACTOR_SCRATCH_CAPACITY, BENCH_NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY,
    benchmark_append_direct_identity_player_actors, benchmark_append_player_actors,
    benchmark_present_identity_notefield, benchmark_present_identity_notefield_legacy,
};
use glam::{Mat4, Vec3};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const FIELD_ACTORS: usize = 224;
const HUD_ACTORS: usize = 32;
const WARMUP_FRAMES: usize = 2_000;
const MEASURE_FRAMES: usize = 50_000;
const PEAK_FIELD_ACTORS: usize = BENCH_NOTEFIELD_ACTOR_SCRATCH_CAPACITY;
const PEAK_HUD_ACTORS: usize = BENCH_NOTEFIELD_HUD_ACTOR_SCRATCH_CAPACITY;
const PEAK_PLAYER_ACTORS: usize = PEAK_FIELD_ACTORS + PEAK_HUD_ACTORS;
const WARMUP_PEAK_FRAMES: usize = 512;
const MEASURE_PEAK_FRAMES: usize = 10_000;

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

// SAFETY: calls delegate unchanged to `System`; atomics only observe
// successful allocation activity.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
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

fn fill(actors: &mut Vec<Actor>, count: usize, base: f32) {
    actors.clear();
    actors.extend((0..count).map(|index| Actor::CameraPush {
        view_proj: Mat4::from_translation(Vec3::new(base + index as f32, 0.0, 0.0)),
    }));
}

fn fill_incrementally(actors: &mut Vec<Actor>, count: usize, base: f32) {
    actors.clear();
    for index in 0..count {
        actors.push(Actor::CameraPush {
            view_proj: Mat4::from_translation(Vec3::new(base + index as f32, 0.0, 0.0)),
        });
    }
}

fn frame(
    field: &mut Vec<Actor>,
    hud: &mut Vec<Actor>,
    out: &mut Vec<Actor>,
    present: fn(&mut Vec<Actor>, &mut Vec<Actor>, &mut Vec<Actor>),
) -> f32 {
    fill(field, FIELD_ACTORS, 0.0);
    fill(hud, HUD_ACTORS, 10_000.0);
    present(field, hud, out);
    let first = match out.first() {
        Some(Actor::CameraPush { view_proj }) => view_proj.w_axis.x,
        _ => -1.0,
    };
    let last = match out.last() {
        Some(Actor::CameraPush { view_proj }) => view_proj.w_axis.x,
        _ => -1.0,
    };
    first + last + out.len() as f32
}

fn assembled_frame(
    field: &mut Vec<Actor>,
    hud: &mut Vec<Actor>,
    player: &mut Vec<Actor>,
    out: &mut Vec<Actor>,
    assemble: fn(&mut Vec<Actor>, &mut Vec<Actor>, &mut Vec<Actor>, &mut Vec<Actor>),
) -> f32 {
    fill(field, FIELD_ACTORS, 0.0);
    fill(hud, HUD_ACTORS, 10_000.0);
    out.clear();
    assemble(out, field, hud, player);
    let first = match out.first() {
        Some(Actor::CameraPush { view_proj }) => view_proj.w_axis.x,
        _ => -1.0,
    };
    let last = match out.last() {
        Some(Actor::CameraPush { view_proj }) => view_proj.w_axis.x,
        _ => -1.0,
    };
    first + last + out.len() as f32
}

fn assemble_buffered(
    out: &mut Vec<Actor>,
    field: &mut Vec<Actor>,
    hud: &mut Vec<Actor>,
    player: &mut Vec<Actor>,
) {
    benchmark_present_identity_notefield(field, hud, player);
    benchmark_append_player_actors(out, player);
}

fn assemble_direct(
    out: &mut Vec<Actor>,
    field: &mut Vec<Actor>,
    hud: &mut Vec<Actor>,
    player: &mut Vec<Actor>,
) {
    benchmark_append_direct_identity_player_actors(out, field, hud, player);
}

struct BenchResult {
    elapsed: std::time::Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    checksum: f32,
}

fn measure(present: fn(&mut Vec<Actor>, &mut Vec<Actor>, &mut Vec<Actor>)) -> BenchResult {
    let mut field = Vec::with_capacity(FIELD_ACTORS);
    let mut hud = Vec::with_capacity(HUD_ACTORS);
    let mut out = Vec::with_capacity(FIELD_ACTORS + HUD_ACTORS);
    for _ in 0..WARMUP_FRAMES {
        black_box(frame(&mut field, &mut hud, &mut out, present));
    }

    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0.0f32;
    for _ in 0..MEASURE_FRAMES {
        checksum += black_box(frame(&mut field, &mut hud, &mut out, present));
    }
    let elapsed = started.elapsed();
    let cycles = read_cycles().saturating_sub(before_cycles);
    let allocated = ALLOC.snapshot().delta(before);
    assert!(field.is_empty());
    assert!(hud.is_empty());
    assert_eq!(out.len(), FIELD_ACTORS + HUD_ACTORS);

    BenchResult {
        elapsed,
        cycles,
        allocated,
        checksum,
    }
}

fn measure_assembled(
    assemble: fn(&mut Vec<Actor>, &mut Vec<Actor>, &mut Vec<Actor>, &mut Vec<Actor>),
) -> BenchResult {
    let mut field = Vec::with_capacity(FIELD_ACTORS);
    let mut hud = Vec::with_capacity(HUD_ACTORS);
    let mut player = Vec::with_capacity(FIELD_ACTORS + HUD_ACTORS);
    let mut out = Vec::with_capacity(FIELD_ACTORS + HUD_ACTORS);
    for _ in 0..WARMUP_FRAMES {
        black_box(assembled_frame(
            &mut field,
            &mut hud,
            &mut player,
            &mut out,
            assemble,
        ));
    }

    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0.0f32;
    for _ in 0..MEASURE_FRAMES {
        checksum += black_box(assembled_frame(
            &mut field,
            &mut hud,
            &mut player,
            &mut out,
            assemble,
        ));
    }
    let elapsed = started.elapsed();
    let cycles = read_cycles().saturating_sub(before_cycles);
    let allocated = ALLOC.snapshot().delta(before);
    assert!(field.is_empty());
    assert!(hud.is_empty());
    assert!(player.is_empty());
    assert_eq!(out.len(), FIELD_ACTORS + HUD_ACTORS);

    BenchResult {
        elapsed,
        cycles,
        allocated,
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "{label:<17} {:>9.2} ns/frame  {:>8.0} cycles/frame  \
         {:>5.2} allocs/frame  {:>7.1} bytes/frame  {:>5.2} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000_000.0 / frames,
        result.cycles as f64 / frames,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames,
        result.allocated.reallocs as f64 / frames,
    );
}

fn peak_scratch_frame(presized: bool) -> f32 {
    let mut checksum = 0.0;
    for player in 0..2 {
        let mut field = if presized {
            Vec::with_capacity(PEAK_FIELD_ACTORS)
        } else {
            Vec::new()
        };
        let mut hud = if presized {
            Vec::with_capacity(PEAK_HUD_ACTORS)
        } else {
            Vec::new()
        };
        let mut assembled = if presized {
            Vec::with_capacity(PEAK_PLAYER_ACTORS)
        } else {
            Vec::new()
        };
        fill_incrementally(&mut field, PEAK_FIELD_ACTORS, player as f32 * 20_000.0);
        fill_incrementally(
            &mut hud,
            PEAK_HUD_ACTORS,
            player as f32 * 20_000.0 + 10_000.0,
        );
        benchmark_present_identity_notefield(&mut field, &mut hud, &mut assembled);
        let first = match assembled.first() {
            Some(Actor::CameraPush { view_proj }) => view_proj.w_axis.x,
            _ => -1.0,
        };
        let last = match assembled.last() {
            Some(Actor::CameraPush { view_proj }) => view_proj.w_axis.x,
            _ => -1.0,
        };
        checksum += first + last + assembled.len() as f32;
    }
    checksum
}

fn measure_peak_scratch(presized: bool) -> BenchResult {
    for _ in 0..WARMUP_PEAK_FRAMES {
        black_box(peak_scratch_frame(presized));
    }
    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0.0;
    for _ in 0..MEASURE_PEAK_FRAMES {
        checksum += black_box(peak_scratch_frame(presized));
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_peak_result(label: &str, result: &BenchResult) {
    let frames = MEASURE_PEAK_FRAMES as f64;
    println!(
        "{label:<17} {:>9.2} us/frame  {:>8.0} cycles/frame  \
         {:>5.2} allocs/frame  {:>8.1} KiB/frame  {:>5.2} reallocs/frame",
        result.elapsed.as_secs_f64() * 1_000_000.0 / frames,
        result.cycles as f64 / frames,
        result.allocated.allocs as f64 / frames,
        result.allocated.bytes as f64 / frames / 1024.0,
        result.allocated.reallocs as f64 / frames,
    );
}

fn main() {
    let legacy = measure(benchmark_present_identity_notefield_legacy);
    let direct = measure(benchmark_present_identity_notefield);
    let buffered = measure_assembled(assemble_buffered);
    let assembled = measure_assembled(assemble_direct);
    assert_eq!(legacy.checksum, direct.checksum);
    assert_eq!(direct.checksum, buffered.checksum);
    assert_eq!(buffered.checksum, assembled.checksum);
    black_box((legacy.checksum, direct.checksum, assembled.checksum));

    println!("identity notefield presentation benchmark");
    print_result("legacy per-actor", &legacy);
    print_result("direct append", &direct);
    print_result("buffered final", &buffered);
    print_result("direct final", &assembled);

    let growing_scratch = measure_peak_scratch(false);
    let presized_scratch = measure_peak_scratch(true);
    assert_eq!(growing_scratch.checksum, presized_scratch.checksum);
    black_box((growing_scratch.checksum, presized_scratch.checksum));
    println!(
        "\ngameplay actor scratch density-spike benchmark \
         (2 players, {PEAK_FIELD_ACTORS} field + {PEAK_HUD_ACTORS} HUD actors)"
    );
    print_peak_result("zero capacity", &growing_scratch);
    print_peak_result("presized", &presized_scratch);
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
