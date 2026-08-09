use deadsync_profile::{
    ActiveProfile, GameplayHudSnapshot, PlayStyle, PlayerSide, ScoreboxProfileView,
    player_side_index, runtime_evaluation_profile_view, runtime_footer_fields_for_side,
    runtime_gameplay_hud_snapshot, runtime_profile_for_side, runtime_scorebox_view,
    runtime_session_side_guest, runtime_session_snapshot, runtime_set_active_profiles,
    runtime_set_session_joined, runtime_set_session_play_style, runtime_set_session_player_side,
    runtime_update_profile_for_side,
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

    let old = run(legacy_evaluation_profile_frame);
    let new = run(shared_evaluation_profile_frame);
    assert_eq!(old.checksum, new.checksum, "old/new output mismatch");

    println!();
    println!("evaluation profile frame microbenchmark ({MEASURE_FRAMES} frames)");
    print_result("old: repeated reads", &old);
    print_result("new: shared snapshot", &new);
    println!(
        "speedup {:.2}x | cycles reduction {:.1}% | allocation reduction {:.1}% | byte reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        100.0 * (1.0 - new.cycles as f64 / old.cycles as f64),
        100.0 * (1.0 - new.alloc.allocs as f64 / old.alloc.allocs as f64),
        100.0 * (1.0 - new.alloc.bytes as f64 / old.alloc.bytes as f64),
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
            profile.player_initials = name[..3].to_owned();
            profile.groovestats_api_key = format!("gs-key-{name}");
            profile.groovestats_username = format!("gs-{name}");
            profile.arrowcloud_api_key = format!("ac-key-{name}");
            profile
                .favorites
                .extend((0..24).map(|index| format!("{name}-chart-{index:02}")));
            profile
                .known_pack_names
                .extend((0..12).map(|index| format!("{name}-pack-{index:02}")));
            true
        });
    }
}

fn legacy_evaluation_profile_frame() -> u64 {
    let scorebox = runtime_scorebox_view(true, true, true);
    let session = runtime_session_snapshot();
    let mut checksum = session.play_style as u64 ^ ((session.player_side as u64) << 8);
    for side in [PlayerSide::P1, PlayerSide::P2] {
        let side_idx = player_side_index(side);
        let profile = runtime_profile_for_side(side);
        let (avatar, display_name) = runtime_footer_fields_for_side(side);
        checksum = checksum.rotate_left(9)
            ^ evaluation_player_checksum(
                session.side_joined(side),
                runtime_session_side_guest(side),
                avatar.as_deref(),
                display_name.as_str(),
                !profile.groovestats_api_key.trim().is_empty(),
                !profile.arrowcloud_api_key.trim().is_empty(),
            );
        let player = black_box(scorebox.sides[side_idx].clone());
        checksum = checksum.rotate_left(11) ^ scorebox_player_checksum(&player);
    }
    checksum
}

fn shared_evaluation_profile_frame() -> u64 {
    let (scorebox, avatars) = runtime_evaluation_profile_view(true, true, true);
    let mut checksum = scorebox.play_style as u64 ^ ((scorebox.player_side as u64) << 8);
    for (player, avatar) in scorebox.sides.into_iter().zip(avatars) {
        checksum = checksum.rotate_left(9)
            ^ evaluation_player_checksum(
                player.joined,
                player.guest,
                avatar.as_deref(),
                player.display_name.as_str(),
                !player.leaderboard.api_key().trim().is_empty(),
                !player.leaderboard.arrowcloud_api_key().trim().is_empty(),
            );
        checksum = checksum.rotate_left(11) ^ scorebox_player_checksum(&player);
    }
    checksum
}

fn evaluation_player_checksum(
    joined: bool,
    guest: bool,
    avatar: Option<&str>,
    display_name: &str,
    groovestats_linked: bool,
    arrowcloud_linked: bool,
) -> u64 {
    joined as u64
        ^ ((guest as u64) << 1)
        ^ ((groovestats_linked as u64) << 2)
        ^ ((arrowcloud_linked as u64) << 3)
        ^ ((avatar.map_or(0, str::len) as u64) << 8)
        ^ ((display_name.len() as u64) << 32)
}

fn scorebox_player_checksum(player: &ScoreboxProfileView) -> u64 {
    let leaderboard = &player.leaderboard;
    player.joined as u64
        ^ ((player.guest as u64) << 1)
        ^ ((leaderboard.display_scorebox as u64) << 2)
        ^ ((leaderboard.gs_active as u64) << 3)
        ^ ((leaderboard.show_ex_score as u64) << 4)
        ^ ((leaderboard.include_arrowcloud() as u64) << 5)
        ^ ((leaderboard.should_auto_populate() as u64) << 6)
        ^ ((player.display_name.len() as u64) << 8)
        ^ ((player.groovestats_username.len() as u64) << 16)
        ^ ((player.player_initials.len() as u64) << 24)
        ^ ((leaderboard.api_key().len() as u64) << 32)
        ^ ((leaderboard.arrowcloud_api_key().len() as u64) << 40)
        ^ ((leaderboard.gs_username().len() as u64) << 48)
        ^ ((leaderboard.persistent_profile_id().map_or(0, str::len) as u64) << 56)
        ^ (leaderboard.auto_profile_id().map_or(0, str::len) as u64).rotate_left(13)
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
