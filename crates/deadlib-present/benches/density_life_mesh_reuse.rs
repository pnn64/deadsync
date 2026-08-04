use deadlib_present::density::{update_density_life_mesh, update_density_life_mesh_reusable};
use deadlib_render::MeshVertex;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PLAYERS: usize = 2;
const MIN_POINTS: usize = 900;
const MAX_POINTS: usize = 961;
const WARMUP_REFRESHES: usize = 128;
const MEASURE_REFRESHES: usize = 2_000;
const GRAPH_WIDTH: f32 = 512.0;
const EDGE_FEATHER: f32 = 0.266_75;

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

// SAFETY: every operation delegates to `System` unchanged; the atomics only
// observe successful allocation activity.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied this layout.
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

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn fixture_points() -> Vec<[f32; 2]> {
    (0..MAX_POINTS)
        .map(|index| {
            let x = index as f32 * GRAPH_WIDTH / (MAX_POINTS - 1) as f32;
            let y = match index % 4 {
                0 => 8.0,
                1 => 92.0,
                2 => 24.0,
                _ => 76.0,
            };
            [x, y]
        })
        .collect()
}

fn point_count(refresh: usize) -> usize {
    MIN_POINTS + refresh % (MAX_POINTS - MIN_POINTS + 1)
}

fn mesh_checksum(vertices: &[MeshVertex]) -> u64 {
    let Some(first) = vertices.first() else {
        return 0;
    };
    let Some(last) = vertices.last() else {
        return 0;
    };
    (vertices.len() as u64).rotate_left(7)
        ^ u64::from(first.pos[0].to_bits()).rotate_left(19)
        ^ u64::from(last.pos[1].to_bits()).rotate_left(41)
}

fn measure_shared(points: &[[f32; 2]]) -> BenchResult {
    let mut meshes: [Option<Arc<[MeshVertex]>>; PLAYERS] = std::array::from_fn(|_| None);
    for refresh in 0..WARMUP_REFRESHES {
        for mesh in &mut meshes {
            update_density_life_mesh(
                mesh,
                &points[..point_count(refresh)],
                0.0,
                GRAPH_WIDTH,
                2.0,
                EDGE_FEATHER,
                [1.0; 4],
            );
        }
    }

    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for refresh in 0..MEASURE_REFRESHES {
        for mesh in &mut meshes {
            update_density_life_mesh(
                mesh,
                black_box(&points[..point_count(refresh)]),
                0.0,
                GRAPH_WIDTH,
                2.0,
                EDGE_FEATHER,
                [1.0; 4],
            );
            checksum = checksum.rotate_left(11)
                ^ mesh_checksum(black_box(mesh.as_deref().unwrap_or_default()));
        }
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn measure_reusable(points: &[[f32; 2]]) -> BenchResult {
    let mut meshes: [Option<Arc<Vec<MeshVertex>>>; PLAYERS] = std::array::from_fn(|_| None);
    for refresh in 0..WARMUP_REFRESHES {
        for mesh in &mut meshes {
            update_density_life_mesh_reusable(
                mesh,
                &points[..point_count(refresh)],
                0.0,
                GRAPH_WIDTH,
                2.0,
                EDGE_FEATHER,
                [1.0; 4],
            );
        }
    }

    let before = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0u64;
    for refresh in 0..MEASURE_REFRESHES {
        for mesh in &mut meshes {
            update_density_life_mesh_reusable(
                mesh,
                black_box(&points[..point_count(refresh)]),
                0.0,
                GRAPH_WIDTH,
                2.0,
                EDGE_FEATHER,
                [1.0; 4],
            );
            checksum = checksum.rotate_left(11)
                ^ mesh_checksum(black_box(mesh.as_deref().map_or(&[], Vec::as_slice)));
        }
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let refreshes = MEASURE_REFRESHES as f64;
    println!(
        "{label:<13} {:>9.2} us/refresh  {:>10.0} cycles/refresh  \
         {:>5.2} allocs/refresh  {:>8.1} KiB/refresh  {:>5.2} reallocs/refresh",
        result.elapsed.as_secs_f64() * 1_000_000.0 / refreshes,
        result.cycles as f64 / refreshes,
        result.allocated.allocs as f64 / refreshes,
        result.allocated.bytes as f64 / refreshes / 1024.0,
        result.allocated.reallocs as f64 / refreshes,
    );
}

fn main() {
    let points = fixture_points();
    let shared = measure_shared(&points);
    let reusable = measure_reusable(&points);
    assert_eq!(shared.checksum, reusable.checksum);
    black_box((shared.checksum, reusable.checksum));

    println!(
        "gameplay density-life mesh refresh benchmark \
         ({PLAYERS} players, {MIN_POINTS}..={MAX_POINTS} visible points)"
    );
    print_result("shared slice", &shared);
    print_result("reusable vec", &reusable);
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
