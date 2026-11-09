//! Borrowed-register Rust callback workloads and allocation counts.
use deadlib_lua::{Error, HostCall, Value, Vm, compile};
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
// SAFETY: this benchmark forwards allocation contracts unchanged to System.
// Counters neither allocate nor dereference the forwarded pointers.
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
fn add(call: &mut HostCall<'_>) -> Result<(), Error> {
    let result = call.number(0)? + call.number(1)?;
    call.push(Value::Number(result))
}
fn echo(call: &mut HostCall<'_>) -> Result<(), Error> {
    let a = call.arguments()[0];
    let b = call.arguments()[1];
    call.push(a)?;
    call.push(Value::Nil)?;
    call.push(b)?;
    call.push(Value::Boolean(false))
}
fn capture(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Value::Number(scale) = call.captures()[0] else {
        unreachable!()
    };
    let result = call.number(0)? * scale;
    call.push(Value::Number(result))
}
fn reenter(call: &mut HostCall<'_>) -> Result<(), Error> {
    let function = call.arguments()[0];
    let arguments = [call.arguments()[1], call.arguments()[2]];
    call.call_and_return(function, &arguments)
}
fn measure(
    name: &str,
    engine: &str,
    sample: usize,
    repeats: u32,
    calls: u32,
    mut run: impl FnMut(),
) {
    ALLOCS.store(0, Relaxed);
    REALLOCS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    ENABLED.store(true, Relaxed);
    let start = Instant::now();
    for _ in 0..repeats {
        run();
    }
    let ns = start.elapsed().as_nanos() / u128::from(repeats);
    ENABLED.store(false, Relaxed);
    let alloc = ALLOCS.load(Relaxed);
    let realloc = REALLOCS.load(Relaxed);
    let bytes = BYTES.load(Relaxed);
    if engine == "rust" {
        assert_eq!(
            (alloc, realloc, bytes),
            (0, 0, 0),
            "warmed callback path for {name}"
        );
    }
    println!(
        "sample\t{name}\t{engine}\t{sample}\t{repeats}\t{calls}\t{ns}\t{alloc}\t{realloc}\t{bytes}"
    );
}
const ADD: &[u8] = b"local f=host;local s=0;for i=1,5000 do s=s+f(i,3)end;return s";
const REENTER_ADD: &[u8] =
    b"local f=host;local child=child;local s=0;for i=1,5000 do s=s+f(child,i,3)end;return s";
const CHILD_ADD: &[u8] = b"function child(a,b)return a+b end";
const CHILD_ECHO: &[u8] = b"function child(a,b)return a,nil,b,false end";
fn main() {
    if let Ok(engine) = std::env::var("DEADLIB_LUA_HOST_CPU") {
        let reentry =
            std::env::var("DEADLIB_LUA_HOST_CPU_FIXTURE").as_deref() == Ok("reenter_add_lua_loop");
        let source = if reentry { REENTER_ADD } else { ADD };
        match engine.as_str() {
            "rust" => {
                let mut vm = Vm::new();
                let function = vm
                    .new_host_function(if reentry { reenter } else { add }, &[])
                    .unwrap();
                vm.set_global(b"host", function).unwrap();
                if reentry {
                    vm.execute(&compile(CHILD_ADD).unwrap()).unwrap();
                }
                let program = compile(source).unwrap();
                assert_eq!(vm.execute(&program).unwrap(), [Value::Number(12_517_500.)]);
                for _ in 0..400 {
                    black_box(vm.execute(black_box(&program)).unwrap());
                }
            }
            _ => panic!("unknown CPU engine"),
        }
        return;
    }
    println!("meta\tvm_size_bytes\t{}", std::mem::size_of::<Vm>());
    println!("meta\tvalue_size_bytes\t{}", std::mem::size_of::<Value>());
    println!("meta\tcache_build\t{}", env!("DEADLIB_LUA_CACHE_BUILD"));
    for(name,source,callback,expected)in[
        ("add_lua_loop",ADD,add as deadlib_lua::HostFunction,12_517_500.),
        ("echo_lua_loop",b"local f=host;local s=0;for i=1,5000 do local a,b,c,d=f(i,i+1);s=s+a+c;if b~=nil or d~=false then error('arity')end end;return s".as_slice(),echo,25_010_000.),
        ("capture_lua_loop",b"local f=host;local s=0;for i=1,5000 do s=s+f(i)end;return s".as_slice(),capture,37_507_500.),
        ("reenter_add_lua_loop",REENTER_ADD,reenter,12_517_500.),
        ("reenter_echo_lua_loop",b"local f=host;local child=child;local s=0;for i=1,5000 do local a,b,c,d=f(child,i,i+1);s=s+a+c;if b~=nil or d~=false then error('arity')end end;return s".as_slice(),reenter,25_010_000.),
    ]{
        let mut vm=Vm::new();let captures=if name=="capture_lua_loop"{[Value::Number(3.)].as_slice()}else{&[]};
        let host=vm.new_host_function(callback,captures).unwrap();vm.set_global(b"host",host).unwrap();let program=compile(source).unwrap();
        if name.starts_with("reenter_") {
            let child = if name=="reenter_add_lua_loop" {CHILD_ADD} else {CHILD_ECHO};
            vm.execute(&compile(child).unwrap()).unwrap();
        }
        for _ in 0..3{assert_eq!(vm.execute(&program).unwrap(),[Value::Number(expected)]);}
        println!("layout\t{name}\t{}\t{}",program.instruction_count(),program.register_count());
        let h=vm.heap_stats();println!("heap\t{name}\t{}\t{}\t{}",h.live_bytes,h.reserved_bytes,h.native_closures);
        for sample in 0..9{
            let rust=|vm:&mut Vm|{measure(name,"rust",sample,20,5000,||{black_box(vm.execute(black_box(&program)).unwrap());});};
            rust(&mut vm);
        }
    }
    let mut vm = Vm::new();
    let host = vm.new_host_function(add, &[]).unwrap();
    let root = vm.pin(host).unwrap();
    for _ in 0..3 {
        assert_eq!(
            vm.call(host, &[Value::Number(17.), Value::Number(25.)])
                .unwrap(),
            [Value::Number(42.)]
        );
    }
    for sample in 0..9 {
        let rust = |vm: &mut Vm| {
            measure("add_direct", "rust", sample, 20000, 1, || {
                black_box(
                    vm.call(
                        black_box(host),
                        black_box(&[Value::Number(17.), Value::Number(25.)]),
                    )
                    .unwrap(),
                );
            });
        };
        rust(&mut vm);
    }
    vm.unpin(root).unwrap();
}
