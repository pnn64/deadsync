use deadsync_profile::{
    ActiveProfile, GameplayHudSnapshot, PlayStyle, PlayerSide, runtime_gameplay_hud_snapshot,
    runtime_set_active_profiles, runtime_set_session_joined, runtime_set_session_play_style,
    runtime_set_session_player_side, runtime_update_profile_for_side,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 2_000;
const MEASURE_FRAMES: usize = 102_400;
const SAMPLE_FRAMES: usize = 256;

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

// SAFETY: every operation delegates to `System` with the caller's original
// pointer and layout; the atomics only observe successful operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the allocation's original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
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

struct Result {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    samples: Vec<u64>,
    checksum: u64,
}

fn main() {
    setup_runtime();
    let cached = runtime_gameplay_hud_snapshot();
    let old = run(|| {
        let snapshot = runtime_gameplay_hud_snapshot();
        snapshot_checksum(black_box(&snapshot))
    });
    let new = run(|| snapshot_checksum(black_box(&cached)));
    assert_eq!(old.checksum, new.checksum, "old/new output mismatch");

    println!("gameplay HUD identity microbenchmark ({MEASURE_FRAMES} frames)");
    print_result("old: lock + clone", &old);
    print_result("new: song snapshot", &new);
    println!(
        "speedup {:.2}x | cycles reduction {:.1}% | allocation reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        100.0 * (1.0 - new.cycles as f64 / old.cycles as f64),
        100.0 * (1.0 - new.alloc.allocs as f64 / old.alloc.allocs as f64),
    );
}

fn setup_runtime() {
    runtime_set_session_play_style(PlayStyle::Versus);
    runtime_set_session_player_side(PlayerSide::P1);
    runtime_set_session_joined(true, true);
    runtime_set_active_profiles([
        ActiveProfile::Local {
            id: "6ea7f26a-4a25-41d8-9fbf-e72cf30cbc1d".to_owned(),
        },
        ActiveProfile::Local {
            id: "52a00e20-a1bc-47c7-828e-67399c81abec".to_owned(),
        },
    ]);
    for (side, name, avatar) in [
        (PlayerSide::P1, "ALICE", "profile-avatar-alice"),
        (PlayerSide::P2, "BOB", "profile-avatar-bob"),
    ] {
        runtime_update_profile_for_side(side, |profile| {
            profile.display_name = name.to_owned();
            profile.avatar_texture_key = Some(avatar.to_owned());
            true
        });
    }
}

fn run(mut frame: impl FnMut() -> u64) -> Result {
    for _ in 0..WARMUP_FRAMES {
        black_box(frame());
    }
    let mut samples = Vec::with_capacity(MEASURE_FRAMES / SAMPLE_FRAMES);
    let before_alloc = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for sample in 0..MEASURE_FRAMES / SAMPLE_FRAMES {
        let sample_started = Instant::now();
        for frame_index in 0..SAMPLE_FRAMES {
            checksum = checksum.rotate_left(7)
                ^ black_box(frame())
                ^ (sample * SAMPLE_FRAMES + frame_index) as u64;
        }
        samples.push(sample_started.elapsed().as_nanos() as u64 / SAMPLE_FRAMES as u64);
    }
    Result {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        alloc: ALLOC.snapshot().delta(before_alloc),
        samples,
        checksum,
    }
}

fn snapshot_checksum(snapshot: &GameplayHudSnapshot) -> u64 {
    let players = [&snapshot.p1, &snapshot.p2];
    players.into_iter().fold(
        snapshot.play_style as u64 ^ ((snapshot.player_side as u64) << 8),
        |sum, player| {
            sum.rotate_left(9)
                ^ player.display_name.len() as u64
                ^ ((player.avatar_texture_key.as_deref().map_or(0, str::len) as u64) << 16)
                ^ ((player.joined as u64) << 32)
                ^ ((player.guest as u64) << 33)
                ^ ((player.hide_username as u64) << 34)
        },
    )
}

fn print_result(name: &str, result: &Result) {
    let frames = MEASURE_FRAMES as f64;
    let mut samples = result.samples.clone();
    samples.sort_unstable();
    println!(
        "{name:<22} {:>9.1} ns/frame {:>9.0} cycles/frame {:>12.0} frames/s",
        result.elapsed.as_secs_f64() * 1.0e9 / frames,
        result.cycles as f64 / frames,
        frames / result.elapsed.as_secs_f64(),
    );
    println!(
        "{:<22} p50 {:>5} ns p95 {:>5} ns p99 {:>5} ns worst {:>5} ns",
        "sampled frame cost",
        percentile(&samples, 50),
        percentile(&samples, 95),
        percentile(&samples, 99),
        samples.last().copied().unwrap_or_default(),
    );
    println!(
        "{:<22} allocs={} reallocs={} bytes={}",
        "memory", result.alloc.allocs, result.alloc.reallocs, result.alloc.bytes,
    );
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    let index = samples.len().saturating_mul(percentile).saturating_sub(1) / 100;
    samples.get(index).copied().unwrap_or_default()
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads do not access memory; they only
    // serialize this thread's measured interval.
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
