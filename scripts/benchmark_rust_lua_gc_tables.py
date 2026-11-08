"""Compare public incremental table collection against a committed Rust parent."""
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
'''

def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--parent',required=True)
    parser.add_argument('--output',required=True,type=Path)
    parser.add_argument('--cpu-time',action='store_true')
    args=parser.parse_args()
    if args.cpu_time and os.name!='nt':
        raise SystemExit('--cpu-time requires Windows process counters')
    root=Path(__file__).resolve().parents[1]
    output=args.output.resolve();output.parent.mkdir(parents=True,exist_ok=True)
    revision=subprocess.check_output(['git','rev-parse','--verify',args.parent+'^{commit}'],cwd=root,text=True).strip()
    sources=list((root/'crates/deadlib-lua').rglob('*.rs'))
    sources += [root/p for p in ('Cargo.toml','Cargo.lock','.cargo/config.toml','crates/deadlib-lua/Cargo.toml')]
    sources.append(Path(__file__).resolve())
    def hashes():
        return {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(sources)}
    before=hashes();started=time.time();commands=[];build_log=[];cpu_samples=[]
    with tempfile.TemporaryDirectory(prefix='lua-gc-tables-',dir=root/'target') as temporary:
        directory=Path(temporary)
        assert directory.resolve().is_relative_to((root/'target').resolve())
        archive=subprocess.check_output(['git','archive','--format=tar',revision,'crates/deadlib-lua/src'],cwd=root)
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(directory/'parent',filter='data')
        parent_hashes={p.relative_to(directory/'parent').as_posix():hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted((directory/'parent/crates/deadlib-lua/src').rglob('*.rs'))}
        flags=['--edition=2024','-C','opt-level=3','-C','debuginfo=1','-C','codegen-units=16','-C','embed-bitcode=yes','-C','panic=abort']
        if os.name=='nt':flags += ['-C','target-feature=+crt-static']
        def build(command):
            commands.append(command)
            run=subprocess.run(command,cwd=root,capture_output=True,text=True)
            build_log.append(run.stdout+run.stderr)
            output.with_suffix('.build.log').write_text(''.join(build_log),encoding='utf-8')
            run.check_returncode()
        libraries={}
        for engine,path in [('parent',directory/'parent/crates/deadlib-lua/src/lib.rs'),('current',root/'crates/deadlib-lua/src/lib.rs')]:
            libraries[engine]=directory/f'liblua_{engine}.rlib'
            build(['rustc',*flags,'--crate-type=rlib','--crate-name',f'lua_{engine}',str(path),'-o',str(libraries[engine])])
        harness=directory/'compare.rs';harness.write_text(HARNESS,encoding='utf-8')
        output.with_suffix('.rs').write_text(HARNESS,encoding='utf-8')
        binary=directory/('compare.exe' if os.name=='nt' else 'compare')
        build(['rustc',*flags,'-C','lto=fat',str(harness),'--extern',f"lua_parent={libraries['parent']}",'--extern',f"lua_current={libraries['current']}",'-o',str(binary)])
        binary_hash=hashlib.sha256(binary.read_bytes()).hexdigest()
        environment=os.environ.copy();environment.pop('DEADLIB_LUA_GC_CPU',None)
        run=subprocess.run([str(binary)],cwd=root,env=environment,capture_output=True,text=True,check=True,timeout=180)
        assert not run.stderr,run.stderr
        output.with_suffix('.tsv').write_text(run.stdout,encoding='utf-8')
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
                    env=environment.copy();env['DEADLIB_LUA_GC_CPU']=engine
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
                    cpu_samples.append(dict(engine=engine,sample=sample,children=100000,collections=32,
                        wall_ns=wall,kernel_100ns=ticks(kernel),user_100ns=ticks(user),
                        cpu_ns=(ticks(kernel)+ticks(user))*100,cpu_cycles=cycles.value))
    if before!=hashes():raise SystemExit('sources changed during benchmark')
    samples=[];sizes={}
    keys=('elapsed_ns','steps','median_step_ns','p99_step_ns','max_step_ns','allocations','reallocations',
          'requested_bytes','before_reserved_bytes','reserved_bytes','observed_peak_reserved_bytes','live_bytes')
    for line in run.stdout.splitlines():
        fields=line.split('\t')
        if fields[0]=='sample':
            _,name,engine,sample,*values=fields
            assert len(values)==len(keys)
            samples.append(dict(workload=name,engine=engine,sample=int(sample),**dict(zip(keys,map(int,values)))))
        elif fields[0]=='size':
            _,engine,vm,value,table=fields
            sizes[engine]=dict(vm_bytes=int(vm),value_bytes=int(value),empty_table_live_bytes=int(table))
    ratios={}
    for name in sorted({row['workload'] for row in samples}):
        pairs={e:{s['sample']:s for s in samples if s['engine']==e and s['workload']==name} for e in ('parent','current')}
        assert pairs['parent'].keys()==pairs['current'].keys()==set(range(9))
        ratios[name]={key:statistics.median(pairs['current'][i][key]/pairs['parent'][i][key] for i in range(9))
            for key in ('elapsed_ns','max_step_ns','reserved_bytes')}
    assert len(samples)==126 and len(sizes)==2
    result=dict(parent_revision=revision,current_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
        source_hashes=before,parent_source_hashes=parent_hashes,binary_sha256=binary_hash,
        protocol="Separately built rlibs, identical optimization flags, static CRT on Windows, one fat-LTO executable; nine alternating parent/current pairs. Fresh stopped VM per sample, preloaded step program, pinned table, setup and correctness outside timing. Time every public execute(step) until one complete cycle at default step multiplier 200. Each step has an Instant timer; total includes timing/bookkeeping. No phase instrumentation: maximum includes atomic work. Allocation counters surround the cycle only. CPU child fixture builds 100,000 child tables then runs 32 complete cycles; process counters include startup/setup/shutdown.",
        build_commands=commands,utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),build_and_run_seconds=time.time()-started,
        platform=platform.platform(),processor=os.environ.get('PROCESSOR_IDENTIFIER',platform.processor()),
        rustc=subprocess.check_output(['rustc','-vV'],text=True),samples=samples,sizes=sizes,
        median_paired_current_over_parent=ratios,cpu_samples=cpu_samples,
        missing_metrics=['exact allocator live/peak heap','process peak RSS','sampled CPU call stacks','native CRT allocations'])
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(ratios,indent=2))
if __name__=='__main__':
    main()
