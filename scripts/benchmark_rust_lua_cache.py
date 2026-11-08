"""Measure persistent shared-bytecode loads against compilation and cache stores."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time

def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',required=True,type=Path)
    parser.add_argument('--cpu-time',action='store_true')
    parser.add_argument('--target-dir',type=Path)
    args=parser.parse_args()
    if args.cpu_time and os.name!='nt':
        raise SystemExit('--cpu-time requires Windows process counters')
    root=Path(__file__).resolve().parents[1]
    output=args.output.resolve();output.parent.mkdir(parents=True,exist_ok=True)
    sources=list((root/'crates/deadlib-lua').rglob('*.rs'))
    sources += [root/p for p in ('Cargo.toml','Cargo.lock','.cargo/config.toml','crates/deadlib-lua/Cargo.toml')]
    sources.append(Path(__file__).resolve())
    reference=[p for p in (root/'vendor/lua-src/lua-5.1.5').iterdir() if p.suffix in ('.c','.h')]
    def hashes(paths):
        return {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(paths)}
    before=hashes(sources);oracle=hashes(reference);started=time.time()
    command=['cargo','bench','-p','deadlib-lua','--bench','cache','--no-run','--message-format=json']
    if args.target_dir:command += ['--target-dir',str(args.target_dir.resolve())]
    build=subprocess.run(command,cwd=root,capture_output=True,text=True)
    output.with_suffix('.build.log').write_text(build.stderr,encoding='utf-8')
    output.with_suffix('.build.jsonl').write_text(build.stdout,encoding='utf-8')
    build.check_returncode()
    artifacts=[json.loads(line) for line in build.stdout.splitlines() if line.startswith('{')]
    binaries=[Path(row['executable']) for row in artifacts if row.get('reason')=='compiler-artifact'
        and row['target']['name']=='cache' and row.get('executable')]
    assert len(binaries)==1,binaries
    binary=binaries[0];binary_hash=hashlib.sha256(binary.read_bytes()).hexdigest()
    environment=os.environ.copy();environment.pop('DEADLIB_LUA_CACHE_CPU',None)
    run=subprocess.run([str(binary)],cwd=root,env=environment,capture_output=True,text=True,check=True,timeout=300)
    assert not run.stderr,run.stderr
    output.with_suffix('.tsv').write_text(run.stdout,encoding='utf-8')
    cpu_samples=[]
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
            for metric in (('compile','hit') if sample%2==0 else ('hit','compile')):
                env=environment.copy();env['DEADLIB_LUA_CACHE_CPU']=metric
                begin=time.perf_counter_ns()
                child=subprocess.Popen([str(binary)],cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
                try:stdout,stderr=child.communicate(timeout=120)
                except subprocess.TimeoutExpired:
                    child.kill();child.communicate();raise
                wall=time.perf_counter_ns()-begin
                assert child.returncode==0 and not stdout and not stderr,(metric,child.returncode,stdout,stderr)
                created,exited,kernel,user=[wintypes.FILETIME() for _ in range(4)]
                if not get_times(wintypes.HANDLE(int(child._handle)),ctypes.byref(created),ctypes.byref(exited),ctypes.byref(kernel),ctypes.byref(user)):
                    raise ctypes.WinError(ctypes.get_last_error())
                cycles=ctypes.c_ulonglong()
                if not get_cycles(wintypes.HANDLE(int(child._handle)),ctypes.byref(cycles)):
                    raise ctypes.WinError(ctypes.get_last_error())
                cpu_samples.append(dict(metric=metric,sample=sample,loads=300,fixture='closures_512',
                    wall_ns=wall,kernel_100ns=ticks(kernel),user_100ns=ticks(user),
                    cpu_ns=(ticks(kernel)+ticks(user))*100,cpu_cycles=cycles.value))
    if before!=hashes(sources) or oracle!=hashes(reference):
        raise SystemExit('sources changed during benchmark')
    samples=[];layouts={};heaps={};metadata={}
    for line in run.stdout.splitlines():
        fields=line.split('\t')
        if fields[0]=='sample':
            _,name,metric,sample,repeats,ns,allocs,reallocs,size=fields
            samples.append(dict(workload=name,metric=metric,sample=int(sample),repeats=int(repeats),
                ns_per_iteration=int(ns),allocations_total=int(allocs),reallocations_total=int(reallocs),requested_bytes_total=int(size)))
        elif fields[0]=='layout':
            _,name,instructions,registers,source,file=fields
            layouts[name]=dict(instructions=int(instructions),entry_registers=int(registers),source_bytes=int(source),cache_file_bytes=int(file))
        elif fields[0]=='heap':
            _,name,metric,*values=fields
            heaps.setdefault(name,{})[metric]=dict(zip(
                ('live_bytes','reserved_bytes','after_collect_live_bytes','after_collect_reserved_bytes','collections','reclaimed_objects'),
                map(int,values)))
        elif fields[0]=='meta':
            _,name,value=fields;metadata[name]=value
    ratios={}
    for name in layouts:
        pairs={metric:{r['sample']:r['ns_per_iteration'] for r in samples if r['workload']==name and r['metric']==metric}
            for metric in ('compile','hit','store')}
        assert all(set(rows)==set(range(9)) for rows in pairs.values())
        ratios[name]={metric+'_over_compile':statistics.median(pairs[metric][i]/pairs['compile'][i] for i in range(9))
            for metric in ('hit','store')}
        assert heaps[name]['compile']==heaps[name]['hit']
    assert len(samples)==135 and len(layouts)==len(heaps)==5
    result=dict(command=command,binary=str(binary),binary_sha256=binary_hash,
        source_hashes=before,reference_source_hashes=oracle,
        utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),build_and_run_seconds=time.time()-started,
        platform=platform.platform(),processor=os.environ.get('PROCESSOR_IDENTIFIER',platform.processor()),
        rustc=subprocess.check_output(['rustc','-vV'],text=True),
        git_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
        protocol="Cargo-built cache bench with counting Rust System allocator. Nine pairs alternate compile/hit order; store follows each pair. Setup and VM correctness/layout/heap checks are outside timing. File deletion forcing store misses is outside timing/counting. Each iteration includes Program drop and its own timer. Loads include index hashing, file open/read, integrity/context/source/name checks, decoding, verification and drop. OS file cache is warm. Store includes compilation, encoding, temporary-file write and rename. CPU children each prime the same cache, then perform 300 compile or hit calls; counting is disabled but the allocator wrapper remains. Process counters include startup/setup/cleanup.",
        metadata=metadata,samples=samples,program_layouts=layouts,heap_estimates=heaps,
        median_paired_ratios=ratios,cpu_samples=cpu_samples,
        missing_metrics=['exact allocator live/peak heap','process peak RSS','cold physical disk latency','sampled CPU call stacks','native CRT allocations'])
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(ratios,indent=2))
if __name__=='__main__':
    main()
