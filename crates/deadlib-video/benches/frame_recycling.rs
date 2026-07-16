use deadlib_video::open_player;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    env,
    hint::black_box,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

const WARMUP_FRAMES: usize = 8;
const MEASURE_FRAMES: usize = 60;
const FRAME_RATE: f32 = 60.0;

#[derive(Clone, Copy)]
enum FrameMode {
    Recycle,
    Discard,
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    allocations: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocations: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocations: self.allocations.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: all operations delegate to `System` with the caller's unchanged
// pointer/layout. The atomics only observe successful allocations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes directly from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocations.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` and `layout` come directly from the allocator caller.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: `ptr` and `old` come directly from the allocator caller.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.allocations.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocations: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocations: self.allocations - before.allocations,
            bytes: self.bytes - before.bytes,
        }
    }
}

fn take_frame(player: &mut deadlib_video::Player, frame: usize, mode: FrameMode) {
    let target = (frame as f32 + 0.5) / FRAME_RATE;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(frame) = player.take_due_frame(target) {
            match mode {
                FrameMode::Recycle => {
                    black_box(frame);
                }
                FrameMode::Discard => {
                    let (image, recycle_tx) = frame.into_upload_parts();
                    black_box(image);
                    drop(recycle_tx);
                }
            }
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for video frame"
        );
        thread::sleep(Duration::from_micros(100));
    }
}

fn main() {
    let path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: frame_recycling <60-fps video path> [discard]");
    let mode = if env::args().nth(2).as_deref() == Some("discard") {
        FrameMode::Discard
    } else {
        FrameMode::Recycle
    };
    let mut player = open_player(&path, false).expect("open benchmark video");

    for frame in 0..WARMUP_FRAMES {
        take_frame(&mut player, frame, mode);
    }

    let before = ALLOC.snapshot();
    let started = Instant::now();
    for frame in WARMUP_FRAMES..WARMUP_FRAMES + MEASURE_FRAMES {
        take_frame(&mut player, frame, mode);
    }
    let elapsed = started.elapsed();
    let allocated = ALLOC.snapshot().delta(before);
    let pool_misses = player.buffer_pool_misses();
    let micros_per_frame = elapsed.as_secs_f64() * 1_000_000.0 / MEASURE_FRAMES as f64;
    let allocs_per_frame = allocated.allocations as f64 / MEASURE_FRAMES as f64;
    let mib_per_frame = allocated.bytes as f64 / MEASURE_FRAMES as f64 / (1024.0 * 1024.0);

    let mode_name = match mode {
        FrameMode::Recycle => "recycle on completion",
        FrameMode::Discard => "discard after completion",
    };
    println!("displayed video frame recycling microbenchmark ({mode_name})");
    println!("{} measured frames from {}", MEASURE_FRAMES, path.display());
    println!(
        "{micros_per_frame:.1} us/frame  {allocs_per_frame:.2} allocs/frame  {mib_per_frame:.3} MiB/frame"
    );
    println!("decoder buffer-pool misses: {pool_misses}");
}
