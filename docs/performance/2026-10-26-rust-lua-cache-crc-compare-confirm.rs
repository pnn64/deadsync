
use std::{alloc::{GlobalAlloc,Layout,System},hint::black_box,fs,path::{Path,PathBuf},
    sync::atomic::{AtomicBool,AtomicU64,Ordering::Relaxed},time::{Instant,SystemTime,UNIX_EPOCH}};
struct Counted;
#[global_allocator]static ALLOCATOR:Counted=Counted;
static ENABLED:AtomicBool=AtomicBool::new(false);
static ALLOCS:AtomicU64=AtomicU64::new(0);
static REALLOCS:AtomicU64=AtomicU64::new(0);
static BYTES:AtomicU64=AtomicU64::new(0);
// SAFETY: forward allocation contracts unchanged to System. Counters neither
// allocate nor dereference pointers, and count only the selected operations.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self,l:Layout)->*mut u8 {
        // SAFETY: caller supplies the GlobalAlloc layout contract.
        let p=unsafe{System.alloc(l)};
        if !p.is_null()&&ENABLED.load(Relaxed){ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size()as u64,Relaxed);}p
    }
    unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8 {
        // SAFETY: caller supplies the GlobalAlloc layout contract.
        let p=unsafe{System.alloc_zeroed(l)};
        if !p.is_null()&&ENABLED.load(Relaxed){ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size()as u64,Relaxed);}p
    }
    unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8 {
        // SAFETY: caller supplies the live allocation and replacement size.
        let p=unsafe{System.realloc(p,l,n)};
        if !p.is_null()&&ENABLED.load(Relaxed){REALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(n as u64,Relaxed);}p
    }
    unsafe fn dealloc(&self,p:*mut u8,l:Layout) {
        // SAFETY: caller supplies the live System allocation and layout.
        unsafe{System.dealloc(p,l)}
    }
}
fn measure(name:&str,engine:&str,sample:usize,repeats:u32,mut setup:impl FnMut(),mut run:impl FnMut()) {
    ALLOCS.store(0,Relaxed);REALLOCS.store(0,Relaxed);BYTES.store(0,Relaxed);
    let mut elapsed=0;
    for _ in 0..repeats {
        setup();ENABLED.store(true,Relaxed);let start=Instant::now();run();
        elapsed+=start.elapsed().as_nanos();ENABLED.store(false,Relaxed);
    }
    println!("sample\t{name}\t{engine}\t{sample}\t{repeats}\t{}\t{}\t{}\t{}",
        elapsed/u128::from(repeats),ALLOCS.load(Relaxed),REALLOCS.load(Relaxed),BYTES.load(Relaxed));
}
fn checksum_measure(name:&str,engine:&str,sample:usize,repeats:u32,mut run:impl FnMut()) {
    ALLOCS.store(0,Relaxed);REALLOCS.store(0,Relaxed);BYTES.store(0,Relaxed);
    ENABLED.store(true,Relaxed);let start=Instant::now();
    for _ in 0..repeats {run();}
    let ns=start.elapsed().as_nanos() as f64/f64::from(repeats);ENABLED.store(false,Relaxed);
    println!("sample\t{name}\t{engine}\t{sample}\t{repeats}\t{ns}\t{}\t{}\t{}",
        ALLOCS.load(Relaxed),REALLOCS.load(Relaxed),BYTES.load(Relaxed));
}
fn closures()->Vec<u8>{
    let mut s=String::from("local t={};");
    for i in 1..=512{s.push_str(&format!("t[{i}]=function(x)local n={i};return n+x end;"));}
    s.push_str("return t[1](7)+t[512](9)");s.into_bytes()
}
fn native(source:&[u8])->Vec<u8>{
    let p=lua_parent::compile_named(source,b"=crc-benchmark").unwrap();
    let mut vm=lua_parent::Vm::new();let function=vm.load_program(&p);
    let get=lua_parent::compile(b"return string.dump").unwrap();
    let dump=vm.execute(&get).unwrap()[0];
    let lua_parent::Value::String(id)=vm.call(dump,&[lua_parent::Value::Function(function)]).unwrap()[0]else{panic!()};
    vm.string_bytes(id).unwrap().to_vec()
}
fn clear(path:&Path){
    for e in fs::read_dir(path).unwrap(){let e=e.unwrap();assert!(e.file_type().unwrap().is_file());fs::remove_file(e.path()).unwrap();}
    fs::remove_dir(path).unwrap();
}
macro_rules! validate {($lua:ident,$program:expr,$expected:expr,$name:expr,$engine:expr)=>{{
    let mut vm=$lua::Vm::new();let expected:Vec<_>=$expected.iter().map(|&n|$lua::Value::Number(n)).collect();
    assert_eq!(vm.execute($program).unwrap(),expected);
    let h=vm.heap_stats();
    println!("heap\t{}\t{}\t{}\t{}\t{}\t{}\t{}",$name,$engine,h.live_bytes,h.reserved_bytes,h.peak_reserved_bytes,h.collections,h.reclaimed_objects);
}}}
fn main(){
    let base=PathBuf::from(std::env::args_os().nth(1).unwrap());fs::create_dir_all(&base).unwrap();
    let root=base.join(format!("{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    fs::create_dir(&root).unwrap();assert_eq!(root.canonicalize().unwrap().parent(),Some(base.canonicalize().unwrap().as_path()));
    if let Ok(engine)=std::env::var("DEADLIB_LUA_CRC_CPU"){
        let source=native(&closures());
        macro_rules! cpu{($lua:ident)=>{{
            let cache=$lua::ProgramCache::new(&root);
            assert_eq!(cache.compile_named(&source,b"=crc-benchmark").unwrap().status,$lua::CacheStatus::Stored);
            for _ in 0..500{
                let p=cache.compile_named(black_box(&source),b"=crc-benchmark").unwrap();
                assert_eq!(p.status,$lua::CacheStatus::Hit);black_box(p);
            }
        }}}
        match engine.as_str(){"parent"=>cpu!(lua_parent),"current"=>cpu!(lua_current),_=>panic!("CPU engine")};
        clear(&root);return;
    }
    println!("size\tparent\t{}\t{}\t{}",std::mem::size_of::<lua_parent::Vm>(),std::mem::size_of::<lua_parent::Value>(),lua_parent::benchmark_crc_table_bytes());
    println!("size\tcurrent\t{}\t{}\t{}",std::mem::size_of::<lua_current::Vm>(),std::mem::size_of::<lua_current::Value>(),lua_current::benchmark_crc_table_bytes());
    for (len,offset) in [(0,0),(1,0),(3,0),(7,0),(8,0),(9,0),(15,0),(16,0),(31,0),(32,0),(33,0),(512,0),(4096,0),(65536,0),(1048576,0),(65536,3)]{
        let bytes:Vec<_>=(0..len+offset).map(|i|((i*131+i/7)%256)as u8).collect();
        let input=&bytes[offset..];let name=format!("crc_{len}_{offset}");
        assert_eq!(lua_parent::benchmark_checksum(input),lua_current::benchmark_checksum(input));
        let repeats=(8_388_608/len.max(1)).clamp(8,200_000)as u32;
        for sample in 0..9{
            let parent=||{black_box(lua_parent::benchmark_checksum(black_box(input)));};
            let current=||{black_box(lua_current::benchmark_checksum(black_box(input)));};
            if sample%2==0{checksum_measure(&name,"parent",sample,repeats,parent);checksum_measure(&name,"current",sample,repeats,current);}
            else{checksum_measure(&name,"current",sample,repeats,current);checksum_measure(&name,"parent",sample,repeats,parent);}
        }
    }
    let mut records=String::from("local t={");for i in 0..1024{records.push_str(&format!("field{i}={i},"));}records.push_str("};return t.field0+t.field1023");
    let mut binary=String::from("local t={");for i in 0..256{binary.push('\'');for j in 0..128{binary.push_str(&format!("\\{:03}",(i+j)%256));}binary.push_str("',");}binary.push_str("};return #t,#t[256],string.byte(t[1],1),string.byte(t[256],128)");
    let source=closures();let native=native(&source);
    for(name,source,expected,repeats)in[
        ("tiny",b"local x=6;return x*7".to_vec(),vec![42.],20),
        ("records_1024",records.into_bytes(),vec![1023.],5),
        ("closures_512",source,vec![529.],5),
        ("binary_strings_256",binary.into_bytes(),vec![256.,128.,0.,126.],5),
        ("native_closures_512",native,vec![529.],5),
    ]{
        let dir=root.join(name);let parent=lua_parent::ProgramCache::new(&dir);let current=lua_current::ProgramCache::new(&dir);
        let old=parent.compile_named(&source,b"=crc-benchmark").unwrap();assert_eq!(old.status,lua_parent::CacheStatus::Stored);
        let entries:Vec<_>=fs::read_dir(&dir).unwrap().map(|e|e.unwrap().path()).collect();assert_eq!(entries.len(),1);let path=&entries[0];
        let oldbytes=fs::read(path).unwrap();
        assert_eq!(current.compile_named(&source,b"=crc-benchmark").unwrap().status,lua_current::CacheStatus::Hit);
        fs::remove_file(path).unwrap();
        let new=current.compile_named(&source,b"=crc-benchmark").unwrap();assert_eq!(new.status,lua_current::CacheStatus::Stored);
        assert_eq!(parent.compile_named(&source,b"=crc-benchmark").unwrap().status,lua_parent::CacheStatus::Hit);
        assert_eq!(oldbytes,fs::read(path).unwrap());
        assert_eq!(old.program.instruction_count(),new.program.instruction_count());assert_eq!(old.program.register_count(),new.program.register_count());
        println!("layout\t{name}\t{}\t{}\t{}\t{}",old.program.instruction_count(),old.program.register_count(),source.len(),oldbytes.len());
        validate!(lua_parent,&old.program,&expected,name,"parent");validate!(lua_current,&new.program,&expected,name,"current");
        for sample in 0..9{
            let hitname=format!("hit_{name}");let storename=format!("store_{name}");
            let ph=||{let p=parent.compile_named(black_box(&source),b"=crc-benchmark").unwrap();assert_eq!(p.status,lua_parent::CacheStatus::Hit);black_box(p);};
            let ch=||{let p=current.compile_named(black_box(&source),b"=crc-benchmark").unwrap();assert_eq!(p.status,lua_current::CacheStatus::Hit);black_box(p);};
            let ps=||{let p=parent.compile_named(black_box(&source),b"=crc-benchmark").unwrap();assert_eq!(p.status,lua_parent::CacheStatus::Stored);black_box(p);};
            let cs=||{let p=current.compile_named(black_box(&source),b"=crc-benchmark").unwrap();assert_eq!(p.status,lua_current::CacheStatus::Stored);black_box(p);};
            if sample%2==0{
                measure(&hitname,"parent",sample,repeats,||{},ph);measure(&hitname,"current",sample,repeats,||{},ch);
                measure(&storename,"parent",sample,repeats,||{fs::remove_file(path).unwrap();},ps);
                measure(&storename,"current",sample,repeats,||{fs::remove_file(path).unwrap();},cs);
            }else{
                measure(&hitname,"current",sample,repeats,||{},ch);measure(&hitname,"parent",sample,repeats,||{},ph);
                measure(&storename,"current",sample,repeats,||{fs::remove_file(path).unwrap();},cs);
                measure(&storename,"parent",sample,repeats,||{fs::remove_file(path).unwrap();},ps);
            }
        }
        clear(&dir);
    }
    fs::remove_dir(root).unwrap();
}
