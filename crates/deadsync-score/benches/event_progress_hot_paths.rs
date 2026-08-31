use deadsync_score::{
    EventOverlayPage, EventProgress, EventProgressKind, LeaderboardEntry, SubmitAchievement,
    SubmitAchievementReward, SubmitProgress, SubmitQuest, SubmitQuestReward, SubmitStatImprovement,
    clear_type_name, event_progress_overlay_pages,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

const QUESTS: usize = 8;
const ACHIEVEMENTS: usize = 8;
const LEADERBOARD_ENTRIES: usize = 20;
const OPERATIONS: usize = 16;
const SAMPLES: usize = 31;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            realloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocation requests are delegated unchanged to `System`; relaxed
// counters are benchmark-only observations while the single-threaded gate is on.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied a valid layout.
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
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.realloc_bytes
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
    alloc_bytes: u64,
    realloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn heap_calls(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn allocated_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes
    }

    const fn churn(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

#[derive(Clone, Copy, Default)]
struct OutputStats {
    checksum: u64,
    bytes: usize,
    pages: usize,
}

struct Measurement {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    output: OutputStats,
}

#[derive(Default)]
struct Samples {
    ns: Vec<f64>,
    cycles: Vec<f64>,
    checksum: u64,
    output: OutputStats,
}

fn output_stats(pages: Vec<EventOverlayPage>) -> OutputStats {
    let mut stats = OutputStats {
        pages: pages.len(),
        ..OutputStats::default()
    };
    for page in pages {
        match page {
            EventOverlayPage::Text(text) => {
                stats.bytes += text.len();
                stats.checksum = text.bytes().fold(stats.checksum, |checksum, byte| {
                    checksum.rotate_left(5) ^ u64::from(byte)
                });
            }
            EventOverlayPage::Leaderboard(entries) => {
                for entry in entries {
                    stats.bytes += entry.name.len() + entry.date.len();
                    stats.checksum = entry
                        .name
                        .bytes()
                        .fold(stats.checksum ^ u64::from(entry.rank), |checksum, byte| {
                            checksum.rotate_left(7) ^ u64::from(byte)
                        })
                        ^ entry.score.to_bits();
                }
            }
        }
    }
    stats
}

fn record_sample(samples: &mut Samples, build: &mut impl FnMut() -> Vec<EventOverlayPage>) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    for _ in 0..OPERATIONS {
        let output = output_stats(build());
        samples.output = output;
        samples.checksum = samples.checksum.wrapping_add(black_box(output.checksum));
    }
    samples
        .ns
        .push(started.elapsed().as_secs_f64() * 1e9 / OPERATIONS as f64);
    if let Some(elapsed) = cycle_start
        .zip(cycle_counter())
        .map(|(start, end)| end.wrapping_sub(start) as f64 / OPERATIONS as f64)
    {
        samples.cycles.push(elapsed);
    }
}

fn allocation_sample(
    build: &mut impl FnMut() -> Vec<EventOverlayPage>,
) -> (AllocSnapshot, OutputStats) {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let output = black_box(output_stats(build()));
    ALLOC.enabled.store(false, Ordering::Relaxed);
    (ALLOC.snapshot().delta(before), output)
}

fn finish(mut samples: Samples, alloc: AllocSnapshot, output: OutputStats) -> Measurement {
    samples.ns.sort_by(f64::total_cmp);
    samples.cycles.sort_by(f64::total_cmp);
    Measurement {
        median_ns: samples.ns[SAMPLES / 2],
        p95_ns: samples.ns[SAMPLES * 95 / 100],
        median_cycles: (!samples.cycles.is_empty())
            .then(|| samples.cycles[samples.cycles.len() / 2]),
        alloc,
        output,
    }
}

fn measure_pair(
    mut old_build: impl FnMut() -> Vec<EventOverlayPage>,
    mut new_build: impl FnMut() -> Vec<EventOverlayPage>,
) -> (Measurement, Measurement) {
    for _ in 0..3 {
        black_box(output_stats(old_build()));
        black_box(output_stats(new_build()));
    }
    let mut old_samples = Samples {
        ns: Vec::with_capacity(SAMPLES),
        cycles: Vec::with_capacity(SAMPLES),
        ..Samples::default()
    };
    let mut new_samples = Samples {
        ns: Vec::with_capacity(SAMPLES),
        cycles: Vec::with_capacity(SAMPLES),
        ..Samples::default()
    };
    for sample in 0..SAMPLES {
        if sample.is_multiple_of(2) {
            record_sample(&mut old_samples, &mut old_build);
            record_sample(&mut new_samples, &mut new_build);
        } else {
            record_sample(&mut new_samples, &mut new_build);
            record_sample(&mut old_samples, &mut old_build);
        }
    }
    let (old_alloc, old_output) = allocation_sample(&mut old_build);
    let (new_alloc, new_output) = allocation_sample(&mut new_build);
    (
        finish(old_samples, old_alloc, old_output),
        finish(new_samples, new_alloc, new_output),
    )
}

fn old_trim_blank_lines(text: String) -> String {
    text.trim_end_matches(['\n', '\r']).to_string()
}

fn old_capitalize_ascii_first(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::new();
    out.extend(first.to_uppercase());
    out.extend(chars);
    out
}

fn old_stat_improvement_lines(progress: Option<&SubmitProgress>) -> Vec<String> {
    let Some(progress) = progress else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for improvement in &progress.stat_improvements {
        if improvement.gained == 0 {
            continue;
        }
        if improvement.name.eq_ignore_ascii_case("clearType") {
            let after = improvement.current.clamp(0, i32::from(u8::MAX)) as u8;
            let before = after.saturating_sub(improvement.gained.min(u32::from(u8::MAX)) as u8);
            lines.push(format!(
                "Clear Type: {} >>> {}",
                clear_type_name(before),
                clear_type_name(after)
            ));
            continue;
        }
        if improvement.name.eq_ignore_ascii_case("grade") {
            let curr = improvement.current;
            let prev = curr - improvement.gained as i32;
            if curr != 0 && prev != curr {
                let grade = match curr {
                    1 => Some("Quad"),
                    2 => Some("Quint"),
                    _ => None,
                };
                if let Some(grade) = grade {
                    lines.push(format!("New {grade}!"));
                }
            }
            continue;
        }
        let stat_name = old_capitalize_ascii_first(improvement.name.trim_end_matches("Level"));
        lines.push(format!(
            "{stat_name} Lvl: {} (+{})",
            improvement.current, improvement.gained
        ));
    }
    lines
}

fn old_itl_summary_page_text(
    progress: &EventProgress,
    submit_progress: Option<&SubmitProgress>,
) -> String {
    let mut text = format!(
        "EX Score: {:.2}% ({:+.2}%)\n\
         Points: {} ({:+})\n\n\
         Ranking Points: {} ({:+})\n\
         Song Points: {} ({:+})\n\
         EX Points: {} ({:+})\n\
         Total Points: {} ({:+})\n\n\
         You've passed the chart {} times",
        f64::from(progress.score_hundredths) / 100.0,
        f64::from(progress.score_delta_hundredths) / 100.0,
        progress.current_points,
        progress.point_delta,
        progress.current_ranking_points,
        progress.ranking_delta,
        progress.current_song_points,
        progress.song_delta,
        progress.current_ex_points,
        progress.ex_delta,
        progress.current_total_points,
        progress.total_delta,
        progress.total_passes,
    );
    let lines = old_stat_improvement_lines(submit_progress);
    if !lines.is_empty() {
        text.push_str("\n\n");
        text.push_str(lines.join("\n").as_str());
    }
    old_trim_blank_lines(text)
}

fn old_append_grouped_reward_text(out: &mut String, reward_type: &str, descriptions: &[String]) {
    if descriptions.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    if !reward_type.eq_ignore_ascii_case("ad-hoc") {
        out.push_str(reward_type.trim().to_ascii_uppercase().as_str());
        out.push_str(":\n");
    }
    out.push_str(descriptions.join("\n").as_str());
}

fn old_quest_page_text(quest: &SubmitQuest) -> String {
    let mut body = format!("Completed \"{}\"!", quest.title.trim());
    let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
    for reward in &quest.rewards {
        let reward_type = reward.reward_type.trim();
        let description = reward.description.trim();
        if description.is_empty() {
            continue;
        }
        if let Some((_, descriptions)) = grouped
            .iter_mut()
            .find(|(kind, _)| kind.eq_ignore_ascii_case(reward_type))
        {
            descriptions.push(description.to_string());
        } else {
            grouped.push((reward_type.to_string(), vec![description.to_string()]));
        }
    }
    for (reward_type, descriptions) in &grouped {
        old_append_grouped_reward_text(&mut body, reward_type.as_str(), descriptions.as_slice());
    }
    old_trim_blank_lines(body)
}

fn old_achievement_page_text(achievement: &SubmitAchievement) -> String {
    let mut lines = vec![format!(
        "Completed the \"{}\" Achievement!",
        achievement.title.trim()
    )];
    for reward in &achievement.rewards {
        let tier = reward.tier.trim();
        if !tier.is_empty() && tier != "0" {
            lines.push(format!("Tier {tier}"));
        }
        for requirement in &reward.requirements {
            let requirement = requirement.trim();
            if !requirement.is_empty() {
                lines.push(requirement.to_string());
            }
        }
        let title = reward.title_unlocked.trim();
        if !title.is_empty() {
            lines.push(format!("Unlocked the \"{title}\" Title!"));
        }
        lines.push(String::new());
    }
    old_trim_blank_lines(lines.join("\n"))
}

fn old_event_progress_overlay_pages(
    progress: &EventProgress,
    submit_progress: Option<&SubmitProgress>,
    leaderboard: &[LeaderboardEntry],
) -> Vec<EventOverlayPage> {
    let mut pages = vec![EventOverlayPage::Text(old_itl_summary_page_text(
        progress,
        submit_progress,
    ))];
    let Some(submit_progress) = submit_progress else {
        pages.push(EventOverlayPage::Leaderboard(leaderboard.to_vec()));
        return pages;
    };
    for quest in &submit_progress.quests_completed {
        pages.push(EventOverlayPage::Text(old_quest_page_text(quest)));
    }
    for achievement in &submit_progress.achievements_completed {
        pages.push(EventOverlayPage::Text(old_achievement_page_text(
            achievement,
        )));
    }
    pages.push(EventOverlayPage::Leaderboard(leaderboard.to_vec()));
    pages
}

fn assert_page_parity(old: &[EventOverlayPage], new: &[EventOverlayPage]) {
    assert_eq!(old.len(), new.len(), "page count diverged");
    for (old, new) in old.iter().zip(new) {
        match (old, new) {
            (EventOverlayPage::Text(old), EventOverlayPage::Text(new)) => {
                assert_eq!(old, new, "text page diverged");
            }
            (EventOverlayPage::Leaderboard(old), EventOverlayPage::Leaderboard(new)) => {
                assert_eq!(old.len(), new.len(), "leaderboard length diverged");
                for (old, new) in old.iter().zip(new) {
                    assert_eq!(old.rank, new.rank);
                    assert_eq!(old.name, new.name);
                    assert_eq!(old.machine_tag, new.machine_tag);
                    assert_eq!(old.score.to_bits(), new.score.to_bits());
                    assert_eq!(old.date, new.date);
                    assert_eq!(old.is_rival, new.is_rival);
                    assert_eq!(old.is_self, new.is_self);
                    assert_eq!(old.is_fail, new.is_fail);
                }
            }
            _ => panic!("page kind diverged"),
        }
    }
}

fn fixture() -> (EventProgress, SubmitProgress, Vec<LeaderboardEntry>) {
    let stat_improvements = (0..20)
        .map(|index| SubmitStatImprovement {
            name: match index {
                0 => "clearType".to_owned(),
                1 => "grade".to_owned(),
                _ => format!("stream{index}Level"),
            },
            gained: 1 + index as u32,
            current: match index {
                0 => 5,
                1 => 2,
                _ => 20 + index,
            },
        })
        .collect();
    let quests_completed = (0..QUESTS)
        .map(|quest| SubmitQuest {
            title: format!("Quest {quest:02}"),
            rewards: (0..12)
                .map(|reward| SubmitQuestReward {
                    reward_type: match reward % 4 {
                        0 => "pack",
                        1 => "song",
                        2 => "ad-hoc",
                        _ => "cosmetic",
                    }
                    .to_owned(),
                    description: format!("Reward {quest:02}-{reward:02} with retained detail"),
                })
                .collect(),
        })
        .collect();
    let achievements_completed = (0..ACHIEVEMENTS)
        .map(|achievement| SubmitAchievement {
            title: format!("Achievement {achievement:02}"),
            rewards: (0..4)
                .map(|reward| SubmitAchievementReward {
                    tier: (reward + 1).to_string(),
                    requirements: (0..3)
                        .map(|requirement| {
                            format!("Requirement {achievement:02}-{reward:02}-{requirement:02}")
                        })
                        .collect(),
                    title_unlocked: format!("Title {achievement:02}-{reward:02}"),
                })
                .collect(),
        })
        .collect();
    let submit = SubmitProgress {
        stat_improvements,
        skill_improvements: Vec::new(),
        quests_completed,
        achievements_completed,
    };
    let progress = EventProgress {
        kind: EventProgressKind::Itl,
        score_hundredths: 9_912,
        score_delta_hundredths: 34,
        current_points: 1_234,
        point_delta: 56,
        current_ranking_points: 2_345,
        ranking_delta: 67,
        current_song_points: 3_456,
        song_delta: 78,
        current_ex_points: 4_567,
        ex_delta: 89,
        current_total_points: 9_999,
        total_delta: 290,
        total_passes: 42,
        ..EventProgress::default()
    };
    let leaderboard = (0..LEADERBOARD_ENTRIES)
        .map(|index| LeaderboardEntry {
            rank: index as u32 + 1,
            name: format!("Player {index:02}"),
            machine_tag: Some(format!("CAB-{index:02}")),
            score: 100.0 - index as f64 / 10.0,
            date: format!("2026-08-{:02}", index + 1),
            is_rival: index.is_multiple_of(3),
            is_self: index == 4,
            is_fail: false,
        })
        .collect();
    (progress, submit, leaderboard)
}

fn main() {
    let (progress, submit, leaderboard) = fixture();
    let old_pages = old_event_progress_overlay_pages(&progress, Some(&submit), &leaderboard);
    let new_pages = event_progress_overlay_pages(&progress, Some(&submit), &leaderboard);
    assert_page_parity(&old_pages, &new_pages);

    let (old, new) = measure_pair(
        || old_event_progress_overlay_pages(&progress, Some(&submit), &leaderboard),
        || event_progress_overlay_pages(&progress, Some(&submit), &leaderboard),
    );
    assert_eq!(old.output.checksum, new.output.checksum, "output diverged");
    assert_eq!(old.output.bytes, new.output.bytes, "output size diverged");
    assert_eq!(old.output.pages, new.output.pages, "page count diverged");
    assert!(new.median_ns < old.median_ns, "median latency regressed");
    assert!(new.p95_ns <= old.p95_ns * 1.10, "p95 latency regressed");
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "CPU cycles regressed");
    }
    assert!(
        new.alloc.heap_calls() < old.alloc.heap_calls(),
        "heap calls did not improve"
    );
    assert!(
        new.alloc.allocated_bytes() < old.alloc.allocated_bytes(),
        "allocated bytes did not improve"
    );
    assert!(
        new.alloc.churn() < old.alloc.churn(),
        "memory churn did not improve"
    );

    println!(
        "event-progress page assembly ({} pages, {} output bytes)",
        new.output.pages, new.output.bytes
    );
    print_row("old", &old);
    print_row("new", &new);
    println!(
        "  change: {:+.2}% median  {:+.2}% p95  {:+.2}% cycles  {:+.2}% throughput  \
         {:+.2}% heap calls  {:+.2}% allocated bytes  {:+.2}% churn",
        change(old.median_ns, new.median_ns),
        change(old.p95_ns, new.p95_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(&old), throughput(&new)),
        change(old.alloc.heap_calls() as f64, new.alloc.heap_calls() as f64),
        change(
            old.alloc.allocated_bytes() as f64,
            new.alloc.allocated_bytes() as f64,
        ),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, row: &Measurement) {
    println!(
        "  {label:<3} {:>11.0} ns  {:>11.0} p95  {:>11.0} cycles  {:>8.2} MB/s  \
         {:>3} alloc {:>3} realloc {:>3} free  {:>9} alloc B  {:>9} churn B",
        row.median_ns,
        row.p95_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        throughput(row) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.allocated_bytes(),
        row.alloc.churn(),
    );
}

fn throughput(row: &Measurement) -> f64 {
    row.output.bytes as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
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
