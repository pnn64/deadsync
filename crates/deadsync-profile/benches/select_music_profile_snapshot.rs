use deadsync_profile::{
    ActiveProfile, PlayStyle, PlayerSide, Profile, SessionState, music_profile_snapshot,
    player_side_for_index, player_side_index, scorebox_runtime_view, session_players_view,
    side_for_physical_pad,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const OPS: usize = 5_000;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates unchanged to `System`; relaxed counters
// observe successful requests only while the single-threaded bench gate is on.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
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
        // SAFETY: the pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    frees: u64,
    alloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    fn churn(self) -> u64 {
        self.alloc_bytes + self.free_bytes
    }
}

struct ResultRow {
    ns: f64,
    cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn fixture() -> ([Profile; 2], SessionState) {
    let mut profiles = [Profile::default(), Profile::default()];
    for (side, profile) in profiles.iter_mut().enumerate() {
        profile.display_name = format!("Player {side}");
        profile.player_initials = format!("P{side}");
        profile.avatar_texture_key = Some(format!("avatar:{side}"));
        profile.groovestats_username = format!("online-{side}");
        profile.groovestats_api_key = format!("secret-{side}");
        for item in 0..250 {
            profile.favorites.insert(format!("chart-{side}-{item}"));
            profile
                .favorited_packs
                .insert(format!("pack-{side}-{item}"));
            profile
                .favorited_series
                .insert(format!("series-{side}-{item}"));
        }
    }
    let session = SessionState {
        active_profiles: [
            ActiveProfile::Local {
                id: "profile-p1".to_string(),
            },
            ActiveProfile::Local {
                id: "profile-p2".to_string(),
            },
        ],
        joined_mask: 3,
        music_rate: 1.25,
        play_style: PlayStyle::Versus,
        player_side: PlayerSide::P1,
        ..SessionState::default()
    };
    (profiles, session)
}

fn legacy(profiles: &[Profile; 2], session: &SessionState) -> u64 {
    let scorebox = scorebox_runtime_view(
        profiles,
        &session.active_profiles,
        session.joined_mask,
        session.play_style,
        session.player_side,
        true,
        true,
        true,
    );
    let players = session_players_view(profiles, session.joined_mask, session.player_side);
    let avatars: [Option<String>; 2] = std::array::from_fn(|side| {
        let full_profile = profiles[side].clone();
        full_profile.avatar_texture_key.clone()
    });
    let local_ids: [Option<String>; 2] = std::array::from_fn(|side| {
        session
            .active_local_profile_id(player_side_for_index(side))
            .map(str::to_owned)
    });
    let pad_ids: [Option<String>; 2] = std::array::from_fn(|pad| {
        let side = side_for_physical_pad(session.play_style, session.player_side, pad == 1);
        session.active_local_profile_id(side).map(str::to_owned)
    });
    checksum(
        scorebox
            .sides
            .each_ref()
            .map(|side| side.display_name.as_str()),
        players.display_names.each_ref().map(String::as_str),
        avatars.each_ref().map(Option::as_deref),
        local_ids.each_ref().map(Option::as_deref),
        pad_ids.each_ref().map(Option::as_deref),
    )
}

fn current(profiles: &[Profile; 2], session: &SessionState) -> u64 {
    let snapshot = music_profile_snapshot(profiles, session, true, true, true);
    let display_names = snapshot
        .scorebox
        .sides
        .each_ref()
        .map(|side| side.display_name.clone());
    checksum(
        snapshot
            .scorebox
            .sides
            .each_ref()
            .map(|side| side.display_name.as_str()),
        display_names.each_ref().map(String::as_str),
        snapshot
            .avatar_texture_keys
            .each_ref()
            .map(Option::as_deref),
        snapshot.local_profile_ids.each_ref().map(Option::as_deref),
        snapshot.pad_profile_ids.each_ref().map(Option::as_deref),
    )
}

fn checksum(
    scorebox_names: [&str; 2],
    display_names: [&str; 2],
    avatars: [Option<&str>; 2],
    local_ids: [Option<&str>; 2],
    pad_ids: [Option<&str>; 2],
) -> u64 {
    let mut value = 0u64;
    for side in 0..2 {
        value = value
            .wrapping_mul(131)
            .wrapping_add(scorebox_names[side].len() as u64)
            .wrapping_add(display_names[side].len() as u64)
            .wrapping_add(avatars[side].map_or(0, str::len) as u64)
            .wrapping_add(local_ids[side].map_or(0, str::len) as u64)
            .wrapping_add(pad_ids[side].map_or(0, str::len) as u64)
            .wrapping_add(player_side_index(player_side_for_index(side)) as u64);
    }
    value
}

fn measure(mut op: impl FnMut() -> u64) -> ResultRow {
    for _ in 0..1_000 {
        black_box(op());
    }
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..OPS {
        checksum = checksum.wrapping_add(black_box(op()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..OPS {
        black_box(op());
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);

    ResultRow {
        ns: elapsed.as_secs_f64() * 1e9 / OPS as f64,
        cycles: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / OPS as f64),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn main() {
    let (profiles, session) = fixture();
    let old = measure(|| legacy(black_box(&profiles), black_box(&session)));
    let new = measure(|| current(black_box(&profiles), black_box(&session)));
    assert_eq!(old.checksum, new.checksum);
    println!("Select Music profile capture (2 profiles, 750 set entries/profile)");
    print("old", &old);
    print("new", &new);
    println!(
        "  change: {:+.2}% latency  {:+.2}% cycles  {:+.2}% churn",
        change(old.ns, new.ns),
        change(
            old.cycles.unwrap_or(f64::NAN),
            new.cycles.unwrap_or(f64::NAN)
        ),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print(label: &str, row: &ResultRow) {
    println!(
        "  {label:<3} {:>10.2} ns/op  {:>10.2} cycles/op  {:>8.3} Mop/s  \
         {:>6.2} alloc/op  {:>6.2} free/op  {:>12.1} churn B/op",
        row.ns,
        row.cycles.unwrap_or(f64::NAN),
        1_000.0 / row.ns,
        row.alloc.allocs as f64 / OPS as f64,
        row.alloc.frees as f64 / OPS as f64,
        row.alloc.churn() as f64 / OPS as f64,
    );
}

fn change(old: f64, new: f64) -> f64 {
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
