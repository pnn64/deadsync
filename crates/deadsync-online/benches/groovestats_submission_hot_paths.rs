use deadsync_online::groovestats::{
    GROOVESTATS_CHART_HASH_VERSION, GROOVESTATS_COMMENT_PREFIX, GrooveStatsJudgmentCounts,
    GrooveStatsRescoreCounts, manual_qr_url, player_options_json, submit_comment,
    timing_windows_comment,
};
use deadsync_profile::{Profile, RemoveMask, TimingWindowsOption};
use deadsync_rules::scroll::ScrollSpeedSetting;
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 31;
const WARMUPS: usize = 5;
const OPS_PER_SAMPLE: usize = 4;
const BATCH: usize = 64;

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

// SAFETY: every allocation operation delegates unchanged to `System`; the
// relaxed counters are enabled only around a single-threaded benchmark batch.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
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
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.allocated_bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            } else {
                self.freed_bytes
                    .fetch_add((old.size() - new_size) as u64, Ordering::Relaxed);
            }
        }
        out
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

struct BenchResult {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure_pair(
    mut old_op: impl FnMut() -> u64,
    mut new_op: impl FnMut() -> u64,
) -> (BenchResult, BenchResult) {
    for _ in 0..WARMUPS {
        black_box(old_op());
        black_box(new_op());
    }

    let mut old_times = Vec::with_capacity(SAMPLES);
    let mut new_times = Vec::with_capacity(SAMPLES);
    let mut old_cycles = Vec::with_capacity(SAMPLES);
    let mut new_cycles = Vec::with_capacity(SAMPLES);
    let mut old_checksum = 0;
    let mut new_checksum = 0;
    for sample in 0..SAMPLES {
        let (old_sample, new_sample) = if sample.is_multiple_of(2) {
            (timed_sample(&mut old_op), timed_sample(&mut new_op))
        } else {
            let new_sample = timed_sample(&mut new_op);
            let old_sample = timed_sample(&mut old_op);
            (old_sample, new_sample)
        };
        old_times.push(old_sample.0);
        new_times.push(new_sample.0);
        if let Some(cycles) = old_sample.1 {
            old_cycles.push(cycles);
        }
        if let Some(cycles) = new_sample.1 {
            new_cycles.push(cycles);
        }
        old_checksum ^= old_sample.2;
        new_checksum ^= new_sample.2;
    }

    let old_allocated = measured_allocations(&mut old_op);
    let new_allocated = measured_allocations(&mut new_op);
    (
        bench_result(old_times, old_cycles, old_allocated, old_checksum),
        bench_result(new_times, new_cycles, new_allocated, new_checksum),
    )
}

fn timed_sample(op: &mut impl FnMut() -> u64) -> (f64, Option<f64>, u64) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..OPS_PER_SAMPLE {
        checksum = checksum.wrapping_add(black_box(op()));
    }
    let elapsed = started.elapsed().as_secs_f64() * 1e9 / OPS_PER_SAMPLE as f64;
    let cycles = cycle_start
        .zip(cycle_counter())
        .map(|(start, end)| end.wrapping_sub(start) as f64 / OPS_PER_SAMPLE as f64);
    (elapsed, cycles, checksum)
}

fn measured_allocations(op: &mut impl FnMut() -> u64) -> AllocSnapshot {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    ALLOC.snapshot().delta(before)
}

fn bench_result(
    mut times: Vec<f64>,
    mut cycles: Vec<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
) -> BenchResult {
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);
    BenchResult {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        allocated,
        checksum,
    }
}

fn checksum(text: &str) -> u64 {
    text.bytes().fold(text.len() as u64, |sum, byte| {
        sum.wrapping_mul(131).wrapping_add(u64::from(byte))
    })
}

fn old_compact_f32_text(value: f32) -> String {
    let mut text = format!("{value:.2}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

fn old_submit_comment(
    counts: &GrooveStatsJudgmentCounts,
    fa_plus_ex_score: Option<f64>,
    music_rate: f32,
    timing_windows: TimingWindowsOption,
    scroll_speed: ScrollSpeedSetting,
) -> String {
    let mut parts = Vec::with_capacity(11);
    if let Some(ex_score) = fa_plus_ex_score {
        parts.push("FA+".to_string());
        parts.push(format!("{ex_score:.2}EX"));
    }
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    if (rate - 1.0).abs() > 0.0001 {
        parts.push(format!("{}x Rate", old_compact_f32_text(rate)));
    }
    for (count, suffix) in [
        (counts.fantastic, "w"),
        (counts.excellent, "e"),
        (counts.great, "g"),
        (counts.decent_count(), "d"),
        (counts.way_off_count(), "wo"),
        (counts.miss, "m"),
    ] {
        if count != 0 {
            parts.push(format!("{count}{suffix}"));
        }
    }
    if let Some(timing_windows) = timing_windows_comment(timing_windows) {
        parts.push(timing_windows.to_string());
    }
    if let ScrollSpeedSetting::CMod(value) = scroll_speed {
        parts.push(format!("C{}", old_compact_f32_text(value)));
    }
    if parts.is_empty() {
        GROOVESTATS_COMMENT_PREFIX.to_string()
    } else {
        format!("{GROOVESTATS_COMMENT_PREFIX}, {}", parts.join(", "))
    }
}

fn old_manual_qr_url(
    base_url: &str,
    chart_hash: &str,
    counts: &GrooveStatsJudgmentCounts,
    rescored: &GrooveStatsRescoreCounts,
    rate: u32,
    used_cmod: bool,
) -> Option<String> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    let mut rescored_str = String::with_capacity(24);
    for (label, value) in [
        ('G', rescored.fantastic_plus),
        ('H', rescored.fantastic),
        ('I', rescored.excellent),
        ('J', rescored.great),
        ('K', rescored.decent),
        ('L', rescored.way_off),
    ] {
        if value != 0 {
            rescored_str.push(label);
            rescored_str.push_str(format!("{value:x}").as_str());
        }
    }
    Some(format!(
        "{}/QR/{hash}/T{:x}G{:x}H{:x}I{:x}J{:x}K{:x}L{:x}M{:x}H{:x}T{:x}R{:x}T{:x}M{:x}T{:x}{rescored_str}/F0R{:x}C{}V{:x}",
        base_url.trim_end_matches('/'),
        counts.total_steps,
        counts.fantastic_plus,
        counts.fantastic,
        counts.excellent,
        counts.great,
        counts.decent_count(),
        counts.way_off_count(),
        counts.miss,
        counts.holds_held,
        counts.total_holds,
        counts.rolls_held,
        counts.total_rolls,
        counts.mines_hit,
        counts.total_mines,
        rate,
        if used_cmod { '1' } else { '0' },
        GROOVESTATS_CHART_HASH_VERSION,
    ))
}

fn old_player_options_json(profile: &Profile) -> String {
    let (speed_mod_type, speed_mod) = match profile.scroll_speed {
        ScrollSpeedSetting::XMod(value) => (1, value),
        ScrollSpeedSetting::CMod(value) => (2, value),
        ScrollSpeedSetting::MMod(value) => (3, value),
    };
    let mut options = JsonMap::with_capacity(18);
    options.insert("SpeedModType".to_string(), JsonValue::from(speed_mod_type));
    options.insert(
        "SpeedMod".to_string(),
        JsonValue::from(f64::from(speed_mod)),
    );
    options.insert(
        "BackgroundFilter".to_string(),
        JsonValue::from(profile.background_filter.percent()),
    );
    for (key, value) in [
        ("HideTargets", profile.hide_targets),
        ("HideSongBG", profile.hide_song_bg),
        ("HideCombo", profile.hide_combo),
        ("HideLifebar", profile.hide_lifebar),
        ("HideScore", profile.hide_score),
        ("HideDanger", profile.hide_danger),
        ("HideComboExplosions", profile.hide_combo_explosions),
        ("ColumnFlashOnMiss", profile.column_flash_on_miss),
        ("SubtractiveScoring", profile.subtractive_scoring),
    ] {
        options.insert(key.to_string(), JsonValue::from(value));
    }
    options.insert("Mini".to_string(), JsonValue::from(profile.mini_percent));
    options.insert(
        "VisualDelay".to_string(),
        JsonValue::from(profile.visual_delay_ms),
    );
    options.insert("Cover".to_string(), JsonValue::from(profile.hide_song_bg));
    options.insert(
        "NoMines".to_string(),
        JsonValue::from(profile.remove_active_mask.contains(RemoveMask::NO_MINES)),
    );
    options.insert(
        "Reverse".to_string(),
        JsonValue::from(
            profile
                .scroll_option
                .contains(deadsync_profile::ScrollOption::Reverse),
        ),
    );
    options.insert(
        "ShowFaPlusWindow".to_string(),
        JsonValue::from(profile.show_fa_plus_window),
    );
    options.insert(
        "ShowExScore".to_string(),
        JsonValue::from(profile.show_ex_score),
    );
    options.insert(
        "ShowFaPlusPane".to_string(),
        JsonValue::from(profile.show_fa_plus_pane),
    );
    serde_json::to_string(&JsonValue::Object(options)).expect("serialize old player options")
}

fn comment_batch(counts: &GrooveStatsJudgmentCounts, new: bool) -> u64 {
    let mut sum = 0u64;
    for index in 0..BATCH {
        let comment = if new {
            submit_comment(
                counts,
                Some(99.5 + index as f64 / 100.0),
                1.5,
                TimingWindowsOption::DecentsAndWayOffs,
                ScrollSpeedSetting::CMod(650.0),
            )
        } else {
            old_submit_comment(
                counts,
                Some(99.5 + index as f64 / 100.0),
                1.5,
                TimingWindowsOption::DecentsAndWayOffs,
                ScrollSpeedSetting::CMod(650.0),
            )
        };
        sum = sum.wrapping_add(checksum(&comment));
    }
    sum
}

fn qr_batch(
    counts: &GrooveStatsJudgmentCounts,
    rescored: &GrooveStatsRescoreCounts,
    new: bool,
) -> u64 {
    let mut sum = 0u64;
    for index in 0..BATCH {
        let hash = if index.is_multiple_of(2) {
            " deadbeef0123456789 "
        } else {
            " abcdef9876543210 "
        };
        let url = if new {
            manual_qr_url(
                "https://www.groovestats.com/",
                hash,
                counts,
                rescored,
                150,
                true,
            )
        } else {
            old_manual_qr_url(
                "https://www.groovestats.com/",
                hash,
                counts,
                rescored,
                150,
                true,
            )
        }
        .expect("fixture QR URL");
        sum = sum.wrapping_add(checksum(&url));
    }
    sum
}

fn options_batch(profile: &Profile, new: bool) -> u64 {
    let mut sum = 0u64;
    for _ in 0..BATCH {
        let json = if new {
            player_options_json(profile)
        } else {
            old_player_options_json(profile)
        };
        sum = sum.wrapping_add(checksum(&json));
    }
    sum
}

fn run_workload(
    name: &str,
    units_per_batch: usize,
    old_op: impl FnMut() -> u64,
    new_op: impl FnMut() -> u64,
) {
    let (old, new) = measure_pair(old_op, new_op);
    assert_eq!(old.checksum, new.checksum, "{name} checksum diverged");

    println!("{name}");
    print_result("old", &old, units_per_batch);
    print_result("new", &new, units_per_batch);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% p95  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% allocs  {:>7.2}% reallocs  {:>7.2}% frees  {:>7.2}% bytes  {:>7.2}% churn",
        improvement(old.median_ns, new.median_ns),
        improvement(old.p95_ns, new.p95_ns),
        improvement(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        percent_change(
            throughput(&old, units_per_batch),
            throughput(&new, units_per_batch),
        ),
        improvement(old.allocated.allocs as f64, new.allocated.allocs as f64),
        improvement(old.allocated.reallocs as f64, new.allocated.reallocs as f64,),
        improvement(old.allocated.frees as f64, new.allocated.frees as f64),
        improvement(
            old.allocated.allocated_bytes as f64,
            new.allocated.allocated_bytes as f64,
        ),
        improvement(old.allocated.churn() as f64, new.allocated.churn() as f64),
    );

    assert!(new.median_ns < old.median_ns, "{name} median regressed");
    assert!(new.p95_ns < old.p95_ns, "{name} p95 regressed");
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "{name} cycles regressed");
    }
    assert!(new.allocated.allocs < old.allocated.allocs, "{name} allocs");
    assert!(
        new.allocated.reallocs <= old.allocated.reallocs,
        "{name} reallocs"
    );
    assert!(new.allocated.frees < old.allocated.frees, "{name} frees");
    assert!(
        new.allocated.allocated_bytes < old.allocated.allocated_bytes,
        "{name} allocated bytes"
    );
    assert!(
        new.allocated.churn() < old.allocated.churn(),
        "{name} allocation churn"
    );
}

fn print_result(label: &str, result: &BenchResult, units_per_batch: usize) {
    println!(
        "  {label:<3} {:>11.1} ns  p95 {:>11.1} ns  {:>11.1} cycles  {:>12.0} items/s  \
         {:>6} alloc  {:>6} realloc  {:>6} free  {:>10} B alloc  {:>10} B churn",
        result.median_ns,
        result.p95_ns,
        result.median_cycles.unwrap_or(f64::NAN),
        throughput(result, units_per_batch),
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.allocated_bytes,
        result.allocated.churn(),
    );
}

fn throughput(result: &BenchResult, units_per_batch: usize) -> f64 {
    units_per_batch as f64 * 1e9 / result.median_ns
}

fn improvement(old: f64, new: f64) -> f64 {
    (1.0 - new / old) * 100.0
}

fn percent_change(old: f64, new: f64) -> f64 {
    (new / old - 1.0) * 100.0
}

fn main() {
    let counts = GrooveStatsJudgmentCounts {
        fantastic_plus: 11,
        fantastic: 721,
        excellent: 34,
        great: 5,
        decent: Some(2),
        way_off: Some(1),
        miss: 3,
        total_steps: 777,
        holds_held: 21,
        total_holds: 22,
        mines_hit: 2,
        total_mines: 14,
        rolls_held: 7,
        total_rolls: 8,
    };
    let rescored = GrooveStatsRescoreCounts {
        fantastic_plus: 17,
        fantastic: 5,
        excellent: 3,
        great: 2,
        decent: 1,
        way_off: 1,
    };
    let mut profile = Profile {
        scroll_speed: ScrollSpeedSetting::CMod(650.0),
        background_filter: "55".parse().expect("background filter"),
        hide_targets: true,
        hide_song_bg: true,
        hide_combo: true,
        hide_lifebar: true,
        hide_score: true,
        hide_danger: true,
        hide_combo_explosions: true,
        column_flash_on_miss: true,
        subtractive_scoring: true,
        mini_percent: 37,
        visual_delay_ms: -12,
        show_fa_plus_window: true,
        show_ex_score: true,
        show_fa_plus_pane: true,
        ..Profile::default()
    };
    profile.remove_active_mask |= RemoveMask::NO_MINES;
    profile.scroll_option = profile
        .scroll_option
        .union(deadsync_profile::ScrollOption::Reverse);

    assert_eq!(comment_batch(&counts, false), comment_batch(&counts, true));
    assert_eq!(
        qr_batch(&counts, &rescored, false),
        qr_batch(&counts, &rescored, true)
    );
    assert_eq!(
        options_batch(&profile, false),
        options_batch(&profile, true)
    );

    run_workload(
        "submit comment streaming (64 comments)",
        BATCH,
        || comment_batch(black_box(&counts), false),
        || comment_batch(black_box(&counts), true),
    );
    run_workload(
        "manual QR streaming (64 URLs)",
        BATCH,
        || qr_batch(black_box(&counts), black_box(&rescored), false),
        || qr_batch(black_box(&counts), black_box(&rescored), true),
    );
    run_workload(
        "player options direct serialization (64 payloads)",
        BATCH,
        || options_batch(black_box(&profile), false),
        || options_batch(black_box(&profile), true),
    );
    run_workload(
        "combined GrooveStats submission assembly (64 submissions)",
        BATCH,
        || {
            comment_batch(black_box(&counts), false)
                ^ qr_batch(black_box(&counts), black_box(&rescored), false)
                ^ options_batch(black_box(&profile), false)
        },
        || {
            comment_batch(black_box(&counts), true)
                ^ qr_batch(black_box(&counts), black_box(&rescored), true)
                ^ options_batch(black_box(&profile), true)
        },
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
