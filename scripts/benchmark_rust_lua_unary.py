"""Compare current/committed-parent expression execution with the pinned Lua CLI."""
from __future__ import annotations
import argparse
import ctypes
from ctypes import wintypes
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
    parser.add_argument('--parent-binary',required=True,type=Path)
    parser.add_argument('--parent-audit',required=True,type=Path)
    parser.add_argument('--parent-revision',required=True)
    parser.add_argument('--current-audit',required=True,type=Path)
    parser.add_argument('--family',choices=('unary','logical','predicate'),default='unary')
    args=parser.parse_args()
    if os.name!='nt':raise SystemExit('Windows process counters are required')
    root=Path(__file__).resolve().parents[1]
    sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
    parent=json.loads(args.parent_audit.read_text());current=json.loads(args.current_audit.read_text())
    assert not parent['rust_binary']['prebuilt']and not current['rust_binary']['prebuilt']
    parent_path=args.parent_audit.resolve().relative_to(root).as_posix()
    committed=subprocess.check_output(['git','show',f'{args.parent_revision}:{parent_path}'],cwd=root)
    assert committed==args.parent_audit.read_bytes().replace(b'\r\n',b'\n'),'parent audit is not the committed evidence'
    binaries={'parent':args.parent_binary.resolve(),'rust':Path(current['rust_binary']['path']),
              'reference':root/'target/lua-cli-reference/lua.exe'}
    expected_hashes={'parent':parent['rust_binary']['sha256'],'rust':current['rust_binary']['sha256'],
                     'reference':current['reference']['binary_sha256']}
    assert all(sha(p)==expected_hashes[engine]for engine,p in binaries.items())
    sources=list((root/'crates/deadlib-lua').rglob('*.rs'))+[Path(__file__).resolve()]
    sources += [root/n for n in ('Cargo.toml','Cargo.lock','.cargo/config.toml','crates/deadlib-lua/Cargo.toml','scripts/check_rust_lua_cli.py','scripts/build_lua_cli_reference.py')]
    hashes=lambda:{p.relative_to(root).as_posix():sha(p)for p in sorted(sources)}
    before=hashes()
    assert all(sha(root/n)==h for n,h in current['source_hashes'].items())
    count=5_000_000
    fixtures={
        'negate_local':(f"local x=3;local n=0;for i=1,{count} do local y=-x;n=n+y;x=-x end;io.write(n,':',x)",b'0:3'),
        'not_local':(f"local flag=false;local n=0;for i=1,{count} do local value=not flag;flag=value;if value then n=n+1 end end;io.write(n)",b'2500000'),
        'length_local':(f"local text='abcdef';local n=0;for i=1,{count} do local value=#text;n=n+value end;io.write(n)",b'30000000'),
        'not_comparison':(f"local n=0;for i=1,{count} do local value=not(i<2500001);if value then n=n+1 end end;io.write(n)",b'2500000'),
        'not_constant':(f"local n=0;for i=1,{count} do local value=not false;if value then n=n+1 end end;io.write(n)",b'5000000'),
    }
    if args.family=='logical':
        fixtures={
            'or_shortcut_true':(f"local a,b=true,false;local n=0;for i=1,{count} do local value=not not(a or b);if value then n=n+1 end end;io.write(n)",b'5000000'),
            'or_rhs_false':(f"local a,b=false,false;local n=0;for i=1,{count} do local value=not not(a or b);if value then n=n+1 end end;io.write(n)",b'0'),
            'and_shortcut_false':(f"local a,b=false,true;local n=0;for i=1,{count} do local value=not not(a and b);if value then n=n+1 end end;io.write(n)",b'0'),
            'nested_mixed':(f"local a,b,c=false,true,false;local n=0;for i=1,{count} do local value=not not((a and b)or c);if value then n=n+1 end end;io.write(n)",b'0'),
            'comparison_rhs':(f"local a,b,c=false,2,3;local n=0;for i=1,{count} do local value=not(a or(b<c));if value then n=n+1 end end;io.write(n)",b'0'),
        }
    if args.family=='predicate':
        fixtures={
            'if_local':(f"local a=true;local n=0;for i=1,{count} do if a then n=n+1 end end;io.write(n)",b'5000000'),
            'if_comparison':(f"local n=0;for i=1,{count} do if i<2500001 then n=n+1 end end;io.write(n)",b'2500000'),
            'if_logical':(f"local a,b,c=false,true,false;local n=0;for i=1,{count} do if not((a and b)or c)then n=n+1 end end;io.write(n)",b'5000000'),
            'while_comparison':(f"local n=0;while n<{count} do n=n+1 end;io.write(n)",b'5000000'),
            'repeat_logical':(f"local n=0;repeat n=n+1 until n>={count} or false;io.write(n)",b'5000000'),
        }
    kernel32=ctypes.WinDLL('kernel32',use_last_error=True)
    get_times=kernel32.GetProcessTimes;get_times.argtypes=[wintypes.HANDLE]+[ctypes.POINTER(wintypes.FILETIME)]*4;get_times.restype=wintypes.BOOL
    get_cycles=kernel32.QueryProcessCycleTime;get_cycles.argtypes=[wintypes.HANDLE,ctypes.POINTER(ctypes.c_ulonglong)];get_cycles.restype=wintypes.BOOL
    ticks=lambda v:(v.dwHighDateTime<<32)|v.dwLowDateTime
    environment=os.environ.copy()
    for key in ('LUA_INIT','LUA_PATH','LUA_CPATH'):environment.pop(key,None)
    samples=[]
    for name,(source,expected)in fixtures.items():
        for binary in binaries.values():
            run=subprocess.run(['lua','-e',source],executable=binary,cwd=root,env=environment,capture_output=True,timeout=30,creationflags=subprocess.CREATE_NO_WINDOW)
            assert run.returncode==0 and run.stdout==expected and not run.stderr,(name,run.returncode,run.stdout,run.stderr)
        engines=list(binaries)
        for sample in range(9):
            order=engines[sample%3:]+engines[:sample%3]
            if(sample//3)%2:order=list(reversed(order))
            for position,engine in enumerate(order):
                begin=time.perf_counter_ns()
                child=subprocess.Popen(['lua','-e',source],executable=binaries[engine],cwd=root,env=environment,
                    stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
                try:stdout,stderr=child.communicate(timeout=30)
                except subprocess.TimeoutExpired:child.kill();child.communicate();raise
                wall=time.perf_counter_ns()-begin
                assert child.returncode==0 and stdout==expected and not stderr,(name,engine,child.returncode,stdout,stderr)
                created,exited,kernel,user=[wintypes.FILETIME()for _ in range(4)]
                handle=wintypes.HANDLE(int(child._handle))
                if not get_times(handle,ctypes.byref(created),ctypes.byref(exited),ctypes.byref(kernel),ctypes.byref(user)):raise ctypes.WinError(ctypes.get_last_error())
                cycles=ctypes.c_ulonglong()
                if not get_cycles(handle,ctypes.byref(cycles)):raise ctypes.WinError(ctypes.get_last_error())
                samples.append(dict(workload=name,engine=engine,sample=sample,position=position,iterations=count,wall_ns=wall,
                    kernel_100ns=ticks(kernel),user_100ns=ticks(user),cpu_ns=(ticks(kernel)+ticks(user))*100,cpu_cycles=cycles.value))
    assert hashes()==before
    assert all(sha(p)==expected_hashes[engine]for engine,p in binaries.items())
    ratios={}
    for name in fixtures:
        pairs={engine:{x['sample']:x for x in samples if x['workload']==name and x['engine']==engine}for engine in binaries}
        assert all(set(v)==set(range(9))for v in pairs.values())
        ratios[name]={metric:{f'rust_over_{engine}':statistics.median(pairs['rust'][i][metric]/pairs[engine][i][metric]for i in range(9))
                             for engine in ('parent','reference')}for metric in ('wall_ns','cpu_ns','cpu_cycles')}
    result=dict(utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),platform=platform.platform(),
        processor=os.environ.get('PROCESSOR_IDENTIFIER',platform.processor()),parent_revision=args.parent_revision,
        parent_audit=dict(path=parent_path,sha256=sha(args.parent_audit)),
        current_audit=dict(path=args.current_audit.resolve().relative_to(root).as_posix(),sha256=sha(args.current_audit)),
        binaries={engine:dict(path=str(p),sha256=expected_hashes[engine])for engine,p in binaries.items()},
        source_hashes=before,reference_source_hashes=current['reference']['source_hashes'],
        protocol='Five fixed Lua sources, 5,000,000 loop iterations per fresh CLI process; nine rotated/reversed triples per fixture, plus one untimed checksum-validated warmup per engine/fixture. Exact stdout, empty stderr and exit status are checked in every child. Wall, GetProcessTimes user/kernel CPU and QueryProcessCycleTime include process launch, compile, startup, execution and cleanup. This compares the ordinary current release CLI with a byte-identical preserved parent CLI pinned by its committed audit and the official C reference pinned by the current audit. No allocator instrumentation or isolated opcode timings; shared host, warm filesystem caches and timer quantization limit small differences.',
        fixtures={name:dict(source=source,source_sha256=hashlib.sha256(source.encode()).hexdigest(),expected_stdout_hex=expected.hex())for name,(source,expected)in fixtures.items()},
        family=args.family,samples=samples,median_paired_ratios=ratios,
        missing_metrics=['allocation counts for these focused CLI fixtures','sampled CPU stacks','physical live/peak allocator heap','peak RSS'])
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(ratios,indent=2))


if __name__=='__main__':main()
