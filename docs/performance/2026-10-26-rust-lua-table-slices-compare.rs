
use std::alloc::{GlobalAlloc,Layout,System};
use std::sync::atomic::{AtomicBool,AtomicU64,Ordering::Relaxed};
use std::time::Instant;
struct Counted;
#[global_allocator]static ALLOCATOR:Counted=Counted;
static ENABLED:AtomicBool=AtomicBool::new(false);
static ALLOCS:AtomicU64=AtomicU64::new(0);
static REALLOCS:AtomicU64=AtomicU64::new(0);
static BYTES:AtomicU64=AtomicU64::new(0);
// SAFETY: forward the caller's allocation contracts unchanged to System.
// The counters neither allocate nor dereference the forwarded pointers.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self,l:Layout)->*mut u8 {
        // SAFETY: the caller supplies a valid GlobalAlloc layout.
        let p=unsafe{System.alloc(l)};
        if !p.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size() as u64,Relaxed);
        } p
    }
    unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8 {
        // SAFETY: the caller supplies a valid GlobalAlloc layout.
        let p=unsafe{System.alloc_zeroed(l)};
        if !p.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size() as u64,Relaxed);
        } p
    }
    unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8 {
        // SAFETY: the caller supplies the live allocation and replacement size.
        let p=unsafe{System.realloc(p,l,n)};
        if !p.is_null() && ENABLED.load(Relaxed) {
            REALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(n as u64,Relaxed);
        } p
    }
    unsafe fn dealloc(&self,p:*mut u8,l:Layout) {
        // SAFETY: the caller supplies the live System allocation and layout.
        unsafe{System.dealloc(p,l)}
    }
}
macro_rules! fixture {($lua:ident,$name:expr,$engine:expr,$sample:expr,$n:expr,$hash:expr,$refs:expr,$sparse:expr,$mt:expr,$cpu:expr)=>{{
    let step=$lua::compile(b"return collectgarbage('step',0)").unwrap();
    let stop=$lua::compile(b"collectgarbage('stop')").unwrap();
    let mut vm=$lua::Vm::new();
    vm.execute(&step).unwrap();vm.collect_garbage().unwrap();vm.execute(&stop).unwrap();
    let source=format!("local t={{}};for i=1,{} do t[{}]={} end;{}{}bench_table=t;return t",
        $n,if $hash {"-i"}else{"i"},if $refs {"{i}"}else{"i"},
        if $sparse {format!("for i=1,{} do t[{}]=nil end;",$n-1,if $hash {"-i"}else{"i"})}else{String::new()},
        if $mt {"setmetatable(t,{__mode=string.rep('x',1048576)}) ;"}else{""});
    let setup=$lua::compile(source.as_bytes()).unwrap();
    let table=vm.execute(&setup).unwrap()[0];let root=vm.pin(table).unwrap();
    let before=vm.heap_stats();
    // Capacity is outside allocation counting; verify it is never exceeded.
    let mut steps=Vec::with_capacity($n+10000);
    let cycles=if $cpu {32}else{1};
    ALLOCS.store(0,Relaxed);REALLOCS.store(0,Relaxed);BYTES.store(0,Relaxed);
    ENABLED.store(true,Relaxed);
    let started=Instant::now();
    for _ in 0..cycles {
        loop {
            let start=Instant::now();
            let done=vm.execute(&step).unwrap()[0]==$lua::Value::Boolean(true);
            let elapsed=start.elapsed().as_nanos() as u64;
            if !$cpu {assert!(steps.len()<steps.capacity());steps.push(elapsed);}
            if done {break}
        }
    }
    let elapsed=started.elapsed().as_nanos() as u64;
    ENABLED.store(false,Relaxed);
    let after=vm.heap_stats();
    assert_eq!(after.collections-before.collections,cycles);
    assert_eq!(vm.rooted(root).unwrap(),table);
    let check=$lua::compile(format!("local n,s=0,0;for k,v in pairs(bench_table)do local x={};assert(x=={});n=n+1;s=s+x end;return n,s",
        if $refs {"v[1]"}else{"v"},if $hash {"-k"}else{"k"}).as_bytes()).unwrap();
    let expected_n=if $sparse {1.0}else{$n as f64};
    let expected_sum=if $sparse {$n as f64}else{($n as f64)*($n as f64+1.0)/2.0};
    assert_eq!(vm.execute(&check).unwrap(),&[$lua::Value::Number(expected_n),$lua::Value::Number(expected_sum)]);
    if !$cpu {
        steps.sort_unstable();
        println!("sample\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            $name,$engine,$sample,elapsed,steps.len(),steps[steps.len()/2],
            steps[(steps.len()-1)*99/100],steps.last().unwrap(),
            ALLOCS.load(Relaxed),REALLOCS.load(Relaxed),BYTES.load(Relaxed),
            before.reserved_bytes,after.reserved_bytes,after.peak_reserved_bytes,after.live_bytes);
    }
}}}
fn main(){
    if let Ok(engine)=std::env::var("DEADLIB_LUA_GC_CPU"){
        match engine.as_str(){
            "parent"=>fixture!(lua_parent,"cpu","parent",0,100000,false,true,false,false,true),
            "current"=>fixture!(lua_current,"cpu","current",0,100000,false,true,false,false,true),
            _=>panic!("unknown CPU engine")
        }
        return
    }
    macro_rules! size {($lua:ident,$engine:expr)=>{{
        let mut vm=$lua::Vm::new();let before=vm.heap_stats().live_bytes;
        for _ in 0..1024 {vm.new_table();}
        let after=vm.heap_stats().live_bytes;
        assert_eq!((after-before)%1024,0);
        println!("size\t{}\t{}\t{}\t{}",$engine,std::mem::size_of::<$lua::Vm>(),
            std::mem::size_of::<$lua::Value>(),(after-before)/1024);
    }}}
    size!(lua_parent,"parent");size!(lua_current,"current");
    for (name,n,hash,refs,sparse,mt) in [
        ("array_numbers",1048576,false,false,false,false),
        ("array_holes",1048576,false,false,true,false),
        ("hash_numbers",262144,true,false,false,false),
        ("hash_tombstones",262144,true,false,true,false),
        ("array_children",32768,false,true,false,false),
        ("hash_children",32768,true,true,false,false),
        ("array_long_mode",262144,false,false,false,true),
    ] {
        for sample in 0..9 {
            macro_rules! parent {()=>{fixture!(lua_parent,name,"parent",sample,n,hash,refs,sparse,mt,false)}}
            macro_rules! current {()=>{fixture!(lua_current,name,"current",sample,n,hash,refs,sparse,mt,false)}}
            if sample%2==0 {parent!();current!()}else{current!();parent!()}
        }
    }
}
