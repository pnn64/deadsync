use deadsync_input::{
    GamepadCodeBinding, InputBinding, Keymap, PAD_ID_COUNT_CAP, PadCode, PadDir, PadId,
    PadLookupBench, VirtualAction,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const LOOKUPS: usize = 4_000_000;
const SAMPLE_LOOKUPS: usize = 20_000;

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

// SAFETY: all calls delegate unchanged to `System`; relaxed counters are
// diagnostic and enabled only around single-threaded benchmark operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied a valid layout.
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
        // SAFETY: the arguments are forwarded unchanged to `System`.
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

#[derive(Clone, Copy, Default)]
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
    ns_per_lookup: f64,
    cycles_per_lookup: Option<f64>,
    worst_sample_ns: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(mut lookup: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..10_000 {
        black_box(lookup());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut worst_sample_ns = 0.0f64;
    for _ in 0..LOOKUPS / SAMPLE_LOOKUPS {
        let sample_started = Instant::now();
        for _ in 0..SAMPLE_LOOKUPS {
            checksum = checksum.wrapping_add(black_box(lookup()));
        }
        worst_sample_ns = worst_sample_ns
            .max(sample_started.elapsed().as_secs_f64() * 1.0e9 / SAMPLE_LOOKUPS as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut alloc_checksum = 0u64;
    for _ in 0..LOOKUPS {
        alloc_checksum = alloc_checksum.wrapping_add(black_box(lookup()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(alloc_checksum);

    BenchResult {
        ns_per_lookup: elapsed.as_secs_f64() * 1.0e9 / LOOKUPS as f64,
        cycles_per_lookup: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / LOOKUPS as f64),
        worst_sample_ns,
        allocated,
        checksum,
    }
}

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_eq!(old.allocated.churn(), 0, "{title} old path allocates");
    assert_eq!(new.allocated.churn(), 0, "{title} new path allocates");
    assert_eq!(old.allocated.bytes, 0, "{title} old path allocates bytes");
    assert_eq!(new.allocated.bytes, 0, "{title} new path allocates bytes");
    assert!(
        new.ns_per_lookup < old.ns_per_lookup,
        "{title} did not improve throughput"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.cycles_per_lookup, new.cycles_per_lookup) {
        assert!(new_cycles < old_cycles, "{title} did not reduce cycles");
    }
    println!("\n{title}");
    for (label, result) in [("old", old), ("new", new)] {
        println!(
            "  {label:<3} {:>8.2} ns  {:>8.2} cycles  {:>8.2} worst ns  \
             {:>8.2} Mlookup/s  {:>3} alloc  {:>3} realloc  {:>3} free  {:>6} B",
            result.ns_per_lookup,
            result.cycles_per_lookup.unwrap_or(f64::NAN),
            result.worst_sample_ns,
            1_000.0 / result.ns_per_lookup,
            result.allocated.allocs,
            result.allocated.reallocs,
            result.allocated.frees,
            result.allocated.bytes,
        );
    }
    println!(
        "  improvement {:>6.2}x throughput, {:>6.2}% fewer cycles",
        old.ns_per_lookup / new.ns_per_lookup,
        100.0
            * (1.0
                - new.cycles_per_lookup.unwrap_or(f64::NAN)
                    / old.cycles_per_lookup.unwrap_or(f64::NAN)),
    );
}

fn direction_fixture() -> PadLookupBench {
    let mut keymap = Keymap::default();
    keymap.bind(VirtualAction::p1_up, &[InputBinding::PadDir(PadDir::Up)]);
    let bindings = (0..PAD_ID_COUNT_CAP)
        .map(|device| InputBinding::PadDirOn {
            device,
            dir: PadDir::Up,
        })
        .collect::<Vec<_>>();
    keymap.bind(VirtualAction::p1_down, &bindings);
    PadLookupBench::new(&keymap)
}

fn code_fixture() -> PadLookupBench {
    let mut keymap = Keymap::default();
    let bindings = (1..=24)
        .map(|usage| {
            InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 0x0009_0000 | usage,
                device: None,
                uuid: None,
            })
        })
        .collect::<Vec<_>>();
    keymap.bind(VirtualAction::p1_start, &bindings);
    PadLookupBench::new(&keymap)
}

fn filter_fixture() -> PadLookupBench {
    let mut keymap = Keymap::default();
    let bindings = (0..8)
        .map(|device| {
            InputBinding::GamepadCode(GamepadCodeBinding {
                code_u32: 77,
                device: Some(device),
                uuid: None,
            })
        })
        .collect::<Vec<_>>();
    keymap.bind(VirtualAction::p1_down, &bindings);
    keymap.bind(
        VirtualAction::p1_up,
        &[InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 77,
            device: None,
            uuid: None,
        })],
    );
    keymap.bind(
        VirtualAction::p1_left,
        &[InputBinding::GamepadCode(GamepadCodeBinding {
            code_u32: 77,
            device: None,
            uuid: Some([7; 16]),
        })],
    );
    PadLookupBench::new(&keymap)
}

fn direction_lookup() {
    let fixture = direction_fixture();
    let mut old_index = 0usize;
    let old = measure(|| {
        let id = PadId((old_index % PAD_ID_COUNT_CAP) as u32);
        old_index += 1;
        u64::from(fixture.dir_old(black_box(id), PadDir::Up))
    });
    let mut new_index = 0usize;
    let new = measure(|| {
        let id = PadId((new_index % PAD_ID_COUNT_CAP) as u32);
        new_index += 1;
        u64::from(fixture.dir_new(black_box(id), PadDir::Up))
    });
    print_pair("device-specific direction: hash vs array", &old, &new);
}

fn button_code_lookup() {
    let fixture = code_fixture();
    let mut old_index = 0u32;
    let old = measure(|| {
        let code = PadCode(0x0009_0001 + old_index % 24);
        old_index = old_index.wrapping_add(1);
        u64::from(fixture.code_old(black_box(code)).unwrap_or(u32::MAX))
    });
    let mut new_index = 0u32;
    let new = measure(|| {
        let code = PadCode(0x0009_0001 + new_index % 24);
        new_index = new_index.wrapping_add(1);
        u64::from(fixture.code_new(black_box(code)).unwrap_or(u32::MAX))
    });
    print_pair("raw-button code: binary search vs lookup", &old, &new);
}

fn device_filter_lookup() {
    let fixture = filter_fixture();
    let mut old_index = 0u32;
    let old = measure(|| {
        let id = PadId(old_index % 8);
        old_index = old_index.wrapping_add(1);
        u64::from(fixture.filter_old(0, black_box(id), [0; 16]))
    });
    let mut new_index = 0u32;
    let new = measure(|| {
        let id = PadId(new_index % 8);
        new_index = new_index.wrapping_add(1);
        u64::from(fixture.filter_new(0, black_box(id), [0; 16]))
    });
    print_pair("device-specific button: scan vs mask", &old, &new);
}

fn main() {
    direction_lookup();
    button_code_lookup();
    device_filter_lookup();
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
