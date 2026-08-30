use deadsync_theme_simply_love::i18n;
use deadsync_theme_simply_love::screens::components::shared::update_overlay::PanelAppendBenchmark;
use deadsync_theme_simply_love::screens::options::OptionsOverlayHotBenchmark;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 2_001;

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

// SAFETY: every operation delegates unchanged to `System`; relaxed counters
// observe only this benchmark's single-threaded, explicitly gated interval.
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

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

#[derive(Clone, Copy)]
struct Sample {
    ns: u64,
    cycles: Option<u64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(action: impl FnOnce() -> u64) -> Sample {
    let before_alloc = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let before_cycles = thread_cycles();
    let started = Instant::now();
    let checksum = black_box(action());
    let ns = started.elapsed().as_nanos() as u64;
    let after_cycles = thread_cycles();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    Sample {
        ns,
        cycles: cycle_delta(before_cycles, after_cycles),
        alloc: ALLOC.snapshot().delta(before_alloc),
        checksum,
    }
}

fn sample_pair(
    mut old_measure: impl FnMut() -> Sample,
    mut new_measure: impl FnMut() -> Sample,
) -> (Vec<Sample>, Vec<Sample>) {
    for _ in 0..64 {
        black_box(old_measure());
        black_box(new_measure());
    }
    let mut old = Vec::with_capacity(SAMPLES);
    let mut new = Vec::with_capacity(SAMPLES);
    for index in 0..SAMPLES {
        if index % 2 == 0 {
            old.push(old_measure());
            new.push(new_measure());
        } else {
            new.push(new_measure());
            old.push(old_measure());
        }
    }
    (old, new)
}

fn percentile(values: &mut [u64], percentile: usize) -> u64 {
    values.sort_unstable();
    values[(values.len() - 1) * percentile / 100]
}

fn mean(samples: &[Sample], value: impl Fn(&Sample) -> u64) -> f64 {
    samples
        .iter()
        .map(|sample| value(sample) as f64)
        .sum::<f64>()
        / samples.len() as f64
}

fn report(name: &str, items: usize, samples: &[Sample]) {
    let mut p50 = samples.iter().map(|sample| sample.ns).collect::<Vec<_>>();
    let mut p95 = p50.clone();
    let ns_mean = mean(samples, |sample| sample.ns);
    let cycle_values = samples
        .iter()
        .filter_map(|sample| sample.cycles)
        .collect::<Vec<_>>();
    let cycles = (!cycle_values.is_empty())
        .then(|| cycle_values.iter().sum::<u64>() as f64 / cycle_values.len() as f64);
    println!(
        "  {name:<6} p50 {:>8} ns  p95 {:>8} ns  mean {:>9.1} ns  {:>9.1} cycles  {:>8.2} Mitem/s  \
         {:>4.1} alloc  {:>4.1} realloc  {:>4.1} free  {:>9.1} churn B/op",
        percentile(&mut p50, 50),
        percentile(&mut p95, 95),
        ns_mean,
        cycles.unwrap_or(f64::NAN),
        items as f64 * 1_000.0 / ns_mean,
        mean(samples, |sample| sample.alloc.allocs),
        mean(samples, |sample| sample.alloc.reallocs),
        mean(samples, |sample| sample.alloc.frees),
        mean(samples, |sample| sample.alloc.churn_bytes()),
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn report_pair(title: &str, items: usize, old: &[Sample], new: &[Sample]) {
    assert_eq!(old[0].checksum, new[0].checksum, "{title} behavior differs");
    assert!(old.iter().all(|sample| sample.checksum == old[0].checksum));
    assert!(new.iter().all(|sample| sample.checksum == new[0].checksum));
    assert!(new.iter().all(|sample| sample.alloc.allocs == 0));
    assert!(new.iter().all(|sample| sample.alloc.reallocs == 0));
    assert!(new.iter().all(|sample| sample.alloc.frees == 0));
    let mut old_p50 = old.iter().map(|sample| sample.ns).collect::<Vec<_>>();
    let mut new_p50 = new.iter().map(|sample| sample.ns).collect::<Vec<_>>();
    let mut old_p95 = old_p50.clone();
    let mut new_p95 = new_p50.clone();
    let old_cycles = mean(old, |sample| sample.cycles.unwrap_or_default());
    let new_cycles = mean(new, |sample| sample.cycles.unwrap_or_default());
    println!("{title}");
    report("old", items, old);
    report("new", items, new);
    println!(
        "  change: {:+.2}% median  {:+.2}% p95  {:+.2}% cycles  {:+.2}% churn",
        percent_change(
            percentile(&mut old_p50, 50) as f64,
            percentile(&mut new_p50, 50) as f64,
        ),
        percent_change(
            percentile(&mut old_p95, 95) as f64,
            percentile(&mut new_p95, 95) as f64,
        ),
        percent_change(old_cycles, new_cycles),
        percent_change(
            mean(old, |sample| sample.alloc.churn_bytes()),
            mean(new, |sample| sample.alloc.churn_bytes()),
        ),
    );
}

fn main() {
    i18n::init_for_tests();
    let options = OptionsOverlayHotBenchmark::new();

    let (old_prompt, new_prompt) = sample_pair(
        || measure(|| options.legacy_prompt_frame()),
        || measure(|| options.retained_prompt_frame()),
    );
    report_pair(
        "confirmation prompt preparation",
        2,
        &old_prompt,
        &new_prompt,
    );

    let mut old_confirm_actors = Vec::with_capacity(5);
    let mut new_confirm_actors = Vec::with_capacity(5);
    let (old_confirm, new_confirm) = sample_pair(
        || measure(|| options.legacy_confirm_frame(&mut old_confirm_actors)),
        || measure(|| options.direct_confirm_frame(&mut new_confirm_actors)),
    );
    report_pair("confirmation actor staging", 5, &old_confirm, &new_confirm);

    let panel = PanelAppendBenchmark::new();
    let mut old_panel_actors = Vec::with_capacity(8);
    let mut new_panel_actors = Vec::with_capacity(8);
    let (old_panel, new_panel) = sample_pair(
        || measure(|| panel.legacy_frame(&mut old_panel_actors)),
        || measure(|| panel.direct_frame(&mut new_panel_actors)),
    );
    report_pair("updater panel actor staging", 8, &old_panel, &new_panel);
}

#[cfg(windows)]
fn thread_cycles() -> Option<u64> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }

    let mut cycles = 0;
    // SAFETY: the pseudo-handle is valid for this process and `cycles` is
    // writable for the duration of the call.
    let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
    (ok != 0).then_some(cycles)
}

#[cfg(not(windows))]
fn thread_cycles() -> Option<u64> {
    None
}

const fn cycle_delta(before: Option<u64>, after: Option<u64>) -> Option<u64> {
    match (before, after) {
        (Some(before), Some(after)) => Some(after.wrapping_sub(before)),
        _ => None,
    }
}
