
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
    for (name,source,expected,repeats,stopped) in [
        ("os_date_table", "local date=os.date;local n=0;for i=1,1000 do local d=date('!*t',946684800+i);n=n+d.year+d.month+d.day end;return n",2002000.,50,false),
        ("os_date_table_gc_stopped", "local date=os.date;local n=0;for i=1,1000 do local d=date('!*t',946684800+i);n=n+d.year+d.month+d.day end;return n",2002000.,30,true),
        ("os_date_table_extended", "local date=os.date;local n=0;for i=1,1000 do local d=date('!*t',946684800+i);d.extra=i;n=n+d.year+d.month+d.day+d.extra end;return n",2502500.,50,false),
        ("constant_left", "local x=1.25;local y=0;for i=1,10000 do y=1+(2*(3+(4*(5+x))))end;return y",57.,50,false),
    ] {
        let p=lua_parent::compile(source.as_bytes()).unwrap();
        let c=lua_current::compile(source.as_bytes()).unwrap();
        assert_eq!(p.instruction_count(),c.instruction_count());
        assert_eq!(p.register_count(),c.register_count());
        println!("layout\t{name}\t{}\t{}",p.instruction_count(),p.register_count());
        let mut pv=lua_parent::Vm::new();let mut cv=lua_current::Vm::new();
        assert_eq!(pv.execute(&p).unwrap(),&[lua_parent::Value::Number(expected)]);
        assert_eq!(cv.execute(&c).unwrap(),&[lua_current::Value::Number(expected)]);
        let pgc=lua_parent::compile(b"collectgarbage('collect');collectgarbage('stop')").unwrap();
        let cgc=lua_current::compile(b"collectgarbage('collect');collectgarbage('stop')").unwrap();
        if stopped {
            pv.execute(&lua_parent::compile(b"collectgarbage('stop')").unwrap()).unwrap();
            cv.execute(&lua_current::compile(b"collectgarbage('stop')").unwrap()).unwrap();
        }
        for sample in 0..9 {
            macro_rules! parent {()=>{measure(name,"parent",sample,repeats,||{
                black_box(pv.execute(black_box(&p)).unwrap());
            })}}
            macro_rules! current {()=>{measure(name,"current",sample,repeats,||{
                black_box(cv.execute(black_box(&c)).unwrap());
            })}}
            if sample%2==0 {parent!();current!()}else{current!();parent!()}
            if stopped {pv.execute(&pgc).unwrap();cv.execute(&cgc).unwrap();}
        }
        macro_rules! heap {($engine:expr,$vm:ident)=>{{
            let h=$vm.heap_stats();
            println!("heap\t{name}\t{}\t{}\t{}\t{}\t{}\t{}",$engine,
                h.live_bytes,h.reserved_bytes,h.peak_reserved_bytes,h.collections,h.reclaimed_objects);
        }}}
        heap!("parent",pv);heap!("current",cv);
    }
}
