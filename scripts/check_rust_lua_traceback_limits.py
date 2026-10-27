"""Run potentially huge Linux traceback requests in bounded private processes."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import resource
import shutil
import signal
import subprocess
import sys
import time
from build_lua_cli_reference import build
from check_rust_lua_full_suite import i686_abi


def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--run-root',type=Path,required=True)
    parser.add_argument('--target',choices=['i686-unknown-linux-gnu'])
    parser.add_argument('--reference-fp-model',choices=['sse2'])
    parser.add_argument('--timeout',type=float,default=2)
    parser.add_argument('--address-space-mib',type=int,default=256)
    args=parser.parse_args()
    if sys.platform!='linux':parser.error('this resource audit requires Linux')
    if args.timeout<=0 or args.address_space_mib<64:parser.error('positive timeout and at least 64 MiB required')
    if args.reference_fp_model and not args.target:parser.error('SSE2 requires the explicit i686 target')
    root=Path(__file__).resolve().parents[1]
    files=list((root/'crates/deadlib-lua').rglob('*.rs'))+[root/'crates/deadlib-lua/Cargo.toml',Path(__file__).resolve(),root/'scripts/build_lua_cli_reference.py',root/'scripts/check_rust_lua_full_suite.py']
    def hashes():return {p.relative_to(root).as_posix():sha(p)for p in sorted(files)}
    before=hashes();target=Path(os.environ.get('CARGO_TARGET_DIR',str(root/'target'))).resolve()
    command=['cargo','build','--release','-p','deadlib-lua','--bin','deadlua']
    if args.target:command+=['--target',args.target]
    subprocess.run(command,cwd=root,check=True)
    rust=(target/args.target if args.target else target)/'release/deadlua'
    reference,metadata=build(root,args.target,args.reference_fp_model)
    runroot=args.run_root.resolve()/str(time.time_ns());runroot.mkdir(parents=True)
    executables={}
    for engine,binary in [('reference',reference),('rust',rust)]:
        path=runroot/engine/'lua';path.parent.mkdir();shutil.copyfile(binary,path);path.chmod(0o755)
        executables[engine]=dict(path=str(path),sha256=sha(path))
    abi={e:i686_abi(Path(v['path']))for e,v in executables.items()}if args.target else None
    def limits():
        resource.setrlimit(resource.RLIMIT_CORE,(0,0))
        cap=args.address_space_mib*1024*1024
        resource.setrlimit(resource.RLIMIT_AS,(cap,cap))
    rows=[]
    for index,expression in enumerate(['0','-3','-2147483648','1/0','-1/0','0/0','2^32+1.9']):
        source="local ok,value=pcall(debug.traceback,'trace',"+expression+");if ok then print(value)else print('error',type(value),value)end\n"
        row=dict(level=expression,source=source,source_sha256=hashlib.sha256(source.encode()).hexdigest())
        for engine,executable in executables.items():
            cwd=runroot/engine/str(index);cwd.mkdir();path=cwd/'case.lua';path.write_bytes(source.encode())
            env={k:v for k,v in os.environ.items()if k.upper()not in ['LUA_INIT','LUA_PATH','LUA_CPATH']}
            proc=subprocess.Popen(['lua','case.lua'],executable=executable['path'],cwd=cwd,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True,preexec_fn=limits)
            expired=False
            try:stdout,stderr=proc.communicate(timeout=args.timeout)
            except subprocess.TimeoutExpired:
                expired=True;os.killpg(proc.pid,signal.SIGKILL);stdout,stderr=proc.communicate(timeout=10)
            row[engine]=dict(returncode=proc.returncode,timeout=expired,stdout_hex=stdout.hex(),stderr_hex=stderr.hex())
            assert sha(path)==row['source_sha256']
        row['equal']=row['reference']==row['rust']and row['reference']['returncode']==0 and not row['reference']['timeout']
        rows.append(row);print(expression,'pass'if row['equal']else'mismatch/incomplete',flush=True)
    assert hashes()==before,'sources changed during traceback resource audit'
    result=dict(source_hashes=before,target=args.target,reference_fp_model=args.reference_fp_model,reference=metadata,executables=executables,abi=abi,timeout_seconds=args.timeout,address_space_mib=args.address_space_mib,cases=rows,passed=sum(r['equal']for r in rows),limitations='Bounded resource diagnostics. Native signals, memory failures and timeouts remain raw outcomes; equal timeouts do not count as passes. No process limits are imposed on ordinary engine use or on the official suite.')
    args.output.resolve().parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(result,indent=2)+'\n')
    if result['passed']!=len(rows):raise SystemExit(1)


if __name__=='__main__':main()
