
use std::{alloc::{GlobalAlloc,Layout,System},hint::black_box,fs,path::{Path,PathBuf},
    sync::atomic::{AtomicBool,AtomicU64,Ordering::Relaxed},time::{Instant,SystemTime,UNIX_EPOCH}};
struct Counted;
#[global_allocator]static ALLOCATOR:Counted=Counted;
static ENABLED:AtomicBool=AtomicBool::new(false);
static ALLOCS:AtomicU64=AtomicU64::new(0);
static REALLOCS:AtomicU64=AtomicU64::new(0);
static BYTES:AtomicU64=AtomicU64::new(0);
static LIVE:AtomicU64=AtomicU64::new(0);
static PEAK:AtomicU64=AtomicU64::new(0);
static UNBALANCED:AtomicBool=AtomicBool::new(false);
fn live_add(bytes:usize){let live=LIVE.fetch_add(bytes as u64,Relaxed).saturating_add(bytes as u64);PEAK.fetch_max(live,Relaxed);}
fn live_sub(bytes:usize){if LIVE.fetch_sub(bytes as u64,Relaxed)<bytes as u64{UNBALANCED.store(true,Relaxed);LIVE.store(0,Relaxed);}}
// SAFETY: allocation contracts are forwarded unchanged to System. Counters
// neither allocate nor dereference pointers and cover only compilation/drop.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self,l:Layout)->*mut u8 {
        // SAFETY: caller supplies the GlobalAlloc layout contract.
        let p=unsafe{System.alloc(l)};
        if !p.is_null()&&ENABLED.load(Relaxed){ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size()as u64,Relaxed);live_add(l.size());}p
    }
    unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8 {
        // SAFETY: caller supplies the GlobalAlloc layout contract.
        let p=unsafe{System.alloc_zeroed(l)};
        if !p.is_null()&&ENABLED.load(Relaxed){ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size()as u64,Relaxed);live_add(l.size());}p
    }
    unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8 {
        // SAFETY: caller supplies the live allocation and replacement size.
        let p=unsafe{System.realloc(p,l,n)};
        if !p.is_null()&&ENABLED.load(Relaxed){REALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(n as u64,Relaxed);if n>=l.size(){live_add(n-l.size());}else{live_sub(l.size()-n);}}p
    }
    unsafe fn dealloc(&self,p:*mut u8,l:Layout) {
        // SAFETY: caller supplies the live System allocation and layout.
        unsafe{System.dealloc(p,l)}
        if ENABLED.load(Relaxed){live_sub(l.size());}
    }
}
fn measure(name:&str,engine:&str,sample:usize,repeats:u32,mut run:impl FnMut()) {
    ALLOCS.store(0,Relaxed);REALLOCS.store(0,Relaxed);BYTES.store(0,Relaxed);
    LIVE.store(0,Relaxed);PEAK.store(0,Relaxed);UNBALANCED.store(false,Relaxed);
    ENABLED.store(true,Relaxed);let start=Instant::now();
    for _ in 0..repeats{run();}
    let ns=start.elapsed().as_nanos()/u128::from(repeats);ENABLED.store(false,Relaxed);
    assert_eq!(LIVE.load(Relaxed),0);assert!(!UNBALANCED.load(Relaxed));
    println!("sample\t{name}\t{engine}\t{sample}\t{repeats}\t{ns}\t{}\t{}\t{}\t{}",
        ALLOCS.load(Relaxed),REALLOCS.load(Relaxed),BYTES.load(Relaxed),PEAK.load(Relaxed));
}
fn records(n:usize)->Vec<u8>{
    let mut s=String::from("local t={");for i in 0..n{s.push_str(&format!("field{i}={i},"));}
    s.push_str(&format!("}};return t.field0+t.field{}",n-1));s.into_bytes()
}
fn clear(path:&Path){
    for e in fs::read_dir(path).unwrap(){let e=e.unwrap();assert!(e.file_type().unwrap().is_file());fs::remove_file(e.path()).unwrap();}
    fs::remove_dir(path).unwrap();
}
macro_rules! validate {($lua:ident,$program:expr,$expected:expr,$name:expr,$engine:expr)=>{{
    let mut vm=$lua::Vm::new();let values=vm.execute($program).unwrap().to_vec();
    let expected:&[f64]=$expected;
    assert_eq!(values.len(),expected.len());
    for(v,&want)in values.iter().zip(expected){
        let $lua::Value::Number(n)=v else{panic!("numeric result")};assert_eq!(n.to_bits(),want.to_bits());
    }
    let h=vm.heap_stats();
    println!("heap\t{}\t{}\t{}\t{}\t{}\t{}\t{}",$name,$engine,h.live_bytes,h.reserved_bytes,h.peak_reserved_bytes,h.collections,h.reclaimed_objects);
}}}
fn main(){
    let base=PathBuf::from(std::env::args_os().nth(1).unwrap());fs::create_dir_all(&base).unwrap();
    let root=base.join(format!("{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir(&root).unwrap();assert_eq!(root.canonicalize().unwrap().parent(),Some(base.canonicalize().unwrap().as_path()));
    if let Ok(engine)=std::env::var("DEADLIB_LUA_CONSTANT_CPU"){
        let source=records(4096);
        macro_rules! cpu{($lua:ident)=>{{for _ in 0..32{black_box($lua::compile_named(black_box(&source),b"=constant-benchmark").unwrap());}}}}
        match engine.as_str(){"parent"=>cpu!(lua_parent),"current"=>cpu!(lua_current),_=>panic!("CPU engine")};
        fs::remove_dir(root).unwrap();return;
    }
    println!("size\tparent\t{}\t{}\t{}",std::mem::size_of::<lua_parent::Vm>(),std::mem::size_of::<lua_parent::Value>(),lua_parent::benchmark_compiler_function_bytes());
    println!("size\tcurrent\t{}\t{}\t{}",std::mem::size_of::<lua_current::Vm>(),std::mem::size_of::<lua_current::Value>(),lua_current::benchmark_compiler_function_bytes());
    let mut fixtures=Vec::new();
    for n in [8,16,32,64,256,1024,4096]{
        fixtures.push((format!("records_{n}"),records(n),vec![(n-1)as f64],if n>=4096{2}else if n>=1024{5}else{40}));
    }
    for n in [16,64,512,4096]{
        let mut s=String::from("local t={");for i in 1..=n{s.push_str(&format!("{i},"));}
        s.push_str(&format!("}};return #t,t[{n}]"));
        fixtures.push((format!("numbers_{n}"),s.into_bytes(),vec![n as f64,n as f64],if n>=4096{2}else{30}));
    }
    for n in [16,256,4096]{
        let payload="p".repeat(128);let mut s=String::new();for _ in 0..n{s.push_str(&format!("x='{payload}';"));}
        s.push_str("return #x,string.byte(x,1),string.byte(x,128)");
        fixtures.push((format!("repeated_string_{n}"),s.into_bytes(),vec![128.,112.,112.],if n>=4096{2}else{30}));
    }
    let mut s=String::new();for i in 1..=1024{s.push_str(&format!("global_{i}={i};"));}s.push_str("return global_1+global_1024");
    fixtures.push(("globals_1024".into(),s.into_bytes(),vec![1025.],5));
    let mut s=String::from("local base={};function base:add(v)return self.n+v end;local funcs={};");
    for i in 1..=64{s.push_str(&format!("funcs[{i}]=function(x)local t={{"));for j in 1..=40{s.push_str(&format!("k{j}={j},"));}s.push_str("};local o=setmetatable({n=x},{__index=base});return o:add(t.k1+t.k40)end;");}
    s.push_str("local n=0;for i=1,64 do n=n+funcs[i](i)end;return n");
    fixtures.push(("nested_methods_64".into(),s.into_bytes(),vec![4704.],5));
    for(negative,zero)in[(false,"0"),(true,"-0")]{
        let mut s=format!("local first={zero};local t={{zero=0,[0]=17,['0']=19}};");for i in 1..=256{s.push_str(&format!("t.key{i}={i};"));}
        s.push_str("return 1/first,1/t.zero,t[0],t['0']");
        fixtures.push((format!("zero_{}",if negative{"negative"}else{"positive"}),s.into_bytes(),vec![if negative{f64::NEG_INFINITY}else{f64::INFINITY};2].into_iter().chain([17.,19.]).collect(),20));
    }
    let mut s=String::from("local t={[true]=11,[false]=13};local a=nil;");for i in 1..=256{s.push_str(&format!("t.key{i}={i};"));}s.push_str("return a==nil and 17 or 19,t[true],t[false]");
    fixtures.push(("booleans_nil".into(),s.into_bytes(),vec![17.,11.,13.],20));
    assert_eq!(fixtures.len(),19);
    for(name,source,expected,repeats)in fixtures{
        let dir=root.join(&name);let parent=lua_parent::ProgramCache::new(&dir);let current=lua_current::ProgramCache::new(&dir);
        let old=parent.compile_named(&source,b"=constant-benchmark").unwrap();assert_eq!(old.status,lua_parent::CacheStatus::Stored);
        let entries:Vec<_>=fs::read_dir(&dir).unwrap().map(|e|e.unwrap().path()).collect();assert_eq!(entries.len(),1);let path=&entries[0];
        let oldbytes=fs::read(path).unwrap();
        assert_eq!(current.compile_named(&source,b"=constant-benchmark").unwrap().status,lua_current::CacheStatus::Hit);
        fs::remove_file(path).unwrap();
        let new=current.compile_named(&source,b"=constant-benchmark").unwrap();assert_eq!(new.status,lua_current::CacheStatus::Stored);
        assert_eq!(parent.compile_named(&source,b"=constant-benchmark").unwrap().status,lua_parent::CacheStatus::Hit);
        assert_eq!(oldbytes,fs::read(path).unwrap(),"full compiler artifact for {name}");
        assert_eq!(old.program.instruction_count(),new.program.instruction_count());assert_eq!(old.program.register_count(),new.program.register_count());
        println!("layout\t{name}\t{}\t{}\t{}\t{}",old.program.instruction_count(),old.program.register_count(),source.len(),oldbytes.len());
        validate!(lua_parent,&old.program,&expected,&name,"parent");validate!(lua_current,&new.program,&expected,&name,"current");
        for sample in 0..9{
            let parent=||{black_box(lua_parent::compile_named(black_box(&source),b"=constant-benchmark").unwrap());};
            let current=||{black_box(lua_current::compile_named(black_box(&source),b"=constant-benchmark").unwrap());};
            if sample%2==0{measure(&name,"parent",sample,repeats,parent);measure(&name,"current",sample,repeats,current);}
            else{measure(&name,"current",sample,repeats,current);measure(&name,"parent",sample,repeats,parent);}
        }
        clear(&dir);
    }
    fs::remove_dir(root).unwrap();
}
