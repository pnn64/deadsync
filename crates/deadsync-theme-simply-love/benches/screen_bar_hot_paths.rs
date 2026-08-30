use deadlib_present::actors::{Actor, actor_tree_stats};
use deadsync_theme_simply_love::screens::components::shared::screen_bar::{
    AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement, build, build_cached,
    build_no_background, build_no_background_cached, build_title_menu, build_title_menu_cached,
};
use deadsync_theme_simply_love::views::SimplyLoveVisualPolicyView;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ITERATIONS: usize = 500_000;
const SAMPLE_OPS: usize = 500;
const SAMPLE_COUNT: usize = ITERATIONS / SAMPLE_OPS;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: all allocation operations are forwarded unchanged to `System`;
// relaxed counters only observe successful calls while measurement is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
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
            self.frees.fetch_add(1, Ordering::Relaxed);
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
    frees: u64,
    bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            bytes: self.bytes - before.bytes,
        }
    }

    const fn churn(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }
}

struct BenchResult {
    ns_per_frame: f64,
    p95_sample_ns: f64,
    cycles_per_frame: Option<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
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

fn actor_checksum(actor: Actor) -> u64 {
    let stats = actor_tree_stats(std::slice::from_ref(&actor));
    u64::from(stats.total)
        ^ u64::from(stats.sprites).rotate_left(7)
        ^ u64::from(stats.texts).rotate_left(13)
        ^ u64::from(stats.frames).rotate_left(19)
        ^ u64::from(stats.text_chars).rotate_left(29)
}

fn measure(mut frame: impl FnMut() -> Actor) -> BenchResult {
    for _ in 0..2_000 {
        black_box(actor_checksum(frame()));
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0_u64;
    let mut sample_ns = [0.0_f64; SAMPLE_COUNT];
    for sample_ns in &mut sample_ns {
        let sample_started = Instant::now();
        for _ in 0..SAMPLE_OPS {
            checksum = checksum.wrapping_add(black_box(actor_checksum(frame())));
        }
        *sample_ns = sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / SAMPLE_OPS as f64;
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    sample_ns.sort_unstable_by(f64::total_cmp);
    let p95_sample_ns = sample_ns[SAMPLE_COUNT * 95 / 100];

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0_u64;
    for _ in 0..ITERATIONS {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(actor_checksum(frame())));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    BenchResult {
        ns_per_frame: elapsed.as_secs_f64() * 1_000_000_000.0 / ITERATIONS as f64,
        p95_sample_ns,
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / ITERATIONS as f64),
        allocated,
        checksum,
    }
}

fn report_pair(name: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{name} behavior diverged");
    assert!(
        old.allocated.allocs > 0,
        "{name} legacy path did not allocate"
    );
    assert_eq!(new.allocated.allocs, 0, "{name} cached path allocated");
    assert_eq!(new.allocated.reallocs, 0, "{name} cached path reallocated");
    assert_eq!(new.allocated.frees, 0, "{name} cached path freed");
    assert_eq!(new.allocated.bytes, 0, "{name} cached path allocated bytes");

    println!("\n{name}");
    print_result("old", old);
    print_result("new", new);
    println!(
        "  improvement {:>7.2}x throughput  {:>7.1}% cycles  {:>7.1}% churn  {:>7.1}% bytes",
        old.ns_per_frame / new.ns_per_frame,
        percent_reduction(
            old.cycles_per_frame.unwrap_or(f64::NAN),
            new.cycles_per_frame.unwrap_or(f64::NAN),
        ),
        percent_reduction(old.allocated.churn() as f64, new.allocated.churn() as f64),
        percent_reduction(old.allocated.bytes as f64, new.allocated.bytes as f64),
    );
}

fn print_result(label: &str, result: &BenchResult) {
    let frames = ITERATIONS as f64;
    println!(
        "  {label:<3} {:>9.2} ns/frame  {:>9.2} cycles/frame  {:>8.3} Mframe/s  \
         {:>9.2} p95 ns  {:>5.2} alloc  {:>5.2} realloc  {:>5.2} free  \
         {:>5.2} churn  {:>9.1} B/frame",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        1_000.0 / result.ns_per_frame,
        result.p95_sample_ns,
        result.allocated.allocs as f64 / frames,
        result.allocated.reallocs as f64 / frames,
        result.allocated.frees as f64 / frames,
        result.allocated.churn() as f64 / frames,
        result.allocated.bytes as f64 / frames,
    );
}

fn percent_reduction(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        0.0
    } else {
        100.0 * (1.0 - new / old)
    }
}

fn normal_params() -> ScreenBarParams<'static> {
    ScreenBarParams {
        title: "Event Mode",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        left_text: Some("Player One"),
        center_text: None,
        right_text: Some("Player Two"),
        left_avatar: Some(AvatarParams {
            texture_key: "avatar-p1-normal",
        }),
        right_avatar: Some(AvatarParams {
            texture_key: "avatar-p2-normal",
        }),
        fg_color: [1.0; 4],
        visual_policy: SimplyLoveVisualPolicyView::default(),
    }
}

fn no_background_params() -> ScreenBarParams<'static> {
    ScreenBarParams {
        title: "",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: true,
        left_text: Some("Player One"),
        center_text: None,
        right_text: Some("Player Two"),
        left_avatar: Some(AvatarParams {
            texture_key: "avatar-p1",
        }),
        right_avatar: Some(AvatarParams {
            texture_key: "avatar-p2",
        }),
        fg_color: [1.0; 4],
        visual_policy: SimplyLoveVisualPolicyView::default(),
    }
}

fn title_menu_params() -> ScreenBarParams<'static> {
    ScreenBarParams {
        title: "1 Credit",
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: true,
        left_text: Some("PRESS START"),
        center_text: None,
        right_text: Some("PRESS START"),
        left_avatar: None,
        right_avatar: None,
        fg_color: [1.0; 4],
        visual_policy: SimplyLoveVisualPolicyView::default(),
    }
}

fn main() {
    let normal = normal_params();
    black_box(build_cached(normal));
    let old = measure(|| build(normal));
    let new = measure(|| build_cached(normal));
    report_pair("1. retained normal screen bar", &old, &new);

    let no_background = no_background_params();
    black_box(build_no_background_cached(no_background));
    let old = measure(|| build_no_background(no_background));
    let new = measure(|| build_no_background_cached(no_background));
    report_pair("2. retained background-free screen bar", &old, &new);

    let title_menu = title_menu_params();
    black_box(build_title_menu_cached(title_menu));
    let old = measure(|| build_title_menu(title_menu));
    let new = measure(|| build_title_menu_cached(title_menu));
    report_pair("3. retained title-menu screen bar", &old, &new);
}
