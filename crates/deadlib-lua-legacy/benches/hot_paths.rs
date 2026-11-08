//! CPU and allocation microbenchmarks. Run with `cargo bench --bench hot_paths --features
//! lua54,vendored`. Windows reports scheduled thread cycles; Unix reports thread CPU nanoseconds.
//! Results are medians of 15 warmed samples. Allocation/reallocation calls and requested bytes
//! are counted separately over 100 operations, with counting disabled during CPU measurements.
//! Lua GC is stopped and fixtures are created outside the measured loops.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Instant;

use mlua::{BorrowedBytes, BorrowedStr, Function, Lua, LuaString, Value};

struct CountingAllocator;

static COUNT: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

fn count(size: usize) {
    if COUNT.load(Relaxed) {
        ALLOCATIONS.fetch_add(1, Relaxed);
        BYTES.fetch_add(size, Relaxed);
    }
}

// SAFETY: All allocation operations are forwarded unchanged to System. The counters do not
// allocate.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count(size);
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[cfg(windows)]
fn cpu_counter() -> u64 {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
    }

    let mut cycles = 0;
    // SAFETY: The pseudo-handle refers to this thread and the output is a valid, writable u64.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) },
        0
    );
    cycles
}

#[cfg(unix)]
fn cpu_counter() -> u64 {
    let mut ts = std::mem::MaybeUninit::uninit();
    // SAFETY: clock_gettime initializes the timespec on success, which is checked before reading
    // it.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, ts.as_mut_ptr()) },
        0
    );
    let ts = unsafe { ts.assume_init() };
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

#[cfg(not(any(windows, unix)))]
fn cpu_counter() -> u64 {
    panic!("thread CPU measurements require Windows or Unix");
}

fn measure(name: &str, elements: usize, mut f: impl FnMut()) {
    // Calibrate to at least 100M scheduled cycles (100ms of CPU time on Unix) per sample.
    let mut iterations = 1;
    loop {
        let start = cpu_counter();
        for _ in 0..iterations {
            f();
        }
        if cpu_counter() - start >= 100_000_000 {
            break;
        }
        iterations *= 2;
    }

    let mut cpu = Vec::with_capacity(15);
    let mut throughput = Vec::with_capacity(15);
    for _ in 0..15 {
        let wall_start = Instant::now();
        let start = cpu_counter();
        for _ in 0..iterations {
            f();
        }
        cpu.push((cpu_counter() - start) as f64 / iterations as f64);
        throughput.push((iterations * elements) as f64 / wall_start.elapsed().as_secs_f64());
    }
    cpu.sort_by(f64::total_cmp);
    throughput.sort_by(f64::total_cmp);

    // Count separately so counter updates do not affect the CPU measurements.
    ALLOCATIONS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    COUNT.store(true, Relaxed);
    for _ in 0..100 {
        f();
    }
    COUNT.store(false, Relaxed);

    println!(
        "{name}: cpu/op={:.2} (p10={:.2}, p90={:.2}), elements/s={:.0}, allocs/op={:.2}, bytes/op={:.2}",
        cpu[7],
        cpu[1],
        cpu[13],
        throughput[7],
        ALLOCATIONS.load(Relaxed) as f64 / 100.0,
        BYTES.load(Relaxed) as f64 / 100.0,
    );
}

fn main() -> mlua::Result<()> {
    println!(
        "CPU units: {}",
        if cfg!(windows) {
            "thread cycles"
        } else {
            "thread nanoseconds"
        }
    );
    let filter = std::env::args()
        .skip(1)
        .find(|arg| arg != "--bench")
        .unwrap_or_default();
    let lua = Lua::new();
    lua.gc_stop();

    if "coerce_integer_to_number".contains(&filter) {
        measure("coerce_integer_to_number", 1, || {
            black_box(lua.coerce_number(Value::Integer(black_box(123456789))).unwrap());
        });
    }

    if "borrowed_string".contains(&filter) {
        let identity: Function = lua.load("function(s) return s end").eval()?;
        let s = lua.create_string("a borrowed Lua string")?;
        measure("borrowed_string", 1, || {
            black_box(identity.call::<BorrowedStr>(black_box(&s)).unwrap());
        });
        measure("borrowed_bytes", 1, || {
            black_box(identity.call::<BorrowedBytes>(black_box(&s)).unwrap());
        });
    }

    if "table_pairs".contains(&filter) {
        for size in [0, 1, 64] {
            let table = lua.create_table_from((0..size).map(|i| (format!("key_{i:08}"), i)))?;
            measure(&format!("table_pairs/{size}"), size.max(1), || {
                for pair in table.pairs::<LuaString, i64>() {
                    black_box(pair.unwrap());
                }
            });
        }
    }

    #[cfg(any(feature = "lua51", feature = "lua52", feature = "luajit"))]
    if "integer_conversion".contains(&filter) {
        let identity: Function = lua.load("function(n) return n end").eval()?;
        for number in [42.0, 42.75] {
            measure(&format!("integer_conversion/{number}"), 1, || {
                black_box(identity.call::<i64>(black_box(number)).unwrap());
            });
        }
        let table = lua.create_sequence_from((0..64).map(|i| i as f64 + 0.75))?;
        measure("integer_conversion/sequence64", 64, || {
            black_box(lua.unpack::<Vec<i64>>(Value::Table(table.clone())).unwrap());
        });
    }

    if "owned_string".contains(&filter) {
        let identity: Function = lua.load("function(s) return s end").eval()?;
        let s = lua.create_string("an owned Rust string")?;
        measure("owned_string/from_value", 1, || {
            let value = identity.call::<Value>(black_box(&s)).unwrap();
            black_box(lua.unpack::<String>(value).unwrap());
        });
        measure("owned_string/boxed_result", 1, || {
            black_box(identity.call::<Box<str>>(black_box(&s)).unwrap());
        });
        measure("owned_string/bytes_from_value", 1, || {
            let value = identity.call::<Value>(black_box(&s)).unwrap();
            black_box(lua.unpack::<bstr::BString>(value).unwrap());
        });
    }

    if "table_len".contains(&filter) {
        for metatable in [false, true] {
            let table = lua.create_sequence_from(0..64)?;
            if metatable {
                table.set_metatable(Some(lua.create_table()?))?;
            }
            measure(&format!("table_len/metatable={metatable}"), 1, || {
                black_box(table.len().unwrap());
            });
        }
    }

    Ok(())
}
