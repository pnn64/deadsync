"""Compare calendar table allocation against a committed independent-Lua parent."""
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
'''


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--parent', required=True, help='Git commit before the optimization')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
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
    with tempfile.TemporaryDirectory(prefix='lua-records-', dir=root/'target') as temporary:
        directory = Path(temporary)
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
        run = subprocess.run([str(binary)], cwd=root, capture_output=True, text=True, check=True)
        output.with_suffix('.tsv').write_text(run.stdout, encoding='utf-8')
        assert not run.stderr, run.stderr
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
    for name in layouts:
        pairs={e:{s['sample']:s['ns_per_iteration'] for s in samples if s['engine']==e and s['workload']==name} for e in ('parent','current')}
        assert pairs['parent'].keys()==pairs['current'].keys()==set(range(9))
        ratios[name]=statistics.median(pairs['current'][i]/pairs['parent'][i] for i in range(9))
    assert len(layouts)==len(heaps)==4 and len(samples)==72
    result=dict(parent_revision=revision,current_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
        dirty=bool(subprocess.check_output(['git','status','--porcelain'],cwd=root)),
        source_hashes=before,parent_source_hashes=parent_hashes,binary_sha256=binary_hash,
        protocol='Two independently compiled rlibs linked into one executable with identical flags and fat LTO; nine paired samples alternate parent/current order. Compile, setup, correctness and manual full collections for the stopped-GC fixture are outside timing. Automatic GC is inside timing for other fixtures.',
        build_commands=commands,utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),build_and_run_seconds=time.time()-started,
        platform=platform.platform(),processor=os.environ.get('PROCESSOR_IDENTIFIER',platform.processor()),rustc=subprocess.check_output(['rustc','-vV'],text=True),
        samples=samples,program_layouts=layouts,heap_estimates=heaps,median_paired_current_over_parent=ratios,
        missing_metrics=['exact allocator live/peak heap','process peak RSS','CPU cycles','native CRT allocation counts'])
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(ratios,indent=2))


if __name__=='__main__':
    main()
