"""Measure Rust callback workloads and allocation counts."""
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
    args=parser.parse_args()
    if args.cpu_time and os.name!='nt':raise SystemExit('--cpu-time requires Windows process counters')
    root=Path(__file__).resolve().parents[1];output=args.output.resolve();output.parent.mkdir(parents=True,exist_ok=True)
    sources=list((root/'crates/deadlib-lua').rglob('*.rs'))+[Path(__file__).resolve()]
    sources += [root/n for n in ('Cargo.toml','Cargo.lock','.cargo/config.toml','crates/deadlib-lua/Cargo.toml')]
    def hashes(paths):return {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest()for p in sorted(paths)}
    before=hashes(sources);started=time.time()
    command=['cargo','bench','-p','deadlib-lua','--bench','host_functions','--no-run','--message-format=json']
    build=subprocess.run(command,cwd=root,capture_output=True,text=True)
    output.with_suffix('.build.log').write_text(build.stderr,encoding='utf-8');output.with_suffix('.build.jsonl').write_text(build.stdout,encoding='utf-8');build.check_returncode()
    artifacts=[json.loads(line)for line in build.stdout.splitlines()if line.startswith('{')]
    binaries=[Path(row['executable'])for row in artifacts if row.get('reason')=='compiler-artifact'and row['target']['name']=='host_functions'and row.get('executable')]
    assert len(binaries)==1,binaries
    binary=binaries[0];binary_hash=hashlib.sha256(binary.read_bytes()).hexdigest()
    env=os.environ.copy();env.pop('DEADLIB_LUA_HOST_CPU',None);env.pop('DEADLIB_LUA_HOST_CPU_FIXTURE',None)
    run=subprocess.run([str(binary)],cwd=root,env=env,capture_output=True,text=True,check=True,timeout=180)
    assert not run.stderr,run.stderr
    output.with_suffix('.tsv').write_text(run.stdout,encoding='utf-8')
    cpu_samples=[]
    if args.cpu_time:
        import ctypes
        from ctypes import wintypes
        kernel32=ctypes.WinDLL('kernel32',use_last_error=True)
        get_times=kernel32.GetProcessTimes;get_times.argtypes=[wintypes.HANDLE]+[ctypes.POINTER(wintypes.FILETIME)]*4;get_times.restype=wintypes.BOOL
        get_cycles=kernel32.QueryProcessCycleTime;get_cycles.argtypes=[wintypes.HANDLE,ctypes.POINTER(ctypes.c_ulonglong)];get_cycles.restype=wintypes.BOOL
        def ticks(value):return(value.dwHighDateTime<<32)|value.dwLowDateTime
        for fixture in ('add_lua_loop','reenter_add_lua_loop'):
            for sample in range(5):
                for engine in ('rust',):
                    child_env=env.copy();child_env['DEADLIB_LUA_HOST_CPU']=engine;child_env['DEADLIB_LUA_HOST_CPU_FIXTURE']=fixture;begin=time.perf_counter_ns()
                    child=subprocess.Popen([str(binary)],cwd=root,env=child_env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
                    try:stdout,stderr=child.communicate(timeout=120)
                    except subprocess.TimeoutExpired:child.kill();child.communicate();raise
                    wall=time.perf_counter_ns()-begin
                    assert child.returncode==0 and not stdout and not stderr,(engine,child.returncode,stdout,stderr)
                    created,exited,kernel,user=[wintypes.FILETIME()for _ in range(4)]
                    if not get_times(wintypes.HANDLE(int(child._handle)),ctypes.byref(created),ctypes.byref(exited),ctypes.byref(kernel),ctypes.byref(user)):raise ctypes.WinError(ctypes.get_last_error())
                    cycles=ctypes.c_ulonglong()
                    if not get_cycles(wintypes.HANDLE(int(child._handle)),ctypes.byref(cycles)):raise ctypes.WinError(ctypes.get_last_error())
                    cpu_samples.append(dict(engine=engine,sample=sample,fixture=fixture,executions=400,callbacks=2_000_000,warmup_callbacks=5000,wall_ns=wall,
                        kernel_100ns=ticks(kernel),user_100ns=ticks(user),cpu_ns=(ticks(kernel)+ticks(user))*100,cpu_cycles=cycles.value))
    assert hashes(sources)==before
    samples=[];layouts={};heaps={};metadata={}
    for line in run.stdout.splitlines():
        fields=line.split('\t')
        if fields[0]=='sample':
            _,name,engine,sample,repeats,calls,ns,alloc,realloc,size=fields
            samples.append(dict(workload=name,engine=engine,sample=int(sample),repeats=int(repeats),callbacks_per_iteration=int(calls),ns_per_iteration=int(ns),
                allocations_total=int(alloc),reallocations_total=int(realloc),requested_bytes_total=int(size)))
        elif fields[0]=='layout':
            _,name,instructions,registers=fields;layouts[name]=dict(instructions=int(instructions),entry_registers=int(registers))
        elif fields[0]=='heap':
            _,name,live,reserved,natives=fields
            heaps[name]=dict(rust_live_bytes=int(live),rust_reserved_bytes=int(reserved),rust_native_closures=int(natives))
        elif fields[0]=='meta':
            _,name,value=fields;metadata[name]=value
    medians={}
    for name in {row['workload'] for row in samples}:
        rows=[row for row in samples if row['workload']==name]
        assert len(rows)==9 and {row['sample'] for row in rows}==set(range(9))
        assert all(row['engine']=='rust' for row in rows)
        medians[name]=statistics.median(row['ns_per_iteration'] for row in rows)
    assert len(samples)==54 and len(medians)==6 and len(layouts)==len(heaps)==5
    assert all(x['allocations_total']==x['reallocations_total']==x['requested_bytes_total']==0 for x in samples if x['engine']=='rust')
    result=dict(command=command,binary=str(binary),binary_sha256=binary_hash,source_hashes=before,
        utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),build_and_run_seconds=time.time()-started,platform=platform.platform(),processor=os.environ.get('PROCESSOR_IDENTIFIER',platform.processor()),
        rustc=subprocess.check_output(['rustc','-vV'],text=True),git_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
        protocol='Nine samples for five 5000-callback Lua loops and one direct Rust entry. Two loops reenter Lua through HostCall, returning one or four values. Child functions are bound before timing. Three warmups, literal result assertions, layouts and heap observations are outside timing. Warmed Rust paths assert zero System allocations, reallocations and requests. Optional CPU children perform 5000 warmup callbacks and 2000000 workload callbacks; counters include startup, setup and cleanup.',
        metadata=metadata,samples=samples,program_layouts=layouts,heap_estimates=heaps,median_ns_per_iteration=medians,cpu_samples=cpu_samples,
        missing_metrics=['exact allocator live/peak heap','process peak RSS','sampled CPU call stacks','registration time','yielding host callbacks'])
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(medians,indent=2))


if __name__=='__main__':main()
