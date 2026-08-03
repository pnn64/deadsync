use deadlib_present::actors::Actor;
use deadsync_online::lobbies::{JoinedLobby, LobbyPlayer};
use deadsync_profile::PlayerSide;
use deadsync_theme_simply_love::screens::components::shared::lobby_hud::{
    CachedRenderParams, LobbyHudCache, RenderParams, build_panel, push_cached_panel,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 5_000;
const MEASURE_FRAMES: usize = 100_000;
const STATUS: &str = "Waiting for players\nHold back to leave lobby";

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
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

// SAFETY: every operation delegates to `System` with the caller's unchanged
// pointer and layout; relaxed atomics only observe successful allocation work.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied this allocation layout.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocs.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller supplied a live pointer/layout pair.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller supplied the live pointer and its current layout.
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
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: usize,
}

fn main() {
    let joined = fixture();
    let legacy = measure(|| {
        let actors = build_panel(RenderParams {
            screen_name: "ScreenGameplay",
            joined: &joined,
            z: 995,
            show_song_info: false,
            status_text: Some(STATUS.to_string()),
            joined_sides: [true, false],
            player_side: PlayerSide::P1,
        });
        actor_checksum(&actors)
    });

    let mut cache = LobbyHudCache::default();
    let mut actors = Vec::with_capacity(2);
    let cached = measure(|| {
        actors.clear();
        push_cached_panel(
            &mut actors,
            &mut cache,
            CachedRenderParams {
                screen_name: "ScreenGameplay",
                joined: &joined,
                z: 995,
                show_song_info: false,
                status_text: Some(STATUS),
                joined_sides: [true, false],
                player_side: PlayerSide::P1,
            },
        );
        actor_checksum(&actors)
    });
    assert_eq!(legacy.checksum, cached.checksum);
    black_box((legacy.checksum, cached.checksum));

    println!("gameplay lobby HUD benchmark (8 players, stable state)");
    print_result("legacy rebuild", &legacy);
    print_result("cached panel", &cached);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocation reduction {:.1}%",
        legacy.elapsed.as_secs_f64() / cached.elapsed.as_secs_f64(),
        100.0 * (1.0 - cached.cycles as f64 / legacy.cycles as f64),
        100.0 * (1.0 - cached.alloc.allocs as f64 / legacy.alloc.allocs as f64),
    );
    println!(
        "  cache hits={} misses={}",
        cache.stats().hits,
        cache.stats().misses,
    );
}

fn fixture() -> JoinedLobby {
    let players = (0..8)
        .map(|index| LobbyPlayer {
            label: format!("Player {}", index + 1),
            ready: index != 7,
            screen_name: "ScreenGameplay".to_string(),
            judgments: None,
            score: Some(99.0 - index as f32 * 0.37),
            ex_score: Some(98.0 - index as f32 * 0.41),
        })
        .collect();
    JoinedLobby {
        code: "ABCD".to_string(),
        players,
        song_info: None,
    }
}

fn actor_checksum(actors: &[Actor]) -> usize {
    actors.iter().fold(actors.len(), |checksum, actor| {
        let value = match actor {
            Actor::Text { content, z, .. } => content
                .as_str()
                .bytes()
                .fold(*z as usize, |sum, byte| sum.rotate_left(5) ^ byte as usize),
            Actor::Sprite { z, .. } => *z as usize,
            _ => 0,
        };
        checksum.rotate_left(7) ^ value
    })
}

fn measure(mut frame: impl FnMut() -> usize) -> BenchResult {
    for _ in 0..WARMUP_FRAMES {
        black_box(frame());
    }
    let before_alloc = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..MEASURE_FRAMES {
        checksum = checksum.wrapping_add(black_box(frame()));
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        alloc: ALLOC.snapshot().delta(before_alloc),
        checksum,
    }
}

fn print_result(name: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    println!(
        "  {name:<16} {:>10.1} ns/frame {:>10.0} cycles/frame {:>10.0} frames/s",
        result.elapsed.as_secs_f64() * 1.0e9 / frames,
        result.cycles as f64 / frames,
        frames / result.elapsed.as_secs_f64(),
    );
    println!(
        "  {:<16} allocs={} reallocs={} frees={} bytes={}",
        "memory",
        result.alloc.allocs,
        result.alloc.reallocs,
        result.alloc.deallocs,
        result.alloc.bytes,
    );
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
