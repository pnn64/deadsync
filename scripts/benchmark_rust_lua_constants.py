"""Compare compiler constant pooling with a committed parent."""
from __future__ import annotations
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import tarfile
import tempfile
import time

HARNESS = r'''
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
'''

def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--parent',required=True)
    parser.add_argument('--output',required=True,type=Path)
    parser.add_argument('--cpu-time',action='store_true')
    args=parser.parse_args()
    if args.cpu_time and os.name!='nt':raise SystemExit('--cpu-time requires Windows')
    root=Path(__file__).resolve().parents[1];output=args.output.resolve();output.parent.mkdir(parents=True,exist_ok=True)
    revision=subprocess.check_output(['git','rev-parse','--verify',args.parent+'^{commit}'],cwd=root,text=True).strip()
    sources=list((root/'crates/deadlib-lua').rglob('*.rs'))+[root/p for p in ('Cargo.toml','Cargo.lock','.cargo/config.toml','crates/deadlib-lua/Cargo.toml')]+[Path(__file__).resolve()]
    def hashes():return {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(sources)}
    before=hashes();started=time.time();commands=[];logs=[];cpu_samples=[]
    # Both copies use an identical benchmark-only build identity so each engine
    # can read the other's files and exact serialized-byte equality is testable.
    shared_build='0123456789abcdef0123456789abcdef'
    wrappers={"src/compiler.rs":"\npub(crate) fn benchmark_compiler_function_bytes() -> usize { std::mem::size_of::<Function<'static>>() }\n","src/lib.rs":"\npub fn benchmark_compiler_function_bytes() -> usize { compiler::benchmark_compiler_function_bytes() }\n"}
    with tempfile.TemporaryDirectory(prefix='lua-constants-',dir=root/'target') as temporary:
        directory=Path(temporary);assert directory.resolve().is_relative_to((root/'target').resolve())
        archive=subprocess.check_output(['git','archive','--format=tar',revision,'crates/deadlib-lua/src'],cwd=root)
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:tar.extractall(directory/'parent',filter='data')
        parent=directory/'parent/crates/deadlib-lua'
        parent_hashes={p.relative_to(directory/'parent').as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((parent/'src').rglob('*.rs'))}
        current=directory/'current';shutil.copytree(root/'crates/deadlib-lua/src',current/'src')
        for copy in (parent,current):
            for relative,suffix in wrappers.items():
                with (copy/relative).open('a',encoding='utf-8',newline='\n') as file:file.write(suffix)
        flags=['--edition=2024','-C','opt-level=3','-C','debuginfo=1','-C','codegen-units=16','-C','embed-bitcode=yes','-C','panic=abort']
        if os.name=='nt':flags += ['-C','target-feature=+crt-static']
        env=os.environ.copy();env['DEADLIB_LUA_CACHE_BUILD']=shared_build
        def build(command):
            commands.append(command);run=subprocess.run(command,cwd=root,env=env,capture_output=True,text=True)
            logs.append(run.stdout+run.stderr);output.with_suffix('.build.log').write_text(''.join(logs),encoding='utf-8');run.check_returncode()
        libraries={}
        for engine,copy in [('parent',parent),('current',current)]:
            libraries[engine]=directory/f'liblua_{engine}.rlib'
            build(['rustc',*flags,'--crate-type=rlib','--crate-name',f'lua_{engine}',str(copy/'src/lib.rs'),'-o',str(libraries[engine])])
        harness=directory/'compare.rs';harness.write_text(HARNESS,encoding='utf-8');output.with_suffix('.rs').write_text(HARNESS,encoding='utf-8')
        binary=directory/('compare.exe'if os.name=='nt'else'compare')
        build(['rustc',*flags,'-C','lto=fat',str(harness),'--extern',f"lua_parent={libraries['parent']}",'--extern',f"lua_current={libraries['current']}",'-o',str(binary)])
        binary_hash=hashlib.sha256(binary.read_bytes()).hexdigest()
        environment=os.environ.copy();environment.pop('DEADLIB_LUA_CONSTANT_CPU',None)
        run=subprocess.run([str(binary),str(directory/'work')],cwd=root,env=environment,capture_output=True,text=True,check=True,timeout=180)
        assert not run.stderr,run.stderr
        output.with_suffix('.tsv').write_text(run.stdout,encoding='utf-8')
        if args.cpu_time:
            import ctypes
            from ctypes import wintypes
            kernel32=ctypes.WinDLL('kernel32',use_last_error=True)
            get_times=kernel32.GetProcessTimes;get_times.argtypes=[wintypes.HANDLE]+[ctypes.POINTER(wintypes.FILETIME)]*4;get_times.restype=wintypes.BOOL
            get_cycles=kernel32.QueryProcessCycleTime;get_cycles.argtypes=[wintypes.HANDLE,ctypes.POINTER(ctypes.c_ulonglong)];get_cycles.restype=wintypes.BOOL
            def ticks(value):return(value.dwHighDateTime<<32)|value.dwLowDateTime
            for sample in range(5):
                for engine in(('parent','current')if sample%2==0 else('current','parent')):
                    env=environment.copy();env['DEADLIB_LUA_CONSTANT_CPU']=engine;begin=time.perf_counter_ns()
                    child=subprocess.Popen([str(binary),str(directory/'work')],cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
                    try:stdout,stderr=child.communicate(timeout=120)
                    except subprocess.TimeoutExpired:child.kill();child.communicate();raise
                    wall=time.perf_counter_ns()-begin
                    assert child.returncode==0 and not stdout and not stderr,(engine,child.returncode,stdout,stderr)
                    created,exited,kernel,user=[wintypes.FILETIME()for _ in range(4)]
                    if not get_times(wintypes.HANDLE(int(child._handle)),ctypes.byref(created),ctypes.byref(exited),ctypes.byref(kernel),ctypes.byref(user)):raise ctypes.WinError(ctypes.get_last_error())
                    cycles=ctypes.c_ulonglong()
                    if not get_cycles(wintypes.HANDLE(int(child._handle)),ctypes.byref(cycles)):raise ctypes.WinError(ctypes.get_last_error())
                    cpu_samples.append(dict(engine=engine,sample=sample,fixture='records_4096',compiles=32,wall_ns=wall,cpu_ns=(ticks(kernel)+ticks(user))*100,cpu_cycles=cycles.value,kernel_100ns=ticks(kernel),user_100ns=ticks(user)))
    if before!=hashes():raise SystemExit('sources changed during benchmark')
    samples=[];layouts={};heaps={};sizes={}
    for line in run.stdout.splitlines():
        f=line.split('\t')
        if f[0]=='sample':
            _,name,engine,sample,repeats,ns,allocs,reallocs,size,peak=f
            samples.append(dict(workload=name,engine=engine,sample=int(sample),repeats=int(repeats),ns_per_iteration=float(ns),allocations_total=int(allocs),reallocations_total=int(reallocs),requested_bytes_total=int(size),peak_tracked_live_bytes=int(peak)))
        elif f[0]=='layout':
            _,name,ins,regs,source,file=f;layouts[name]=dict(instructions=int(ins),entry_registers=int(regs),source_bytes=int(source),cache_file_bytes=int(file))
        elif f[0]=='heap':
            _,name,engine,*values=f;heaps.setdefault(name,{})[engine]=dict(zip(('live_bytes','reserved_bytes','observed_peak_reserved_bytes','collections','reclaimed_objects'),map(int,values)))
        elif f[0]=='size':
            _,engine,vm,value,table=f;sizes[engine]=dict(vm_bytes=int(vm),value_bytes=int(value),compiler_function_bytes=int(table))
    ratios={}
    for name in sorted({x['workload']for x in samples}):
        pairs={e:{x['sample']:x for x in samples if x['engine']==e and x['workload']==name}for e in('parent','current')}
        assert all(set(v)==set(range(9))for v in pairs.values())
        for i in range(9):
            assert pairs['parent'][i]['repeats']==pairs['current'][i]['repeats']
        ratios[name]=statistics.median(pairs['current'][i]['ns_per_iteration']/pairs['parent'][i]['ns_per_iteration']for i in range(9))
    assert len(samples)==342 and len(layouts)==len(heaps)==19 and len(sizes)==2 and all(h['parent']==h['current']for h in heaps.values())
    result=dict(parent_revision=revision,current_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
        source_hashes=before,parent_source_hashes=parent_hashes,benchmark_only_wrappers=wrappers,benchmark_only_shared_build_id=shared_build,
        binary_sha256=binary_hash,build_commands=commands,utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),build_and_run_seconds=time.time()-started,
        platform=platform.platform(),processor=os.environ.get('PROCESSOR_IDENTIFIER',platform.processor()),rustc=subprocess.check_output(['rustc','-vV'],text=True),
        protocol="Two source copies with identical compiler-function size accessors and a fixed benchmark-only cache build ID; separately built rlibs with identical flags in one fat-LTO executable. Nine alternating pairs measure compile_named including Program drop. Inputs, correctness, layouts, heaps and artifact checks are outside timing. Both compilers produce byte-identical complete cache artifacts and cross-read them. Rust allocation layouts update logical live/peak counters; realloc uses its net size delta, internal allocator overlap is unobserved, and every interval asserts zero live balance with no underflow. CPU children compile the 4096-field record 32 times; counters include startup/input setup/cleanup, with allocation counting disabled.",
        samples=samples,program_layouts=layouts,heap_estimates=heaps,sizes=sizes,median_paired_current_over_parent=ratios,cpu_samples=cpu_samples,
        missing_metrics=['exact allocator live/peak heap','process peak RSS','cold physical disk latency','sampled CPU call stacks','native CRT allocations'])
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(ratios,indent=2))
if __name__=='__main__':main()
