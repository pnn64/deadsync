//! Opt-in allocation accounting for the standalone song Lua tools.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::Instant;
use std::io::{Read, Write};

struct CountedSystem;
#[global_allocator]
static ALLOCATOR: CountedSystem = CountedSystem;
static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: pointers, layouts and results pass unchanged to System. Accounting
// uses only allocation-free atomic operations, including on worker threads.
unsafe impl GlobalAlloc for CountedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size() as u64, Relaxed);
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size() as u64, Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies this live System allocation and layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: the caller supplies the live pointer, layout and new size.
        let ptr = unsafe { System.realloc(ptr, layout, size) };
        if !ptr.is_null() && ENABLED.load(Relaxed) {
            REALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(size as u64, Relaxed);
        }
        ptr
    }
}

struct PerfControl {
    control: std::fs::File,
    ack: std::fs::File,
}

impl PerfControl {
    fn open() -> Option<Self> {
        let path = std::env::var_os("DEADSYNC_PERF_CONTROL")?;
        let ack = std::env::var_os("DEADSYNC_PERF_ACK").expect("perf acknowledgement FIFO");
        Some(Self {
            control: std::fs::OpenOptions::new().write(true).open(path).expect("perf control FIFO"),
            ack: std::fs::File::open(ack).expect("perf acknowledgement FIFO"),
        })
    }
    fn command(&mut self, command: &[u8]) {
        self.control.write_all(command).expect("control perf counters");
        let mut ack = [0; 4];
        let mut filled = 0;
        while filled < ack.len() {
            let mut byte = [0];
            self.ack.read_exact(&mut byte).expect("perf counter acknowledgement");
            // Some perf releases include a trailing NUL after "ack\n".
            if filled == 0 && byte[0] == 0 { continue; }
            ack[filled] = byte[0];
            filled += 1;
        }
        assert_eq!(&ack, b"ack\n");
    }
}

pub struct Measurement {
    started: Instant,
    control: Option<PerfControl>,
}
impl Measurement {
    pub fn start() -> Self {
        let mut control = PerfControl::open();
        if let Some(control) = &mut control { control.command(b"enable\n"); }
        assert!(!ENABLED.swap(true, Relaxed), "allocation measurements cannot nest");
        ALLOCS.store(0, Relaxed);
        REALLOCS.store(0, Relaxed);
        BYTES.store(0, Relaxed);
        Self { started: Instant::now(), control }
    }
    pub fn finish(mut self) -> serde_json::Value {
        let elapsed = self.started.elapsed();
        ENABLED.store(false, Relaxed);
        if let Some(control) = &mut self.control { control.command(b"disable\n"); }
        serde_json::json!({"wall_seconds":elapsed.as_secs_f64(),
            "allocations":ALLOCS.load(Relaxed),"reallocations":REALLOCS.load(Relaxed),
            "allocated_bytes":BYTES.load(Relaxed),
            "allocation_scope":"current process, all threads; Rust GlobalAlloc including mlua; native malloc excluded"})
    }
}
