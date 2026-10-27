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
from check_rust_lua_upvalue_names import rewrite

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--runner',type=Path,help='Prebuilt development example; source provenance is not established by this option')
    args=parser.parse_args()
    if os.name!='nt':raise SystemExit('this boundary proof requires Windows/MSVC')
    root=Path(__file__).resolve().parents[1]
    fixture=root/'crates/deadlib-lua/tests/native-boundary'
    files=list((root/'crates/deadlib-lua').rglob('*.rs'))+list(fixture.iterdir())+[root/'crates/deadlib-lua/Cargo.toml',Path(__file__).resolve(),root/'scripts/build_lua_cli_reference.py',root/'scripts/check_rust_lua_upvalue_names.py']
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
    wire_source='local a,b=17,29;return function()return a,b end,function()return a,b end,function()return a,b end'
    wire_command=[str(reference),'-e',"local f=assert(loadstring("+json.dumps(wire_source)+",'=native upvalues'));local out=assert(io.open('original.luac','wb'));out:write(string.dump(f));assert(out:close())"]
    wire_env=os.environ.copy()
    for key in ['LUA_INIT','LUA_PATH','LUA_CPATH']:wire_env.pop(key,None)
    generated=subprocess.run(wire_command,cwd=directory/'reference',env=wire_env,capture_output=True,timeout=30)
    assert generated.returncode==0 and not generated.stdout and not generated.stderr
    original=(directory/'reference/original.luac').read_bytes()
    assert rewrite(original)[0]==original
    chunks={}
    for count in range(3):
        for style in ['original','empty','nul']:
            chunk,targets=rewrite(original,count,style)
            assert targets==3
            chunks[f'upvalues-{count}-{style}.luac']=chunk
    names=['boundary_sum','boundary_identity','boundary_raise','boundary_binary','boundary_call','boundary_stack','boundary_mutate','boundary_state','boundary_state_reenter','boundary_cached_top','boundary_make_counter','boundary_snapshot','boundary_set_environment','boundary_set_globals','boundary_registry_roundtrip','boundary_make_many','boundary_table','boundary_get','boundary_set','boundary_field','boundary_raw','boundary_metatable','boundary_count','boundary_shuffle','boundary_userdata','boundary_userdata_read','boundary_userdata_finalize','boundary_bytes','boundary_environment','boundary_capture_bytes','boundary_length','boundary_userdata_empty','boundary_protected','boundary_protected_negative','boundary_protected_argument','boundary_replace_handler','boundary_cpcall','boundary_scalar','boundary_scalar_null','boundary_scalar_capture','boundary_integers','boundary_compare','boundary_concat','boundary_concat_prefix','boundary_format','boundary_format_gc','boundary_auxcheck','boundary_auxwhere','boundary_auxerror','boundary_auxuserdata','boundary_auxget','boundary_auxnewmetatable','boundary_auxmetafield','boundary_auxopen','boundary_auxfind','boundary_auxrefs']
    names+=['boundary_auxbuffer_info','boundary_auxbuffer','boundary_auxgsub']
    names+=['boundary_upget','boundary_upset','boundary_upbind']
    names+=['boundary_gc','boundary_gc_counts']
    names+=['boundary_load','boundary_loadbuffer','boundary_loadtop']
    names+=['boundary_loadfile']
    boot=';'.join(f"{name}=assert(package.loadlib({json.dumps(str(directory/'reference/reference_module.dll'))},{json.dumps(name)}))"for name in names)
    runs={}
    # This independent probe deliberately retains a known compatibility gap.
    # lua_gettop can observe PUC's private parser table/prototype roots.
    scratch_source="do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end;collectgarbage();local trace={};assert(type(boundary_loadtop(function(i,n)trace[i]=n end))=='function');print('parser-scratch-slots',unpack(trace))"
    scratch_runs={}
    for engine in ['reference','rust']:
        with tempfile.TemporaryDirectory(prefix='deadlua-native-boundary-')as temporary:
            work=Path(temporary);(work/'boundary.lua').write_bytes((fixture/'cases.lua').read_bytes())
            for name,chunk in chunks.items():(work/name).write_bytes(chunk)
            env=os.environ.copy()
            for key in ['LUA_INIT','LUA_PATH','LUA_CPATH']:env.pop(key,None)
            env['PATH']=str(directory/engine)+os.pathsep+env.get('PATH','')
            command=[str(reference),'-e',boot,'boundary.lua']if engine=='reference'else[str(runner),str(directory/'rust/boundary.dll'),str(directory/'rust/rust_module.dll'),str(work/'boundary.lua')]
            try:
                run=subprocess.run(command,cwd=work,env=env,input=b'17 return 42\n',capture_output=True,timeout=30)
                runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            except subprocess.TimeoutExpired:runs[engine]=dict(command=command,timeout=True)
            (work/'scratch.lua').write_text(scratch_source,encoding='utf-8')
            command=command[:-1]+[str(work/'scratch.lua')]
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=30)
            scratch_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
    assert hashes()==before,'sources changed during boundary proof'
    reference_run=runs['reference'];rust=runs['rust']
    buffers={engine:bool(re.search(rb'(?m)^aux-buffer-complete\t99\t40\r*$',bytes.fromhex(run.get('stdout_hex',''))))for engine,run in runs.items()}
    upvalues={engine:b'native-upvalues-complete'in bytes.fromhex(run.get('stdout_hex',''))for engine,run in runs.items()}
    collectors={engine:b'native-gc-complete'in bytes.fromhex(run.get('stdout_hex',''))for engine,run in runs.items()}
    loaders={engine:b'native-load-complete'in bytes.fromhex(run.get('stdout_hex',''))for engine,run in runs.items()}
    file_loaders={engine:b'native-file-complete'in bytes.fromhex(run.get('stdout_hex',''))for engine,run in runs.items()}
    matched=reference_run.get('returncode')==rust.get('returncode')==0 and reference_run.get('stdout_hex')==rust.get('stdout_hex')and not reference_run.get('stderr_hex')and all(buffers.values())and all(upvalues.values())and all(collectors.values())and all(loaders.values())and all(file_loaders.values())
    match=re.fullmatch(rb'BOUNDARY operations=(\d+) drops=(\d+) panic_abort=true\r?\nSTATES created=(\d+) collected=(\d+) destroyed=(\d+)\r?\n',bytes.fromhex(rust.get('stderr_hex','')))
    returned=bool(match and int(match[1])==int(match[2])and int(match[1])>1000)
    states=bool(match and int(match[3])==int(match[5]) and 0<int(match[4])<int(match[3]))
    artifacts=[reference,runner,*directory.rglob('*.dll')]
    result=dict(source_hashes=before,reference=metadata,runner=dict(path=str(runner),sha256=sha(runner),prebuilt=args.runner is not None),artifacts={str(p):sha(p)for p in artifacts},commands=commands,runs=runs,program_observations_equal=matched,every_dispatch_returned_before_jump=returned,persistent_state_lifetimes_balanced=states,buffer_api_checks_complete=buffers,limitations=__doc__+' The probe uses VM-owned stacks, persistent C states, unprotected calls, private owned function bindings, up to 255 actual C upvalues and Lua 5.1 pseudo-indexes, aligned stable userdata payloads, environments and NUL-terminated string conversions, plus protected calls and error functions, scalar conversions, comparison and native-window concatenation, formatted messages, auxiliary checks/registration/references and auxiliary buffers/literal replacement. Cross-thread/inactive-state API access, complete C APIs, production library caching/unload, package.loadlib and require integration remain unfinished.')
    result.update(native_load_complete=loaders,native_gc_complete=collectors,native_upvalues_complete=upvalues,native_upvalue_fixture=dict(source=wire_source,command=wire_command,original_chunk_hex=original.hex(),chunks={name:dict(sha256=hashlib.sha256(chunk).hexdigest(),hex=chunk.hex())for name,chunk in chunks.items()}))
    scratch_complete=all(run['returncode']==0 and b'parser-scratch-slots'in bytes.fromhex(run['stdout_hex'])for run in scratch_runs.values())
    result['parser_scratch_probe']=dict(source=scratch_source,runs=scratch_runs,completed=scratch_complete,observations_equal=scratch_complete and scratch_runs['reference']['stdout_hex']==scratch_runs['rust']['stdout_hex'],full_native_loader_parity_proven=False)
    result['native_file_complete']=file_loaders
    result['stdin_hex']=b'17 return 42\n'.hex()
    result['limitations']+=' Native reader/buffer/string/file loading is a development subset: the independent parser_scratch_probe retains the observable lua_gettop mismatch from PUC parser roots. Allocator/OOM statuses, full native loader/C API parity and production loading remain unfinished.'
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print('program observations equal:',matched,'dispatch/drop guards balanced:',returned,'state lifetimes balanced:',states,flush=True)
    if not matched or not returned or not states or not scratch_complete:raise SystemExit(1)

if __name__=='__main__':main()
