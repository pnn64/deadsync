"""Development-only MSVC C API/error-boundary proof against official Lua 5.1.

This does not enable production package.loadlib or provide a complete C API.
It tests persistent per-thread C states, a small API subset and C-only error jumps while
the Rust engine uses panic=abort. No C Lua implementation executes Rust cases.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
from build_lua_cli_reference import build

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--runner',type=Path,help='Prebuilt development example; source provenance is not established by this option')
    args=parser.parse_args()
    if os.name!='nt':raise SystemExit('this boundary proof requires Windows/MSVC')
    root=Path(__file__).resolve().parents[1]
    fixture=root/'crates/deadlib-lua/tests/native-boundary'
    files=list((root/'crates/deadlib-lua').rglob('*.rs'))+list(fixture.iterdir())+[root/'crates/deadlib-lua/Cargo.toml',Path(__file__).resolve(),root/'scripts/build_lua_cli_reference.py']
    sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
    hashes=lambda:{p.relative_to(root).as_posix():sha(p)for p in sorted(files)if p.is_file()}
    before=hashes()
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(root/'target')))
    if not target.is_absolute():target=root/target
    target=target.resolve()
    if args.runner is None:
        subprocess.run(['cargo','build','--release','-p','deadlib-lua','--example','native_boundary'],cwd=root,check=True)
    runner=args.runner.resolve()if args.runner else target/'release/examples/native_boundary.exe'
    _,metadata=build(root)
    source=root/'target/lua-cli-reference/lua-5.1.5/src'
    directory=root/'target'/f'lua-native-boundary-fixtures-{time.time_ns()}'
    directory.mkdir()
    (directory/'reference').mkdir();(directory/'rust').mkdir()
    vswhere=Path(os.environ.get('ProgramFiles(x86)',r'C:\Program Files (x86)'))/'Microsoft Visual Studio/Installer/vswhere.exe'
    setup=subprocess.check_output([str(vswhere),'-latest','-products','*','-find','VC/Auxiliary/Build/vcvars64.bat'],text=True).strip().splitlines()[0]
    commands=[]
    def compile_native(name,folder,arguments,dll=True):
        work=directory/folder
        command=['cl','/nologo','/O2','/MT',*(['/LD']if dll else []),'/D_CRT_SECURE_NO_WARNINGS',*map(str,arguments),'/link','/OUT:'+str(work/(name+('.dll'if dll else'.exe'))),*(['/IMPLIB:'+str(work/(name+'.lib'))]if dll else [])]
        response=work/(name+'.rsp');response.write_text(' '.join('"'+a+'"'for a in command[1:])+'\n')
        script=work/(name+'.cmd');script.write_text('@call "'+setup+'" >nul\n@cl @"'+str(response)+'"\n@exit /b %errorlevel%\n')
        run=subprocess.run(['cmd','/d','/c',str(script)],cwd=work,capture_output=True)
        (work/(name+'.log')).write_bytes(run.stdout+run.stderr)
        assert run.returncode==0,(name,(run.stdout+run.stderr).decode(errors='replace'))
        commands.append(dict(command=command,log=str(work/(name+'.log'))))
    official=[p for p in sorted(source.glob('*.c'))if p.name not in ['lua.c','luac.c','print.c']]
    compile_native('lua51','reference',['/DLUA_BUILD_AS_DLL','/I'+str(source),*official])
    # Both the CLI and module must use one Lua image. Passing a static CLI's
    # state into a second Lua DLL breaks internal singleton/dummy-node identity.
    compile_native('lua','reference',['/DLUA_BUILD_AS_DLL','/I'+str(source),source/'lua.c',directory/'reference/lua51.lib'],dll=False)
    reference=directory/'reference/lua.exe'
    metadata=dict(metadata,binary_path=str(reference),binary_sha256=sha(reference),command=commands[-1]['command'],link_mode='CLI and module share lua51.dll')
    compile_native('reference_module','reference',['/DREFERENCE','/I'+str(source),fixture/'module.c',directory/'reference/lua51.lib'])
    compile_native('boundary','rust',['/I'+str(fixture),fixture/'boundary.c'])
    compile_native('rust_module','rust',['/I'+str(fixture),fixture/'module.c',directory/'rust/boundary.lib'])
    names=['boundary_sum','boundary_identity','boundary_raise','boundary_binary','boundary_call','boundary_stack','boundary_mutate','boundary_state','boundary_state_reenter','boundary_cached_top','boundary_make_counter','boundary_snapshot','boundary_set_environment','boundary_set_globals','boundary_registry_roundtrip','boundary_make_many']
    boot=';'.join(f"{name}=assert(package.loadlib({json.dumps(str(directory/'reference/reference_module.dll'))},{json.dumps(name)}))"for name in names)
    runs={}
    for engine in ['reference','rust']:
        with tempfile.TemporaryDirectory(prefix='deadlua-native-boundary-')as temporary:
            work=Path(temporary);(work/'boundary.lua').write_bytes((fixture/'cases.lua').read_bytes())
            env=os.environ.copy()
            for key in ['LUA_INIT','LUA_PATH','LUA_CPATH']:env.pop(key,None)
            env['PATH']=str(directory/engine)+os.pathsep+env.get('PATH','')
            command=[str(reference),'-e',boot,'boundary.lua']if engine=='reference'else[str(runner),str(directory/'rust/boundary.dll'),str(directory/'rust/rust_module.dll'),str(work/'boundary.lua')]
            try:
                run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=30)
                runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            except subprocess.TimeoutExpired:runs[engine]=dict(command=command,timeout=True)
    assert hashes()==before,'sources changed during boundary proof'
    reference_run=runs['reference'];rust=runs['rust']
    matched=reference_run.get('returncode')==rust.get('returncode')==0 and reference_run.get('stdout_hex')==rust.get('stdout_hex')and not reference_run.get('stderr_hex')
    match=re.fullmatch(rb'BOUNDARY operations=(\d+) drops=(\d+) panic_abort=true\r?\nSTATES created=(\d+) collected=(\d+) destroyed=(\d+)\r?\n',bytes.fromhex(rust.get('stderr_hex','')))
    returned=bool(match and int(match[1])==int(match[2])and int(match[1])>1000)
    states=bool(match and int(match[3])==int(match[5]) and 0<int(match[4])<int(match[3]))
    artifacts=[reference,runner,*directory.rglob('*.dll')]
    result=dict(source_hashes=before,reference=metadata,runner=dict(path=str(runner),sha256=sha(runner),prebuilt=args.runner is not None),artifacts={str(p):sha(p)for p in artifacts},commands=commands,runs=runs,program_observations_equal=matched,every_dispatch_returned_before_jump=returned,persistent_state_lifetimes_balanced=states,limitations=__doc__+' The probe uses VM-owned stacks, persistent C states, unprotected calls, private owned function bindings, up to 255 actual C upvalues and Lua 5.1 pseudo-indexes. Cross-thread/inactive-state API access, complete C APIs, production library caching/unload, package.loadlib and require integration remain unfinished.')
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print('program observations equal:',matched,'dispatch/drop guards balanced:',returned,'state lifetimes balanced:',states,flush=True)
    if not matched or not returned or not states:raise SystemExit(1)

if __name__=='__main__':main()
