"""Measure string interning and long-string execution against a committed Lua parent."""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import tarfile
import tempfile
import time


HARNESS = r'''
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
'''


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--parent', required=True, help='Git commit before the optimization')
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--cpu-time', action='store_true', help='Measure five paired Windows child-process CPU/wall times for 400,000 wide substitutions')
    args = parser.parse_args()
    if args.cpu_time and os.name != 'nt':
        raise SystemExit('--cpu-time requires Windows process CPU counters')
    root = Path(__file__).resolve().parents[1]
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    revision = subprocess.check_output(['git', 'rev-parse', '--verify', args.parent+'^{commit}'], cwd=root, text=True).strip()
    sources = list((root/'crates/deadlib-lua').rglob('*.rs'))
    sources += [root/p for p in ('Cargo.toml','Cargo.lock','.cargo/config.toml','crates/deadlib-lua/Cargo.toml')]
    sources.append(Path(__file__).resolve())
    def hashes() -> dict[str, str]:
        return {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(sources)}
    before = hashes()
    started = time.time()
    commands = []
    build_log = []
    cpu_samples = []
    with tempfile.TemporaryDirectory(prefix='lua-intern-', dir=root/'target') as temporary:
        directory = Path(temporary)
        assert directory.resolve().is_relative_to((root/'target').resolve())
        archive = subprocess.check_output(['git','archive','--format=tar',revision,'crates/deadlib-lua/src'], cwd=root)
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(directory/'parent', filter='data')
        parent_sources = sorted((directory/'parent/crates/deadlib-lua/src').rglob('*.rs'))
        parent_hashes = {p.relative_to(directory/'parent').as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in parent_sources}
        flags = ['--edition=2024','-C','opt-level=3','-C','debuginfo=1','-C','codegen-units=16','-C','embed-bitcode=yes','-C','panic=abort']
        if os.name == 'nt':
            flags += ['-C','target-feature=+crt-static']
        def build(command: list[str]) -> None:
            commands.append(command)
            run = subprocess.run(command, cwd=root, capture_output=True, text=True)
            build_log.append(run.stdout+run.stderr)
            output.with_suffix('.build.log').write_text(''.join(build_log), encoding='utf-8')
            run.check_returncode()
        libraries = {}
        for engine, path in [('parent', directory/'parent/crates/deadlib-lua/src/lib.rs'),('current',root/'crates/deadlib-lua/src/lib.rs')]:
            libraries[engine] = directory/f'liblua_{engine}.rlib'
            build(['rustc',*flags,'--crate-type=rlib','--crate-name',f'lua_{engine}',str(path),'-o',str(libraries[engine])])
        harness = directory/'compare.rs'
        harness.write_text(HARNESS, encoding='utf-8')
        output.with_suffix('.rs').write_text(HARNESS, encoding='utf-8')
        binary = directory/('compare.exe' if os.name == 'nt' else 'compare')
        build(['rustc',*flags,'-C','lto=fat',str(harness),'--extern',f"lua_parent={libraries['parent']}",'--extern',f"lua_current={libraries['current']}",'-o',str(binary)])
        binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
        environment = os.environ.copy()
        environment.pop('DEADLIB_LUA_INTERN_CPU', None)
        run = subprocess.run([str(binary)], cwd=root, env=environment, capture_output=True, text=True, check=True)
        output.with_suffix('.tsv').write_text(run.stdout, encoding='utf-8')
        assert not run.stderr, run.stderr
        if args.cpu_time:
            import ctypes
            from ctypes import wintypes
            kernel32=ctypes.WinDLL('kernel32',use_last_error=True)
            get_times=kernel32.GetProcessTimes
            get_times.argtypes=[wintypes.HANDLE]+[ctypes.POINTER(wintypes.FILETIME)]*4
            get_times.restype=wintypes.BOOL
            get_cycles=kernel32.QueryProcessCycleTime
            get_cycles.argtypes=[wintypes.HANDLE,ctypes.POINTER(ctypes.c_ulonglong)]
            get_cycles.restype=wintypes.BOOL
            def ticks(value):return (value.dwHighDateTime<<32)|value.dwLowDateTime
            for sample in range(5):
                for engine in (('parent','current') if sample%2==0 else ('current','parent')):
                    env=environment.copy();env['DEADLIB_LUA_INTERN_CPU']=engine
                    begin=time.perf_counter_ns()
                    child=subprocess.Popen([str(binary)],cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
                    try:stdout,stderr=child.communicate(timeout=120)
                    except subprocess.TimeoutExpired:
                        child.kill();child.communicate();raise
                    wall=time.perf_counter_ns()-begin
                    assert child.returncode==0 and not stdout and not stderr,(engine,child.returncode,stdout,stderr)
                    created,exited,kernel,user=[wintypes.FILETIME() for _ in range(4)]
                    if not get_times(wintypes.HANDLE(int(child._handle)),ctypes.byref(created),ctypes.byref(exited),ctypes.byref(kernel),ctypes.byref(user)):
                        raise ctypes.WinError(ctypes.get_last_error())
                    cycles=ctypes.c_ulonglong()
                    if not get_cycles(wintypes.HANDLE(int(child._handle)),ctypes.byref(cycles)):
                        raise ctypes.WinError(ctypes.get_last_error())
                    cpu_samples.append(dict(engine=engine,sample=sample,substitutions=400000,wall_ns=wall,kernel_100ns=ticks(kernel),user_100ns=ticks(user),cpu_ns=(ticks(kernel)+ticks(user))*100,cpu_cycles=cycles.value))
                    print('CPU fixture',sample,engine,flush=True)
    if before != hashes():
        raise SystemExit('sources changed during benchmark; results cannot be attributed')
    samples = [];layouts = {};heaps = {}
    for line in run.stdout.splitlines():
        fields=line.split('\t')
        if fields[0]=='sample':
            _,name,engine,sample,repeats,ns,allocs,reallocs,size=fields
            samples.append(dict(workload=name,engine=engine,sample=int(sample),repeats=int(repeats),ns_per_iteration=int(ns),allocations_total=int(allocs),reallocations_total=int(reallocs),allocated_bytes_total=int(size)))
        elif fields[0]=='layout':
            _,name,instructions,registers=fields
            layouts[name]=dict(instructions=int(instructions),entry_registers=int(registers))
        elif fields[0]=='heap':
            _,name,engine,live,reserved,peak,collections,reclaimed=fields
            heaps.setdefault(name,{})[engine]=dict(live_bytes=int(live),reserved_bytes=int(reserved),observed_peak_reserved_bytes=int(peak),collections=int(collections),reclaimed_objects=int(reclaimed))
    ratios = {}
    for name in sorted({row["workload"] for row in samples}):
        pairs={e:{s['sample']:s['ns_per_iteration'] for s in samples if s['engine']==e and s['workload']==name} for e in ('parent','current')}
        assert pairs['parent'].keys()==pairs['current'].keys()==set(range(9))
        ratios[name]=statistics.median(pairs['current'][i]/pairs['parent'][i] for i in range(9))
    assert len(layouts)==3 and len(heaps)==27 and len(samples)==486
    result=dict(parent_revision=revision,current_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
        dirty=bool(subprocess.check_output(['git','status','--porcelain'],cwd=root)),
        source_hashes=before,parent_source_hashes=parent_hashes,binary_sha256=binary_hash,
        protocol='Two independently compiled rlibs linked into one executable with identical flags and fat LTO; nine paired samples alternate parent/current order. Compile, input/VM setup and correctness are outside timing. Host hit fixtures reuse existing interned bytes; miss fixtures use fresh VMs and 128 distinct inputs per sample. Automatic GC is inside timing for Lua workloads.',
        build_commands=commands,utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),build_and_run_seconds=time.time()-started,
        platform=platform.platform(),processor=os.environ.get('PROCESSOR_IDENTIFIER',platform.processor()),rustc=subprocess.check_output(['rustc','-vV'],text=True),
        samples=samples,program_layouts=layouts,heap_estimates=heaps,median_paired_current_over_parent=ratios,cpu_samples=cpu_samples,
        missing_metrics=['exact allocator live/peak heap','process peak RSS','native CRT allocation counts']+([] if cpu_samples else ['process CPU time','CPU cycles']))
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(ratios,indent=2))


if __name__=='__main__':
    main()
