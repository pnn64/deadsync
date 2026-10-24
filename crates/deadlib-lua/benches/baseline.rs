//! Initial correctness-checked, allocation-instrumented reference measurements.
use deadlib_lua::{Value, Vm, compile};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    time::Instant,
};

struct Counted;
#[global_allocator]
static ALLOCATOR: Counted = Counted;
static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);

// SAFETY: this benchmark-only allocator forwards pointers/layouts unchanged to
// System. Instrumentation uses allocation-free atomics and never dereferences pointers.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract.
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size() as u64, Relaxed);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract.
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size() as u64, Relaxed);
        }
        p
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: the caller supplies the live pointer, layout and new size.
        let p = unsafe { System.realloc(p, layout, size) };
        if !p.is_null() && ENABLED.load(Relaxed) {
            REALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(size as u64, Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies this live System allocation and its layout.
        unsafe { System.dealloc(p, layout) }
    }
}

fn measure(
    mut work: impl FnMut(),
    repeats: u32,
    metric: &str,
    engine: &str,
    name: &str,
    sample: usize,
) {
    ALLOCS.store(0, Relaxed);
    REALLOCS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    ENABLED.store(true, Relaxed);
    let start = Instant::now();
    for _ in 0..repeats {
        work();
    }
    let ns = start.elapsed().as_nanos() / u128::from(repeats);
    ENABLED.store(false, Relaxed);
    println!(
        "{metric}\t{engine}\t{name}\t{sample}\t{repeats}\t{ns}\t{}\t{}\t{}",
        ALLOCS.load(Relaxed),
        REALLOCS.load(Relaxed),
        BYTES.load(Relaxed)
    );
}

fn main() {
    println!(
        "metric\tengine\tworkload\tsample\trepeats\tns_per_iteration\trust_allocations_total\trust_reallocations_total\trust_bytes_total"
    );
    measure(
        || {
            black_box(Vm::new());
        },
        1000,
        "startup_drop",
        "rust",
        "state",
        0,
    );
    measure(
        || {
            black_box(lua_reference::Lua::new());
        },
        1000,
        "startup_drop",
        "lua51",
        "state",
        0,
    );
    for (name, source, expected, repeats) in [
        (
            "numeric_for",
            "local s=0; for i=1,100000 do s=s+i end; return s",
            5000050000.0,
            100,
        ),
        (
            "numeric_while",
            "local i,s=0,0; while i<100000 do i=i+1; s=s+i end; return s",
            5000050000.0,
            100,
        ),
        (
            "array_table",
            "local t={}; for i=1,10000 do t[i]=i end; local s=0; for i=1,10000 do s=s+t[i] end; return s",
            50005000.0,
            50,
        ),
        (
            "recursive_fib",
            "local function f(n) if n<2 then return n end; return f(n-1)+f(n-2) end; return f(20)",
            6765.0,
            30,
        ),
        (
            "closure_calls",
            "local x=0; local function f(n) x=x+n end; for i=1,10000 do f(i) end; return x",
            50005000.0,
            50,
        ),
        (
            "byte_concat",
            "local s=''; for i=1,1000 do s=s..'x' end; return #s",
            1000.0,
            30,
        ),
        (
            "pattern_find",
            "local s=0;for i=1,1000 do local a,b=string.find('prefix abc123 xyz','(%a+)(%d+)');s=s+a+b end;return s",
            21000.0,
            30,
        ),
        (
            "pattern_replace",
            "local s=0;for i=1,1000 do local r,n=string.gsub('abc 123 def 456','(%a+)','[%1]');s=s+#r+n end;return s",
            21000.0,
            30,
        ),
        (
            "pattern_callback",
            "local s=0;for i=1,1000 do local r,n=string.gsub('abc 123 def 456','%a+',string.upper);s=s+#r+n end;return s",
            17000.0,
            30,
        ),
        (
            "string_format",
            "local s=0;for i=1,1000 do s=s+#string.format('%08d:%.3f',i,i/1000) end;return s",
            14000.0,
            30,
        ),
        (
            "table_sort",
            "local s=0;for k=1,100 do local t={};for i=1,128 do t[i]=129-i end;table.sort(t);s=s+t[1]+t[128] end;return s",
            12900.0,
            20,
        ),
        (
            "table_sort_callback",
            "local s=0;local function less(a,b)return a<b end;for k=1,100 do local t={};for i=1,128 do t[i]=129-i end;table.sort(t,less);s=s+t[1]+t[128] end;return s",
            12900.0,
            20,
        ),
        (
            "coroutine_resume",
            "local co=coroutine.create(function()for i=1,1000 do coroutine.yield(i) end;return 0 end);local s=0;for i=1,1001 do local ok,n=coroutine.resume(co);assert(ok);s=s+n end;return s",
            500500.0,
            30,
        ),
        (
            "coroutine_wrap",
            "local f=coroutine.wrap(function()for i=1,1000 do coroutine.yield(i) end;return 0 end);local s=0;for i=1,1001 do s=s+f() end;return s",
            500500.0,
            30,
        ),
        (
            "coroutine_deep_yield",
            "local function f(n)if n==0 then for i=1,1000 do coroutine.yield(i) end;return 0 end;return f(n-1)+0 end;local c=coroutine.create(function()return f(128)end);local s=0;for i=1,1001 do local ok,n=coroutine.resume(c);assert(ok);s=s+n end;return s",
            500500.0,
            30,
        ),
        (
            "coroutine_reuse",
            "if not cachedco then cachedco=coroutine.create(function()while true do coroutine.yield(1) end end) end;local s=0;for i=1,1000 do local ok,n=coroutine.resume(cachedco);assert(ok);s=s+n end;return s",
            1000.0,
            30,
        ),
    ] {
        let source = source.as_bytes();
        let program = compile(source).unwrap();
        let mut vm = Vm::new();
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Number(expected)]);
        let reference = lua_reference::Lua::new();
        let function = reference.load(source).into_function().unwrap();
        assert_eq!(function.call::<f64>(()).unwrap(), expected);
        measure(
            || {
                black_box(compile(black_box(source)).unwrap());
            },
            1000,
            "compile_drop",
            "rust",
            name,
            0,
        );
        measure(
            || {
                black_box(reference.load(black_box(source)).into_function().unwrap());
            },
            1000,
            "compile_drop",
            "lua51",
            name,
            0,
        );
        for sample in 0..5 {
            if sample % 2 == 0 {
                measure(
                    || {
                        black_box(vm.execute(black_box(&program)).unwrap());
                    },
                    repeats,
                    "execute",
                    "rust",
                    name,
                    sample,
                );
                measure(
                    || {
                        black_box(function.call::<f64>(()).unwrap());
                    },
                    repeats,
                    "execute",
                    "lua51",
                    name,
                    sample,
                );
            } else {
                measure(
                    || {
                        black_box(function.call::<f64>(()).unwrap());
                    },
                    repeats,
                    "execute",
                    "lua51",
                    name,
                    sample,
                );
                measure(
                    || {
                        black_box(vm.execute(black_box(&program)).unwrap());
                    },
                    repeats,
                    "execute",
                    "rust",
                    name,
                    sample,
                );
            }
        }
        println!(
            "reference_live_heap_bytes\t{name}\t{}",
            reference.used_memory()
        );
    }
    println!("value_size_bytes\t{}", std::mem::size_of::<Value>());
    println!(
        "allocation_scope\tRust GlobalAlloc only; native C malloc excluded; totals cover all iterations in each row"
    );
    println!(
        "limitations\tNo Rust GC yet; startup opens different libraries; execution includes host entry/return; compile includes drop; instrumented timings are preliminary"
    );
}
