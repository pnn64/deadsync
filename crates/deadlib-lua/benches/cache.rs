//! Public compile/store/reload costs with allocation counts and warm OS file cache.
use deadlib_lua::{CacheStatus, Program, ProgramCache, Value, Vm, compile_named};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs,
    hint::black_box,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
struct Counted;
#[global_allocator]
static ALLOCATOR: Counted = Counted;
static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
// SAFETY: pointers/layouts are forwarded unchanged to System. The counters
// neither allocate nor dereference the forwarded pointers.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract.
        let p = unsafe { System.alloc(l) };
        if !p.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(l.size() as u64, Relaxed);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract.
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(l.size() as u64, Relaxed);
        }
        p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        // SAFETY: the caller supplies the live allocation and replacement size.
        let p = unsafe { System.realloc(p, l, n) };
        if !p.is_null() && ENABLED.load(Relaxed) {
            REALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(n as u64, Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: the caller supplies the live System allocation and layout.
        unsafe { System.dealloc(p, l) }
    }
}
fn measure(
    name: &str,
    metric: &str,
    sample: usize,
    repeats: u32,
    mut setup: impl FnMut(),
    mut run: impl FnMut(),
) {
    ALLOCS.store(0, Relaxed);
    REALLOCS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    let mut elapsed = 0;
    for _ in 0..repeats {
        setup();
        ENABLED.store(true, Relaxed);
        let start = Instant::now();
        run();
        elapsed += start.elapsed().as_nanos();
        ENABLED.store(false, Relaxed);
    }
    println!(
        "sample\t{name}\t{metric}\t{sample}\t{repeats}\t{}\t{}\t{}\t{}",
        elapsed / u128::from(repeats),
        ALLOCS.load(Relaxed),
        REALLOCS.load(Relaxed),
        BYTES.load(Relaxed)
    );
}
fn validate(program: &Program, expected: &[Value], name: &str, engine: &str, print: bool) {
    let mut vm = Vm::new();
    assert_eq!(vm.execute(program).unwrap(), expected);
    let before = vm.heap_stats();
    let after = vm.collect_garbage().unwrap();
    if print {
        println!(
            "heap\t{name}\t{engine}\t{}\t{}\t{}\t{}\t{}\t{}",
            before.live_bytes,
            before.reserved_bytes,
            after.live_bytes,
            after.reserved_bytes,
            after.collections,
            after.reclaimed_objects
        );
    }
}
fn closures(n: usize) -> Vec<u8> {
    let mut source = String::from("local t={};");
    for i in 1..=n {
        source.push_str(&format!("t[{i}]=function(x)local n={i};return n+x end;"));
    }
    source.push_str(&format!("return t[1](7)+t[{n}](9)"));
    source.into_bytes()
}
fn root() -> PathBuf {
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/lua-cache-benchmark");
    fs::create_dir_all(&parent).unwrap();
    let path = parent.join(format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&path).unwrap();
    assert_eq!(
        path.canonicalize().unwrap().parent(),
        Some(parent.canonicalize().unwrap().as_path())
    );
    path
}
fn clear(directory: &Path) {
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        fs::remove_file(entry.path()).unwrap();
    }
    fs::remove_dir(directory).unwrap();
}
fn main() {
    let directory = root();
    if let Ok(metric) = std::env::var("DEADLIB_LUA_CACHE_CPU") {
        let source = closures(512);
        let cache = ProgramCache::new(&directory);
        assert_eq!(
            cache
                .compile_named(&source, b"=cache-benchmark")
                .unwrap()
                .status,
            CacheStatus::Stored
        );
        for _ in 0..300 {
            let program = match metric.as_str() {
                "compile" => compile_named(black_box(&source), b"=cache-benchmark").unwrap(),
                "hit" => {
                    let p = cache
                        .compile_named(black_box(&source), b"=cache-benchmark")
                        .unwrap();
                    assert_eq!(p.status, CacheStatus::Hit);
                    p.program
                }
                _ => panic!("unknown CPU metric"),
            };
            black_box(program);
        }
        clear(&directory);
        return;
    }
    let mut records = String::from("local t={");
    for i in 0..1024 {
        records.push_str(&format!("field{i}={i},"));
    }
    records.push_str("};return t.field0+t.field1023");
    let mut binary = String::from("local t={");
    for i in 0..256 {
        binary.push('\'');
        for j in 0..128 {
            binary.push_str(&format!("\\{:03}", (i + j) % 256));
        }
        binary.push_str("',");
    }
    binary.push_str("};return #t,#t[256],string.byte(t[1],1),string.byte(t[256],128)");
    let source = closures(512);
    let lua = lua_reference::Lua::new();
    let native = lua
        .load(source.as_slice())
        .set_name("=cache-native")
        .into_function()
        .unwrap()
        .dump(false);
    let fixtures = [
        (
            "tiny",
            b"local x=6;return x*7".to_vec(),
            vec![Value::Number(42.)],
            300,
        ),
        (
            "records_1024",
            records.into_bytes(),
            vec![Value::Number(1023.)],
            20,
        ),
        ("closures_512", source, vec![Value::Number(529.)], 10),
        (
            "binary_strings_256",
            binary.into_bytes(),
            vec![
                Value::Number(256.),
                Value::Number(128.),
                Value::Number(0.),
                Value::Number(126.),
            ],
            15,
        ),
        ("native_closures_512", native, vec![Value::Number(529.)], 10),
    ];
    println!("meta\tcache_build\t{}", env!("DEADLIB_LUA_CACHE_BUILD"));
    println!("meta\tvm_size_bytes\t{}", std::mem::size_of::<Vm>());
    println!("meta\tvalue_size_bytes\t{}", std::mem::size_of::<Value>());
    for (name, source, expected, repeats) in fixtures {
        let case_dir = directory.join(name);
        let cache = ProgramCache::new(&case_dir);
        let program = compile_named(&source, b"=cache-benchmark").unwrap();
        let first = cache.compile_named(&source, b"=cache-benchmark").unwrap();
        assert_eq!(first.status, CacheStatus::Stored);
        let hit = cache.compile_named(&source, b"=cache-benchmark").unwrap();
        assert_eq!(hit.status, CacheStatus::Hit);
        assert_eq!(program.instruction_count(), hit.program.instruction_count());
        assert_eq!(program.register_count(), hit.program.register_count());
        let entries: Vec<_> = fs::read_dir(&case_dir)
            .unwrap()
            .map(|p| p.unwrap().path())
            .collect();
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        println!(
            "layout\t{name}\t{}\t{}\t{}\t{}",
            program.instruction_count(),
            program.register_count(),
            source.len(),
            fs::metadata(entry).unwrap().len()
        );
        validate(&program, &expected, name, "compile", true);
        validate(&hit.program, &expected, name, "hit", true);
        for sample in 0..9 {
            let compile = || {
                black_box(compile_named(black_box(&source), b"=cache-benchmark").unwrap());
            };
            let hit = || {
                let p = cache
                    .compile_named(black_box(&source), b"=cache-benchmark")
                    .unwrap();
                assert_eq!(p.status, CacheStatus::Hit);
                black_box(p);
            };
            if sample % 2 == 0 {
                measure(name, "compile", sample, repeats, || {}, compile);
                measure(name, "hit", sample, repeats, || {}, hit);
            } else {
                measure(name, "hit", sample, repeats, || {}, hit);
                measure(name, "compile", sample, repeats, || {}, compile);
            }
            measure(
                name,
                "store",
                sample,
                repeats,
                || {
                    fs::remove_file(entry).unwrap();
                },
                || {
                    let p = cache
                        .compile_named(black_box(&source), b"=cache-benchmark")
                        .unwrap();
                    assert_eq!(p.status, CacheStatus::Stored);
                    black_box(p);
                },
            );
        }
        clear(&case_dir);
    }
    fs::remove_dir(&directory).unwrap();
}
