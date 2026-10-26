"""Measure direct Rust callbacks against the development Lua 5.1 C bridge."""
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
    reference=[p for p in (root/'vendor/lua-src/lua-5.1.5').iterdir()if p.suffix in ('.c','.h')]
    def hashes(paths):return {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest()for p in sorted(paths)}
    before=hashes(sources);oracle=hashes(reference);started=time.time()
    command=['cargo','bench','-p','deadlib-lua','--bench','host_functions','--no-run','--message-format=json']
    build=subprocess.run(command,cwd=root,capture_output=True,text=True)
    output.with_suffix('.build.log').write_text(build.stderr,encoding='utf-8');output.with_suffix('.build.jsonl').write_text(build.stdout,encoding='utf-8');build.check_returncode()
    artifacts=[json.loads(line)for line in build.stdout.splitlines()if line.startswith('{')]
    binaries=[Path(row['executable'])for row in artifacts if row.get('reason')=='compiler-artifact'and row['target']['name']=='host_functions'and row.get('executable')]
    assert len(binaries)==1,binaries
    binary=binaries[0];binary_hash=hashlib.sha256(binary.read_bytes()).hexdigest()
    env=os.environ.copy();env.pop('DEADLIB_LUA_HOST_CPU',None)
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
        for sample in range(5):
            for engine in (('rust','reference')if sample%2==0 else('reference','rust')):
                child_env=env.copy();child_env['DEADLIB_LUA_HOST_CPU']=engine;begin=time.perf_counter_ns()
                child=subprocess.Popen([str(binary)],cwd=root,env=child_env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
                try:stdout,stderr=child.communicate(timeout=120)
                except subprocess.TimeoutExpired:child.kill();child.communicate();raise
                wall=time.perf_counter_ns()-begin
                assert child.returncode==0 and not stdout and not stderr,(engine,child.returncode,stdout,stderr)
                created,exited,kernel,user=[wintypes.FILETIME()for _ in range(4)]
                if not get_times(wintypes.HANDLE(int(child._handle)),ctypes.byref(created),ctypes.byref(exited),ctypes.byref(kernel),ctypes.byref(user)):raise ctypes.WinError(ctypes.get_last_error())
                cycles=ctypes.c_ulonglong()
                if not get_cycles(wintypes.HANDLE(int(child._handle)),ctypes.byref(cycles)):raise ctypes.WinError(ctypes.get_last_error())
                cpu_samples.append(dict(engine=engine,sample=sample,fixture='add_lua_loop',executions=400,callbacks=2_000_000,warmup_callbacks=5000,wall_ns=wall,
                    kernel_100ns=ticks(kernel),user_100ns=ticks(user),cpu_ns=(ticks(kernel)+ticks(user))*100,cpu_cycles=cycles.value))
    assert hashes(sources)==before and hashes(reference)==oracle
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
            _,name,live,reserved,natives,reference_live=fields
            heaps[name]=dict(rust_live_bytes=int(live),rust_reserved_bytes=int(reserved),rust_native_closures=int(natives),reference_lua_bytes=int(reference_live))
        elif fields[0]=='meta':
            _,name,value=fields;metadata[name]=value
    ratios={}
    for name in {x['workload']for x in samples}:
        pairs={engine:{x['sample']:x for x in samples if x['workload']==name and x['engine']==engine}for engine in ('rust','reference')}
        assert all(set(rows)==set(range(9))for rows in pairs.values())
        assert all(pairs['rust'][i]['repeats']==pairs['reference'][i]['repeats']and pairs['rust'][i]['callbacks_per_iteration']==pairs['reference'][i]['callbacks_per_iteration']for i in range(9))
        ratios[name]=statistics.median(pairs['rust'][i]['ns_per_iteration']/pairs['reference'][i]['ns_per_iteration']for i in range(9))
    assert len(samples)==72 and len(layouts)==len(heaps)==3
    assert all(x['allocations_total']==x['reallocations_total']==x['requested_bytes_total']==0 for x in samples if x['engine']=='rust')
    result=dict(command=command,binary=str(binary),binary_sha256=binary_hash,source_hashes=before,reference_source_hashes=oracle,
        utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),build_and_run_seconds=time.time()-started,platform=platform.platform(),processor=os.environ.get('PROCESSOR_IDENTIFIER',platform.processor()),
        rustc=subprocess.check_output(['rustc','-vV'],text=True),git_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),
        protocol='Nine alternating pairs for three 5000-callback Lua loops and one direct Rust entry. Timed Rust callbacks use borrowed arguments, existing result registers and function-pointer dispatch; reference callbacks use the development mlua bridge to official C Lua 5.1.5. Loop timings include interpreter execution and both callback boundaries, not an isolated function-pointer cost. Typed reference callbacks perform equivalent arithmetic/results; reference captured state is a Rust closure rather than Lua upvalues. Three warmups, setup, correctness, layouts and heap observations are outside timing. All warmed Rust paths assert zero Rust System allocations/reallocations/requests. Reference counters cover Rust bridge allocations only, excluding native C allocation calls. CPU children include VM/setup/startup/cleanup, 5000 warmup callbacks and 2000000 workload callbacks; counter increments are disabled but the forwarding allocator remains.',
        metadata=metadata,samples=samples,program_layouts=layouts,heap_estimates=heaps,median_paired_rust_over_reference=ratios,cpu_samples=cpu_samples,
        missing_metrics=['exact allocator live/peak heap','process peak RSS','native C allocation counts','sampled CPU call stacks','registration time','reentrant or yielding host callbacks'])
    output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(ratios,indent=2))


if __name__=='__main__':main()
