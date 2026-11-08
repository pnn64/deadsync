"""Compare safe cache CRC and full persistent-cache calls with a committed parent."""
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
    wrappers={'src/cache/codec.rs':'\npub(super) fn benchmark_crc_table_bytes() -> usize { std::mem::size_of_val(&CRC) }\n',
        'src/cache.rs':'\npub(crate) fn benchmark_checksum(bytes: &[u8]) -> u32 { codec::checksum(bytes) }\npub(crate) fn benchmark_crc_table_bytes() -> usize { codec::benchmark_crc_table_bytes() }\n',
        'src/lib.rs':'\npub fn benchmark_checksum(bytes: &[u8]) -> u32 { cache::benchmark_checksum(bytes) }\npub fn benchmark_crc_table_bytes() -> usize { cache::benchmark_crc_table_bytes() }\n'}
    with tempfile.TemporaryDirectory(prefix='lua-cache-crc-',dir=root/'target') as temporary:
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
        environment=os.environ.copy();environment.pop('DEADLIB_LUA_CRC_CPU',None)
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
                    env=environment.copy();env['DEADLIB_LUA_CRC_CPU']=engine;begin=time.perf_counter_ns()
                    child=subprocess.Popen([str(binary),str(directory/'work')],cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
                    try:stdout,stderr=child.communicate(timeout=120)
                    except subprocess.TimeoutExpired:child.kill();child.communicate();raise
                    wall=time.perf_counter_ns()-begin
                    assert child.returncode==0 and not stdout and not stderr,(engine,child.returncode,stdout,stderr)
                    created,exited,kernel,user=[wintypes.FILETIME()for _ in range(4)]
                    if not get_times(wintypes.HANDLE(int(child._handle)),ctypes.byref(created),ctypes.byref(exited),ctypes.byref(kernel),ctypes.byref(user)):raise ctypes.WinError(ctypes.get_last_error())
                    cycles=ctypes.c_ulonglong()
                    if not get_cycles(wintypes.HANDLE(int(child._handle)),ctypes.byref(cycles)):raise ctypes.WinError(ctypes.get_last_error())
                    cpu_samples.append(dict(engine=engine,sample=sample,fixture='native_closures_512',loads=500,wall_ns=wall,cpu_ns=(ticks(kernel)+ticks(user))*100,cpu_cycles=cycles.value,kernel_100ns=ticks(kernel),user_100ns=ticks(user)))
    if before!=hashes():raise SystemExit('sources changed during benchmark')
    samples=[];layouts={};heaps={};sizes={}
    for line in run.stdout.splitlines():
        f=line.split('\t')
        if f[0]=='sample':
            _,name,engine,sample,repeats,ns,allocs,reallocs,size=f
            samples.append(dict(workload=name,engine=engine,sample=int(sample),repeats=int(repeats),ns_per_iteration=float(ns),allocations_total=int(allocs),reallocations_total=int(reallocs),requested_bytes_total=int(size)))
        elif f[0]=='layout':
            _,name,ins,regs,source,file=f;layouts[name]=dict(instructions=int(ins),entry_registers=int(regs),source_bytes=int(source),cache_file_bytes=int(file))
        elif f[0]=='heap':
            _,name,engine,*values=f;heaps.setdefault(name,{})[engine]=dict(zip(('live_bytes','reserved_bytes','observed_peak_reserved_bytes','collections','reclaimed_objects'),map(int,values)))
        elif f[0]=='size':
            _,engine,vm,value,table=f;sizes[engine]=dict(vm_bytes=int(vm),value_bytes=int(value),crc_table_bytes=int(table))
    ratios={}
    for name in sorted({x['workload']for x in samples}):
        pairs={e:{x['sample']:x for x in samples if x['engine']==e and x['workload']==name}for e in('parent','current')}
        assert all(set(v)==set(range(9))for v in pairs.values())
        for i in range(9):
            assert all(pairs['parent'][i][k]==pairs['current'][i][k] for k in('repeats','allocations_total','reallocations_total','requested_bytes_total')),(name,i)
        ratios[name]=statistics.median(pairs['current'][i]['ns_per_iteration']/pairs['parent'][i]['ns_per_iteration']for i in range(9))
    assert len(samples)==468 and len(layouts)==len(heaps)==5 and len(sizes)==2 and all(h['parent']==h['current']for h in heaps.values())
    result=dict(parent_revision=revision,current_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
        source_hashes=before,parent_source_hashes=parent_hashes,benchmark_only_wrappers=wrappers,benchmark_only_shared_build_id=shared_build,
        binary_sha256=binary_hash,build_commands=commands,utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),build_and_run_seconds=time.time()-started,
        platform=platform.platform(),processor=os.environ.get('PROCESSOR_IDENTIFIER',platform.processor()),rustc=subprocess.check_output(['rustc','-vV'],text=True),
        protocol="Two source copies with identical private checksum forwarding wrappers and a fixed benchmark-only cache build ID; independently built rlibs with identical flags and one fat-LTO executable. Nine alternating pairs. CRC timing loops exclude per-call timers; public hit/store calls include per-call timing and Program drop, with forced-miss deletion outside timing/counting. Both engines cross-read and generate byte-identical entries. Native input comes from committed parent's existing Lua 5.1 dumper. Setup/layout/results/heaps are checked outside timing. OS file cache is warm. CPU children each prime then perform 500 native-file hits; counters include setup/cleanup and disabled allocation-counter wrappers.",
        samples=samples,program_layouts=layouts,heap_estimates=heaps,sizes=sizes,median_paired_current_over_parent=ratios,cpu_samples=cpu_samples,
        missing_metrics=['exact allocator live/peak heap','process peak RSS','cold physical disk latency','sampled CPU call stacks','native CRT allocations'])
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(ratios,indent=2))
if __name__=='__main__':main()
