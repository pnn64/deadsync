"""Retain standalone unpack results, native crashes and optional overflow diagnostics."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
from build_lua_cli_reference import build


def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def literal(text):return '"'+''.join('\\%03d'%b for b in text.encode())+'"'


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--run-root',type=Path,required=True)
    parser.add_argument('--diagnose-signed-overflow',action='store_true')
    args=parser.parse_args()
    if args.diagnose_signed_overflow and os.name=='nt':
        raise SystemExit('the diagnostic build requires a GNU-compatible C compiler')
    root=Path(__file__).resolve().parents[1]
    files=list((root/'crates/deadlib-lua').rglob('*.rs'))+[root/'crates/deadlib-lua/Cargo.toml',Path(__file__).resolve(),root/'scripts/build_lua_cli_reference.py']
    def hashes():return {p.relative_to(root).as_posix():sha(p)for p in sorted(files)}
    before=hashes()
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(root/'target'))).resolve()
    subprocess.run(['cargo','build','--release','-p','deadlib-lua','--bin','deadlua'],cwd=root,check=True)
    reference,metadata=build(root)
    runroot=args.run_root.resolve()/str(time.time_ns());runroot.mkdir(parents=True)
    executables={}
    for engine,binary in [('reference',reference),('rust',target/'release'/('deadlua.exe'if os.name=='nt'else'deadlua'))]:
        path=runroot/engine/('lua.exe'if os.name=='nt'else'lua');path.parent.mkdir()
        shutil.copyfile(binary,path)
        if os.name!='nt':path.chmod(0o755)
        executables[engine]=dict(path=str(path),sha256=sha(path))
    diagnostic=None
    wrapping_control=None
    if args.diagnose_signed_overflow:
        output=runroot/'lua-signed-overflow'
        command=metadata['command'][:]
        command[command.index('-o')+1]=str(output)
        command[1:1]=['-g','-fsanitize=signed-integer-overflow','-fno-sanitize-recover=signed-integer-overflow']
        result=subprocess.run(command,cwd=root,capture_output=True)
        (runroot/'diagnostic-build.log').write_bytes(result.stdout+result.stderr)
        if result.returncode:raise SystemExit('overflow diagnostic build failed')
        diagnostic=dict(command=command,path=str(output),sha256=sha(output),purpose='Instrumented diagnostic only; the strict reference build is unchanged.')
        output=runroot/'lua-wrapping-control'
        command=metadata['command'][:]
        command[command.index('-o')+1]=str(output)
        command.insert(1,'-fwrapv')
        result=subprocess.run(command,cwd=root,capture_output=True)
        (runroot/'wrapping-build.log').write_bytes(result.stdout+result.stderr)
        if result.returncode:raise SystemExit('wrapping diagnostic build failed')
        wrapping_control=dict(command=command,path=str(output),sha256=sha(output),purpose='Supplemental signed-wrapping control; excluded from the strict comparison gate.')
    cases=[]
    for count in range(7997,8002):
        source=f"local t={{}};for i=1,{count} do t[i]=i end;local function summarize(...)return select('#',...),select(1,...),select(-1,...)end;return pcall(function()return summarize(unpack(t))end)"
        cases.append((str(count),source))
    for operands in ['{},3,1','{},0/0,0/0','{},-2147483648,2147483647','{},0,2147483647',
                     '{},-1,2147483647','{},-2147483648,-1','{},-2147483648,0',
                     '{},2147483647,-2147483648','{},nil,false','false,false,false','{},false,false']:
        cases.append((operands,'return pcall(function()return unpack('+operands+')end)'))
    def disable_core():
        import resource
        resource.setrlimit(resource.RLIMIT_CORE,(0,0))
    rows=[]
    for index,(name,source)in enumerate(cases):
        program="local f=assert(loadstring("+literal(source)+",'=(unpack-probe)'));print(f())\n"
        row=dict(name=name,source=source,program=program,program_sha256=hashlib.sha256(program.encode()).hexdigest())
        engines=dict(executables)
        if diagnostic:engines['diagnostic']=diagnostic
        if wrapping_control:engines['wrapping_control']=wrapping_control
        for engine,executable in engines.items():
            cwd=runroot/engine/str(index);cwd.mkdir(parents=True)
            path=cwd/'case.lua';path.write_bytes(program.encode())
            environment={k:v for k,v in os.environ.items()if k not in ['LUA_INIT','LUA_PATH','LUA_CPATH']}
            options=dict(executable=executable['path'],cwd=cwd,env=environment,capture_output=True,timeout=5)
            if os.name!='nt':options['preexec_fn']=disable_core
            try:
                result=subprocess.run(['lua','case.lua'],**options)
                row[engine]=dict(returncode=result.returncode,stdout_hex=result.stdout.hex(),stderr_hex=result.stderr.hex())
            except subprocess.TimeoutExpired as error:
                row[engine]=dict(timeout=True,stdout_hex=(error.stdout or b'').hex(),stderr_hex=(error.stderr or b'').hex())
            assert sha(path)==row['program_sha256']
        row['equal']=row['reference']==row['rust']and row['reference'].get('returncode')==0
        rows.append(row)
        print(name,'pass'if row['equal']else'mismatch',flush=True)
    assert hashes()==before,'sources changed during unpack audit'
    reference_source=reference.parent/'lua-5.1.5/src'
    assert {name:sha(reference_source/name)for name in metadata['source_hashes']}==metadata['source_hashes']
    report=dict(source_hashes=before,reference=metadata,executables=executables,diagnostic=diagnostic,wrapping_control=wrapping_control,cases=rows,
                passed=sum(row['equal']for row in rows),
                limitations='Strict output/status comparisons retain native crashes and timeouts. The optional sanitizer executable is a separate diagnostic and does not replace or modify the default reference. This audit does not establish full standalone Lua 5.1 parity.')
    args.output.resolve().parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,indent=2)+'\n')
    if report['passed']!=len(rows):raise SystemExit(1)


if __name__=='__main__':main()
