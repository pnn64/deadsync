
use std::{alloc::{GlobalAlloc,Layout,System},hint::black_box,
    sync::atomic::{AtomicBool,AtomicU64,Ordering::Relaxed},time::Instant};
struct Counted;
#[global_allocator] static ALLOCATOR:Counted=Counted;
static ENABLED:AtomicBool=AtomicBool::new(false);
static ALLOCS:AtomicU64=AtomicU64::new(0);
static REALLOCS:AtomicU64=AtomicU64::new(0);
static BYTES:AtomicU64=AtomicU64::new(0);
// SAFETY: forward all pointers and layouts unchanged to System. Counters never
// allocate or dereference pointers and are enabled only around execution.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self,l:Layout)->*mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract.
        let p=unsafe{System.alloc(l)};
        if !p.is_null() && ENABLED.load(Relaxed) {
            ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size() as u64,Relaxed);
        } p
    }
    unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8 {
        // SAFETY: the caller supplies the GlobalAlloc layout contract.
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
        // SAFETY: the caller supplies this live System allocation and layout.
        unsafe{System.dealloc(p,l)}
    }
}
fn measure(name:&str,engine:&str,sample:usize,repeats:u32,mut run:impl FnMut()) {
    ALLOCS.store(0,Relaxed);REALLOCS.store(0,Relaxed);BYTES.store(0,Relaxed);
    ENABLED.store(true,Relaxed);let start=Instant::now();
    for _ in 0..repeats {run()}
    let ns=start.elapsed().as_nanos()/u128::from(repeats);
    ENABLED.store(false,Relaxed);
    println!("sample\t{name}\t{engine}\t{sample}\t{repeats}\t{ns}\t{}\t{}\t{}",
        ALLOCS.load(Relaxed),REALLOCS.load(Relaxed),BYTES.load(Relaxed));
}
fn main() {
    if let Ok(engine)=std::env::var("DEADLIB_LUA_INTERN_CPU") {
        let source=b"local chunk=string.rep('x',513);local n=0;for i=1,400000 do local s=string.gsub('abcdef','.',function()return chunk end);n=n+#s end;return n";
        macro_rules! cpu {($lua:ident)=>{{let p=$lua::compile(source).unwrap();let mut vm=$lua::Vm::new();assert_eq!(vm.execute(&p).unwrap(),&[$lua::Value::Number(1231200000.)]);}}}
        match engine.as_str(){"parent"=>cpu!(lua_parent),"current"=>cpu!(lua_current),_=>panic!("unknown CPU fixture engine")};
        return;
    }
    macro_rules! heap {($name:expr,$engine:expr,$vm:ident)=>{{
        let h=$vm.heap_stats();
        println!("heap\t{}\t{}\t{}\t{}\t{}\t{}\t{}",$name,$engine,h.live_bytes,h.reserved_bytes,h.peak_reserved_bytes,h.collections,h.reclaimed_objects);
    }}}
    for len in [0usize,1,3,4,7,8,15,16,17,31,32,33,63,64,65,512,4096,65536,1048576] {
        let name=format!("intern_hit_{len}");
        let bytes:Vec<_>=(0..len).map(|i|((i*131+i/7)%256)as u8).collect();
        let repeats=(16_777_216/len.max(1)).clamp(16,200_000) as u32;
        let mut pv=lua_parent::Vm::new();let mut cv=lua_current::Vm::new();
        let p=pv.intern(&bytes);let c=cv.intern(&bytes);
        for sample in 0..9 {
            macro_rules! parent {()=>{measure(&name,"parent",sample,repeats,||{black_box(pv.intern(black_box(&bytes)));})}}
            macro_rules! current {()=>{measure(&name,"current",sample,repeats,||{black_box(cv.intern(black_box(&bytes)));})}}
            if sample%2==0 {parent!();current!()}else{current!();parent!()}
            assert_eq!(pv.intern(&bytes),p);assert_eq!(cv.intern(&bytes),c);
        }
        heap!(&name,"parent",pv);heap!(&name,"current",cv);
    }
    for (len,middle) in [(32,false),(512,false),(4096,false),(65536,false),(4096,true)] {
        let name=format!("intern_{}_{len}",if middle{"middle"}else{"miss"});
        let inputs:Vec<Vec<u8>>=(0..128u64).map(|i|{let mut b=vec![b'x';len];let pos=if middle{len/2}else{len-8};b[pos..pos+8].copy_from_slice(&i.to_le_bytes());b}).collect();
        for sample in 0..9 {
            let mut pv=lua_parent::Vm::new();let mut cv=lua_current::Vm::new();
            let mut pi=0;let mut ci=0;
            macro_rules! parent {()=>{measure(&name,"parent",sample,128,||{black_box(pv.intern(black_box(&inputs[pi])));pi+=1;})}}
            macro_rules! current {()=>{measure(&name,"current",sample,128,||{black_box(cv.intern(black_box(&inputs[ci])));ci+=1;})}}
            if sample%2==0 {parent!();current!()}else{current!();parent!()}
            for input in &inputs {
                let lua_parent::Value::String(id)=pv.intern(input)else{panic!()};assert_eq!(pv.string_bytes(id),Some(input.as_slice()));
                let lua_current::Value::String(id)=cv.intern(input)else{panic!()};assert_eq!(cv.string_bytes(id),Some(input.as_slice()));
            }
            if sample==8 {heap!(&name,"parent",pv);heap!(&name,"current",cv);}
        }
    }
    for (name,source,expected,repeats) in [
        ("gsub_callback_wide","local n=0;local chunk=string.rep('x',513);for i=1,100 do local s=string.gsub('abcdef','.',function()return chunk end);n=n+#s end;return n",307800.,100),
        ("long_concat","local s=string.rep('a',4096);local n=0;for i=1,1000 do n=n+#(s..'b')end;return n",4097000.,50),
        ("long_rep","local n=0;for i=1,100 do n=n+#string.rep('x',65536)end;return n",6553600.,50),
    ] {
        let p=lua_parent::compile(source.as_bytes()).unwrap();let c=lua_current::compile(source.as_bytes()).unwrap();
        assert_eq!(p.instruction_count(),c.instruction_count());assert_eq!(p.register_count(),c.register_count());
        println!("layout\t{name}\t{}\t{}",p.instruction_count(),p.register_count());
        let mut pv=lua_parent::Vm::new();let mut cv=lua_current::Vm::new();
        assert_eq!(pv.execute(&p).unwrap(),&[lua_parent::Value::Number(expected)]);
        assert_eq!(cv.execute(&c).unwrap(),&[lua_current::Value::Number(expected)]);
        for sample in 0..9 {
            macro_rules! parent {()=>{measure(name,"parent",sample,repeats,||{black_box(pv.execute(black_box(&p)).unwrap());})}}
            macro_rules! current {()=>{measure(name,"current",sample,repeats,||{black_box(cv.execute(black_box(&c)).unwrap());})}}
            if sample%2==0 {parent!();current!()}else{current!();parent!()}
        }
        heap!(name,"parent",pv);heap!(name,"current",cv);
    }
}
