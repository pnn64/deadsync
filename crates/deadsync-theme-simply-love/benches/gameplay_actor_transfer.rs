use deadlib_present::actors::Actor;
use deadsync_theme_simply_love::screens::gameplay::{
    benchmark_append_player_actors, benchmark_append_player_actors_legacy,
};
use glam::{Mat4, Vec3};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const ACTORS: usize = 256;
const WARMUP_TRANSFERS: usize = 2_000;
const MEASURE_TRANSFERS: usize = 100_000;

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

// SAFETY: every operation delegates to `System`; counters only observe
// successful calls and do not affect allocation ownership.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: this pointer/layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: all arguments are forwarded unchanged to `System`.
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

fn transfer(a: &mut Vec<Actor>, b: &mut Vec<Actor>, append: fn(&mut Vec<Actor>, &mut Vec<Actor>)) {
    if a.is_empty() {
        append(a, b);
    } else {
        append(b, a);
    }
}

struct BenchResult {
    ns_per_transfer: f64,
    allocated: AllocSnapshot,
    checksum: f32,
}

fn measure(append: fn(&mut Vec<Actor>, &mut Vec<Actor>)) -> BenchResult {
    let mut a: Vec<_> = (0..ACTORS)
        .map(|index| Actor::CameraPush {
            view_proj: Mat4::from_translation(Vec3::new(index as f32, 0.0, 0.0)),
        })
        .collect();
    let mut b = Vec::with_capacity(ACTORS);
    for _ in 0..WARMUP_TRANSFERS {
        transfer(&mut a, &mut b, append);
    }

    let before = ALLOC.snapshot();
    let started = Instant::now();
    for _ in 0..MEASURE_TRANSFERS {
        black_box(transfer(&mut a, &mut b, append));
    }
    let elapsed = started.elapsed();
    let allocated = ALLOC.snapshot().delta(before);
    let actors = if a.is_empty() { &b } else { &a };
    let checksum = actors.iter().fold(0.0f32, |sum, actor| match actor {
        Actor::CameraPush { view_proj } => sum + view_proj.w_axis.x,
        _ => sum,
    });
    assert_eq!(actors.len(), ACTORS);
    BenchResult {
        ns_per_transfer: elapsed.as_secs_f64() * 1_000_000_000.0 / MEASURE_TRANSFERS as f64,
        allocated,
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let transfers = MEASURE_TRANSFERS as f64;
    println!(
        "{label:<13} {:>9.2} ns/transfer  {:>5.2} allocs/transfer  \
         {:>7.1} bytes/transfer  {:>5.2} reallocs/transfer",
        result.ns_per_transfer,
        result.allocated.allocs as f64 / transfers,
        result.allocated.bytes as f64 / transfers,
        result.allocated.reallocs as f64 / transfers,
    );
}

fn main() {
    let legacy = measure(benchmark_append_player_actors_legacy);
    let bulk = measure(benchmark_append_player_actors);
    assert_eq!(legacy.checksum, bulk.checksum);
    black_box((legacy.checksum, bulk.checksum));

    println!("gameplay Actor transfer benchmark ({ACTORS} actors)");
    print_result("extend drain", &legacy);
    print_result("bulk append", &bulk);
}
