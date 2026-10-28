"""MSVC runtime C API/error-boundary proof against official Lua 5.1.

The default runtime adapter loads modules; complete C API semantics remain open.
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
import struct
import subprocess
import tempfile
import shutil
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
    files=[p for p in (root/'crates/deadlib-lua').rglob('*')if p.suffix in {'.rs','.c','.h'}]+list(fixture.iterdir())+[root/'crates/deadlib-lua/Cargo.toml',Path(__file__).resolve(),root/'scripts/build_lua_cli_reference.py',root/'scripts/check_rust_lua_upvalue_names.py']
    sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
    hashes=lambda:{p.relative_to(root).as_posix():sha(p)for p in sorted(files)if p.is_file()}
    before=hashes()
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(root/'target')))
    if not target.is_absolute():target=root/target
    target=target.resolve()
    if args.runner is None:
        built=subprocess.run(['cargo','build','--release','-p','deadlib-lua','--example','native_boundary','--message-format=json'],cwd=root,capture_output=True,check=True)
        messages=[json.loads(line)for line in built.stdout.splitlines()if line.startswith(b'{')]
        sdk=Path(next(m['out_dir']for m in messages if m.get('reason')=='build-script-executed'and'/deadlib-lua#'in m.get('package_id','').replace('\\','/')))
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
    if args.runner is not None:
        compile_native('boundary','rust',['/I'+str(fixture),fixture/'boundary.c'])
        module_library=directory/'rust/boundary.lib'
    else:module_library=sdk/'lua51.lib'
    compile_native('rust_module','rust',['/I'+str(fixture),fixture/'module.c',module_library])
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
    # Valid unused constants exercise every writer tag without changing code.
    order='little'if original[6]else'big';width=original[8]
    position=12
    length=int.from_bytes(original[position:position+width],order);position+=width+length+12
    count=int.from_bytes(original[position:position+4],order);position+=4+count*4
    constant_count_position=position
    count=int.from_bytes(original[position:position+4],order);position+=4
    assert count==2
    for _ in range(count):assert original[position]==3;position+=9
    binary_text=b'a\0\xff'
    extra=b'\0\1\1\1\0\4'+(len(binary_text)+1).to_bytes(width,order)+binary_text+b'\0'+b'\3'+struct.pack('=d',-0.0)+b'\3'+int(0x7ff8000000000042).to_bytes(8,order)
    extra_chunks={'dump-constants.luac':original[:constant_count_position]+(count+6).to_bytes(4,order)+original[constant_count_position+4:position]+extra+original[position:],'dump-stripped.luac':rewrite(original,0,'original',True)[0]}
    root_wire_source='return function(a)local x="blob";return function(b,...)return function()return a,b,x end end end'
    root_wire_path=directory/'reader-roots-nested.luac'
    root_wire_command=[str(reference),'-e',f'local f=assert(loadstring({json.dumps(root_wire_source)}));local o=assert(io.open({json.dumps(str(root_wire_path))},"wb"));o:write(string.dump(f));o:close()']
    subprocess.run(root_wire_command,check=True,capture_output=True)
    root_chunks={'reader-roots-nested.luac':root_wire_path.read_bytes(),'reader-roots-stripped.luac':extra_chunks['dump-stripped.luac']}
    names=['boundary_sum','boundary_identity','boundary_raise','boundary_binary','boundary_call','boundary_stack','boundary_mutate','boundary_state','boundary_state_reenter','boundary_cached_top','boundary_make_counter','boundary_snapshot','boundary_set_environment','boundary_set_globals','boundary_registry_roundtrip','boundary_make_many','boundary_table','boundary_get','boundary_set','boundary_field','boundary_raw','boundary_metatable','boundary_count','boundary_shuffle','boundary_userdata','boundary_userdata_read','boundary_userdata_finalize','boundary_bytes','boundary_environment','boundary_capture_bytes','boundary_length','boundary_userdata_empty','boundary_protected','boundary_protected_negative','boundary_protected_argument','boundary_replace_handler','boundary_cpcall','boundary_scalar','boundary_scalar_null','boundary_scalar_capture','boundary_integers','boundary_compare','boundary_concat','boundary_concat_prefix','boundary_format','boundary_format_gc','boundary_auxcheck','boundary_auxwhere','boundary_auxerror','boundary_auxuserdata','boundary_auxget','boundary_auxnewmetatable','boundary_auxmetafield','boundary_auxopen','boundary_auxfind','boundary_auxrefs']
    names+=['boundary_auxbuffer_info','boundary_auxbuffer','boundary_auxgsub']
    names+=['boundary_upget','boundary_upset','boundary_upbind']
    names+=['boundary_gc','boundary_gc_counts']
    names+=['boundary_load','boundary_loadbuffer','boundary_loadtop','boundary_loadroots']
    names+=['boundary_loadfile']
    names+=['boundary_dump','boundary_dump_null']
    names+=['boundary_thread','boundary_thread_status','boundary_thread_pointer','boundary_thread_capture']
    names+=['boundary_pointer']
    names+=['boundary_resume','boundary_yield']
    names+=['boundary_level','boundary_setlevel','boundary_level_reader']
    names+=['boundary_debug_info','boundary_debug_local','boundary_debug_hold']
    names+=['boundary_hook_set','boundary_hook_info','boundary_hook_restore']
    names+=['boundary_context_make','boundary_context_read','boundary_context_write','boundary_context_install']
    names+=['boundary_return_stack']
    names+=['boundary_body_call','boundary_body_copy','boundary_body_info']
    names+=['boundary_body_hook']
    names+=['boundary_library_handle']
    names+=['boundary_inactive_objects']
    names+=['boundary_thread_pcall','boundary_thread_cpcall','boundary_thread_yield_tail','boundary_thread_called','boundary_thread_load']
    names+=['boundary_panic_swap','boundary_panic_raise','boundary_panic_protected_error','boundary_panic_yield_tail','boundary_panic_unhandled']
    names+=['boundary_thread_file','boundary_thread_file_panic']
    names+=['boundary_allocation_gc','boundary_allocation_yield_tail']
    names+=['boundary_execution_construct','boundary_execution_format','boundary_execution_concat','boundary_execution_concat_panic','boundary_execution_yield_tail','boundary_execution_called']
    names+=['boundary_concat_offsets']
    names+=['boundary_gc_thread','boundary_gc_thread_panic']
    names+=['boundary_waiting_raise','boundary_waiting_snapshot','boundary_waiting_c_resume']
    names+=['boundary_unprotected_call','boundary_unprotected_panic','boundary_unprotected_fail']
    names+=['boundary_yield_call','boundary_yield_resume']
    names+=['boundary_table_thread','boundary_table_thread_panic','boundary_table_invoke']
    names+=['boundary_table_yield','boundary_table_c_yield_all','boundary_table_c_yield_tail','boundary_table_c_yield_zero']
    names+=['boundary_file_closer','boundary_file_capture','boundary_file_body','boundary_file_hook']
    names+=['boundary_newthread','boundary_thread_values','boundary_thread_push','boundary_thread_move','boundary_thread_primitives','boundary_thread_selfmove','boundary_thread_resize','boundary_thread_hold']
    boot=';'.join(f"{name}=assert(package.loadlib({json.dumps(str(directory/'reference/reference_module.dll'))},{json.dumps(name)}))"for name in names)
    runs={}
    # Keep the original regression independently from the broader root traces.
    scratch_source="do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end;collectgarbage();local trace={};assert(type(boundary_loadtop(function(i,n)trace[i]=n end))=='function');print('parser-scratch-slots',unpack(trace))"
    scratch_runs={}
    root_runs={}
    thread_stack_runs={}
    resume_runs={}
    level_runs={}
    reader_level_runs={}
    debug_runs={}
    hook_runs={}
    context_runs={}
    return_stack_runs={}
    protection_hook_runs={}
    library_stack_runs={}
    package_stack_runs={}
    body_stack_runs={}
    body_runs={}
    library_runs={}
    inactive_object_runs={}
    thread_call_runs={}
    api_yield_runs={}
    thread_load_runs={}
    reader_continuation_runs={}
    panic_runs={}
    panic_exit_runs={}
    thread_file_runs={}
    thread_file_stdin_runs={}
    thread_file_panic_runs={}
    thread_file_exit_runs={}
    allocation_gc_runs={}
    execution_runs={}
    execution_panic_runs={}
    concat_yield_runs={}
    thread_gc_runs={}
    thread_gc_panic_runs={}
    waiting_error_runs={}
    thread_unprotected_runs={}
    thread_unprotected_exit_runs={}
    call_yield_runs={}
    gc_integer_runs={}
    table_thread_runs={}
    table_yield_runs={}
    io_closer_runs={}
    for engine in ['reference','rust']:
        with tempfile.TemporaryDirectory(prefix='deadlua-native-boundary-')as temporary:
            work=Path(temporary);(work/'boundary.lua').write_bytes((fixture/'cases.lua').read_bytes())
            shutil.copyfile(directory/engine/('reference_module.dll'if engine=='reference'else'rust_module.dll'),work/'native.dll')
            (work/'invalid-native.dll').write_bytes(b'not a dynamic library')
            for name,chunk in chunks.items():(work/name).write_bytes(chunk)
            for name,chunk in extra_chunks.items():(work/name).write_bytes(chunk)
            for name,chunk in root_chunks.items():(work/name).write_bytes(chunk)
            env=os.environ.copy()
            for key in ['LUA_INIT','LUA_PATH','LUA_CPATH']:env.pop(key,None)
            env.pop('DEADLUA_NATIVE_BOUNDARY_INSTRUCTIONS',None)
            env['PATH']=str(directory/engine)+os.pathsep+env.get('PATH','')
            command=[str(reference),'-e',boot,'boundary.lua']if engine=='reference'else[str(runner),str(directory/'rust/boundary.dll')if args.runner is not None else'default',str(directory/'rust/rust_module.dll'),str(work/'boundary.lua')]
            try:
                run=subprocess.run(command,cwd=work,env=env,input=b'17 return 42\n',capture_output=True,timeout=30)
                runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            except subprocess.TimeoutExpired:runs[engine]=dict(command=command,timeout=True)
            (work/'scratch.lua').write_text(scratch_source,encoding='utf-8')
            command=command[:-1]+[str(work/'scratch.lua')]
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=30)
            scratch_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'reader-roots.lua').write_bytes((fixture/'reader-roots.lua').read_bytes())
            command=command[:-1]+[str(work/'reader-roots.lua')]
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            root_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'thread-stacks.lua').write_bytes((fixture/'thread-stacks.lua').read_bytes())
            command=command[:-1]+[str(work/'thread-stacks.lua')]
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            thread_stack_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'native-resume.lua').write_bytes((fixture/'native-resume.lua').read_bytes())
            command=command[:-1]+[str(work/'native-resume.lua')]
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            resume_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'native-levels.lua').write_bytes((fixture/'native-levels.lua').read_bytes())
            command=command[:-1]+[str(work/'native-levels.lua')]
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            level_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'reader-call-levels.lua').write_bytes((fixture/'reader-call-levels.lua').read_bytes())
            command=command[:-1]+[str(work/'reader-call-levels.lua')]
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            reader_level_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            # Use the same source name as the standalone Rust development runner.
            (work/'boundary.lua').write_bytes((fixture/'native-debug.lua').read_bytes())
            command=command[:-1]+['boundary.lua']
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            debug_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'native-hooks.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            hook_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'native-context.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            context_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'return-stacks.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            return_stack_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'protection-hooks.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            protection_hook_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'library-stacks.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            library_stack_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'package-stacks.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            package_stack_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            body_stack_runs[engine]={}
            (work/'boundary.lua').write_bytes((fixture/'inactive-objects.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            inactive_object_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'thread-calls.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            thread_call_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'api-yields.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            api_yield_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'thread-loads.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            thread_load_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'reader-continuations.lua').read_bytes()+b'\n'+(fixture/'reader-environments.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            reader_continuation_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'native-panic.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            panic_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            panic_exit_runs[engine]={}
            for mode,setup_panic in [('default',''),('absent','boundary_panic_swap(nil,0);'),('returning','boundary_panic_swap(nil,3);')]:
                source=setup_panic+"local co=coroutine.create(function()end);boundary_panic_unhandled(co,'native failure');error('panic returned')"
                (work/'boundary.lua').write_text(source,encoding='utf-8')
                run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
                panic_exit_runs[engine][mode]=dict(source=source,command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'thread-file-directory').mkdir()
            (work/'boundary.lua').write_bytes((fixture/'thread-files.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            thread_file_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'thread-file-stdin.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,input=b'7return marker,59\n',capture_output=True,timeout=60)
            thread_file_stdin_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'thread-file-panic.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            thread_file_panic_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            thread_file_exit_runs[engine]={}
            for mode,replacement in [('absent',0),('returning',3)]:
                source="local co=coroutine.create(function()end);local path='thread-file-late-exit.lua';local f=assert(io.open(path,'wb'));f:write('return 42');f:close();collectgarbage('stop');do local p=newproxy(true);getmetatable(p).__gc=function()assert(coroutine.running()==co);boundary_panic_swap(co,"+str(replacement)+");assert(os.remove(path));error('native failure',0)end end;boundary_panic_swap(co,1);boundary_thread_file_panic(co,'native failure',path);error('panic returned')"
                (work/'boundary.lua').write_text(source,encoding='utf-8')
                run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
                thread_file_exit_runs[engine][mode]=dict(source=source,command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'allocation-gc.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            allocation_gc_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'thread-execution.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            execution_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'thread-execution-panic.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            execution_panic_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'concat-yields.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            concat_yield_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'thread-gc.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            thread_gc_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'thread-gc-panic.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            thread_gc_panic_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'waiting-errors.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            waiting_error_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'thread-unprotected.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            thread_unprotected_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'call-yields.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            call_yield_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'gc-integers.lua').read_bytes())
            gc_integer_env=env.copy()
            if engine=='rust':gc_integer_env['DEADLUA_NATIVE_BOUNDARY_INSTRUCTIONS']='50000000'
            run=subprocess.run(command,cwd=work,env=gc_integer_env,capture_output=True,timeout=60)
            gc_integer_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex(),instruction_limit=50000000 if engine=='rust'else None)
            (work/'boundary.lua').write_bytes((fixture/'thread-tables.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            table_thread_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'table-yields.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            table_yield_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            (work/'boundary.lua').write_bytes((fixture/'io-closers.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            io_closer_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            thread_unprotected_exit_runs[engine]={}
            for kind in ['fresh','completed','yielded','failed','partial']:
                for native in [False,True]:
                    for mode in [0,3]:
                        key=f'{kind}-{native}-{mode}'
                        source=f'WAITING_KIND={json.dumps(kind)};WAITING_NATIVE={str(native).lower()};WAITING_PANIC={mode};\n'+(fixture/'thread-unprotected-exit.lua').read_text()
                        (work/'boundary.lua').write_text(source)
                        run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=30)
                        newline='\r\n'if os.name=='nt'else'\n'
                        expected=f'waiting-panic-exit\t{kind}\t{str(native).lower()}\t{mode}'+newline+(('panic-return\t2\t1'+newline)if mode==3 else'')
                        output_file=work/'waiting-exit-buffered-output.bin'
                        thread_unprotected_exit_runs[engine][key]=dict(command=command,source=source,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex(),expected_stdout_hex=expected.encode().hex(),buffered_file_hex=output_file.read_bytes().hex()if output_file.exists()else None)
            (work/'boundary.lua').write_bytes((fixture/'native-bodies.lua').read_bytes())
            run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
            body_runs[engine]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            for source_name in ['return-stacks.lua','library-stacks.lua','package-stacks.lua']:
                (work/'boundary.lua').write_bytes(b'BODY_COPY=true\n'+(fixture/source_name).read_bytes())
                run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
                body_stack_runs[engine][source_name]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
            library_runs[engine]={}
            for mode,prefix in [('ordinary',b''),('copied',b'BODY_COPY=true\n')]:
                (work/'boundary.lua').write_bytes(prefix+(fixture/'library-loading.lua').read_bytes())
                run=subprocess.run(command,cwd=work,env=env,capture_output=True,timeout=60)
                library_runs[engine][mode]=dict(command=command,returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
    assert hashes()==before,'sources changed during boundary proof'
    reference_run=runs['reference'];rust=runs['rust']
    buffers={engine:bool(re.search(rb'(?m)^aux-buffer-complete\t99\t40\r*$',bytes.fromhex(run.get('stdout_hex',''))))for engine,run in runs.items()}
    upvalues={engine:b'native-upvalues-complete'in bytes.fromhex(run.get('stdout_hex',''))for engine,run in runs.items()}
    collectors={engine:b'native-gc-complete'in bytes.fromhex(run.get('stdout_hex',''))for engine,run in runs.items()}
    loaders={engine:b'native-load-complete'in bytes.fromhex(run.get('stdout_hex',''))for engine,run in runs.items()}
    file_loaders={engine:b'native-file-complete'in bytes.fromhex(run.get('stdout_hex',''))for engine,run in runs.items()}
    dumpers={engine:b'native-dump-complete'in bytes.fromhex(run.get('stdout_hex',''))for engine,run in runs.items()}
    threads={engine:b'native-thread-complete'in bytes.fromhex(run.get('stdout_hex',''))for engine,run in runs.items()}
    pointers={engine:b'native-pointer-complete'in bytes.fromhex(run.get('stdout_hex',''))for engine,run in runs.items()}
    matched=reference_run.get('returncode')==rust.get('returncode')==0 and reference_run.get('stdout_hex')==rust.get('stdout_hex')and not reference_run.get('stderr_hex')and all(buffers.values())and all(upvalues.values())and all(collectors.values())and all(loaders.values())and all(file_loaders.values())and all(dumpers.values())and all(threads.values())and all(pointers.values())
    match=re.fullmatch(rb'BOUNDARY operations=(\d+) drops=(\d+) panic_abort=true\r?\nSTATES created=(\d+) collected=(\d+) destroyed=(\d+)\r?\n(?:LIBRARIES opened=(\d+) closed=(\d+)\r?\n)?',bytes.fromhex(rust.get('stderr_hex','')))
    returned=bool(match and int(match[1])==int(match[2])and int(match[1])>1000 and (match[6]is None or int(match[6])==int(match[7])>0))
    states=bool(match and int(match[3])==int(match[5]) and 0<int(match[4])<int(match[3]))
    artifacts=[reference,runner,*directory.rglob('*.dll')]
    result=dict(source_hashes=before,reference=metadata,runner=dict(path=str(runner),sha256=sha(runner),prebuilt=args.runner is not None),artifacts={str(p):sha(p)for p in artifacts},commands=commands,runs=runs,program_observations_equal=matched,every_dispatch_returned_before_jump=returned,persistent_state_lifetimes_balanced=states,buffer_api_checks_complete=buffers,limitations=__doc__+' The probe uses VM-owned stacks, persistent C states, unprotected calls, private owned function bindings, up to 255 actual C upvalues and Lua 5.1 pseudo-indexes, aligned stable userdata payloads, environments and NUL-terminated string conversions, plus protected calls and error functions, scalar conversions, comparison and native-window concatenation, formatted messages, auxiliary checks/registration/references and auxiliary buffers/literal replacement. Complete inactive-state execution, lifecycle/allocator APIs and broader native error/resource semantics remain unfinished.')
    if args.runner is None:result['runtime_c_abi']=dict(default_adapter=True,image_path=str(sdk/'lua51.dll'),image_sha256=sha(sdk/'lua51.dll'),import_library_path=str(module_library),import_library_sha256=sha(module_library),header_sha256=sha(root/'crates/deadlib-lua/src/native_abi/api.h'))
    result.update(native_load_complete=loaders,native_gc_complete=collectors,native_upvalues_complete=upvalues,native_upvalue_fixture=dict(source=wire_source,command=wire_command,original_chunk_hex=original.hex(),chunks={name:dict(sha256=hashlib.sha256(chunk).hexdigest(),hex=chunk.hex())for name,chunk in chunks.items()}))
    scratch_complete=all(run['returncode']==0 and b'parser-scratch-slots'in bytes.fromhex(run['stdout_hex'])for run in scratch_runs.values())
    result['parser_scratch_probe']=dict(source=scratch_source,runs=scratch_runs,completed=scratch_complete,observations_equal=scratch_complete and scratch_runs['reference']['stdout_hex']==scratch_runs['rust']['stdout_hex'],full_native_loader_parity_proven=False)
    roots_complete=all(run['returncode']==0 and b'native-reader-roots-complete'in bytes.fromhex(run['stdout_hex'])for run in root_runs.values())
    roots_equal=roots_complete and root_runs['reference']['stdout_hex']==root_runs['rust']['stdout_hex'] and not root_runs['reference']['stderr_hex']
    result['native_reader_roots_probe']=dict(runs=root_runs,completed=roots_complete,observations_equal=roots_equal,fixture='crates/deadlib-lua/tests/native-boundary/reader-roots.lua',binary_source=root_wire_source,binary_command=root_wire_command,chunks={name:dict(sha256=hashlib.sha256(chunk).hexdigest(),hex=chunk.hex())for name,chunk in root_chunks.items()})
    thread_stacks_complete=all(run['returncode']==0 and b'native-thread-stacks-complete\t1000'in bytes.fromhex(run['stdout_hex'])for run in thread_stack_runs.values())
    thread_stacks_equal=thread_stacks_complete and thread_stack_runs['reference']['stdout_hex']==thread_stack_runs['rust']['stdout_hex'] and not thread_stack_runs['reference']['stderr_hex']
    result['native_thread_stacks_probe']=dict(runs=thread_stack_runs,completed=thread_stacks_complete,observations_equal=thread_stacks_equal,fixture='crates/deadlib-lua/tests/native-boundary/thread-stacks.lua',complete_inactive_api_proven=False)
    resume_complete=all(run['returncode']==0 and b'native-resume-complete'in bytes.fromhex(run['stdout_hex'])for run in resume_runs.values())
    resume_equal=resume_complete and resume_runs['reference']['stdout_hex']==resume_runs['rust']['stdout_hex'] and not resume_runs['reference']['stderr_hex']
    result['native_resume_probe']=dict(runs=resume_runs,completed=resume_complete,observations_equal=resume_equal,fixture='crates/deadlib-lua/tests/native-boundary/native-resume.lua',complete_coroutine_api_proven=False)
    levels_complete=all(run['returncode']==0 and b'native-call-levels-complete\t1000'in bytes.fromhex(run['stdout_hex'])for run in level_runs.values())
    levels_equal=levels_complete and level_runs['reference']['stdout_hex']==level_runs['rust']['stdout_hex'] and not level_runs['reference']['stderr_hex']
    result['native_call_levels_probe']=dict(runs=level_runs,completed=levels_complete,observations_equal=levels_equal,fixture='crates/deadlib-lua/tests/native-boundary/native-levels.lua',reference_counter='source-built Lua 5.1.5 lua_State.nCcalls',complete_coroutine_api_proven=False)
    reader_levels_complete=all(run['returncode']==0 and b'reader-call-levels-complete\t26'in bytes.fromhex(run['stdout_hex'])for run in reader_level_runs.values())
    reader_levels_equal=reader_levels_complete and reader_level_runs['reference']['stdout_hex']==reader_level_runs['rust']['stdout_hex'] and not reader_level_runs['reference']['stderr_hex']
    result['reader_call_levels_probe']=dict(runs=reader_level_runs,completed=reader_levels_complete,observations_equal=reader_levels_equal,fixture='crates/deadlib-lua/tests/native-boundary/reader-call-levels.lua')
    debug_complete=all(run['returncode']==0 and b'native-debug-complete\t1000'in bytes.fromhex(run['stdout_hex'])for run in debug_runs.values())
    debug_equal=debug_complete and debug_runs['reference']['stdout_hex']==debug_runs['rust']['stdout_hex'] and not debug_runs['reference']['stderr_hex']
    result['native_debug_probe']=dict(runs=debug_runs,completed=debug_complete,observations_equal=debug_equal,fixture='crates/deadlib-lua/tests/native-boundary/native-debug.lua',full_debug_api_parity_proven=False)
    hooks_complete=all(run['returncode']==0 and b'native-hooks-complete'in bytes.fromhex(run['stdout_hex'])for run in hook_runs.values())
    hooks_equal=hooks_complete and hook_runs['reference']['stdout_hex']==hook_runs['rust']['stdout_hex'] and not hook_runs['reference']['stderr_hex']
    result['native_hooks_probe']=dict(runs=hook_runs,completed=hooks_complete,observations_equal=hooks_equal,fixture='crates/deadlib-lua/tests/native-boundary/native-hooks.lua',full_hook_parity_proven=False)
    context_complete=all(run['returncode']==0 and b'native-context-complete'in bytes.fromhex(run['stdout_hex'])for run in context_runs.values())
    context_equal=context_complete and context_runs['reference']['stdout_hex']==context_runs['rust']['stdout_hex'] and not context_runs['reference']['stderr_hex']
    result['native_context_probe']=dict(runs=context_runs,completed=context_complete,observations_equal=context_equal,fixture='crates/deadlib-lua/tests/native-boundary/native-context.lua',complete_pseudo_index_parity_proven=False)
    return_stacks_complete=all(run['returncode']==0 and b'return-stacks-complete'in bytes.fromhex(run['stdout_hex'])for run in return_stack_runs.values())
    return_stacks_equal=return_stacks_complete and return_stack_runs['reference']['stdout_hex']==return_stack_runs['rust']['stdout_hex'] and not return_stack_runs['reference']['stderr_hex']
    result['native_return_stacks_probe']=dict(runs=return_stack_runs,completed=return_stacks_complete,observations_equal=return_stacks_equal,fixture='crates/deadlib-lua/tests/native-boundary/return-stacks.lua',full_native_stack_parity_proven=False)
    protection_hooks_complete=all(run['returncode']==0 and b'protection-hooks-complete\t39'in bytes.fromhex(run['stdout_hex'])for run in protection_hook_runs.values())
    protection_hooks_equal=protection_hooks_complete and protection_hook_runs['reference']['stdout_hex']==protection_hook_runs['rust']['stdout_hex'] and not protection_hook_runs['reference']['stderr_hex']
    result['native_protection_hooks_probe']=dict(runs=protection_hook_runs,completed=protection_hooks_complete,observations_equal=protection_hooks_equal,fixture='crates/deadlib-lua/tests/native-boundary/protection-hooks.lua',full_protection_parity_proven=False)
    library_stacks_complete=all(run['returncode']==0 and b'library-stacks-complete'in bytes.fromhex(run['stdout_hex'])for run in library_stack_runs.values())
    library_stacks_equal=library_stacks_complete and library_stack_runs['reference']['stdout_hex']==library_stack_runs['rust']['stdout_hex'] and not library_stack_runs['reference']['stderr_hex']
    result['native_library_stacks_probe']=dict(runs=library_stack_runs,completed=library_stacks_complete,observations_equal=library_stacks_equal,fixture='crates/deadlib-lua/tests/native-boundary/library-stacks.lua',full_native_stack_parity_proven=False)
    package_stacks_complete=all(run['returncode']==0 and b'package-stacks-complete'in bytes.fromhex(run['stdout_hex'])for run in package_stack_runs.values())
    package_stacks_equal=package_stacks_complete and package_stack_runs['reference']['stdout_hex']==package_stack_runs['rust']['stdout_hex']and not package_stack_runs['reference']['stderr_hex']
    result['native_package_stacks_probe']=dict(runs=package_stack_runs,completed=package_stacks_complete,observations_equal=package_stacks_equal,fixture='crates/deadlib-lua/tests/native-boundary/package-stacks.lua',full_package_parity_proven=False)
    body_stack_equal={name:all(body_stack_runs[engine][name]['returncode']==0 for engine in ['reference','rust'])and body_stack_runs['reference'][name]['stdout_hex']==body_stack_runs['rust'][name]['stdout_hex']and not body_stack_runs['reference'][name]['stderr_hex']for name in ['return-stacks.lua','library-stacks.lua','package-stacks.lua']}
    result['native_body_stacks_probe']=dict(runs=body_stack_runs,observations_equal=body_stack_equal,full_native_body_parity_proven=False)
    bodies_complete=all(run['returncode']==0 and b'native-bodies-complete'in bytes.fromhex(run['stdout_hex'])for run in body_runs.values())
    bodies_equal=bodies_complete and body_runs['reference']['stdout_hex']==body_runs['rust']['stdout_hex']and not body_runs['reference']['stderr_hex']
    result['native_bodies_probe']=dict(runs=body_runs,completed=bodies_complete,observations_equal=bodies_equal,full_native_body_parity_proven=False)
    library_equal={mode:all(library_runs[e][mode]['returncode']==0 and b'native-library-complete'in bytes.fromhex(library_runs[e][mode]['stdout_hex'])for e in ['reference','rust'])and library_runs['reference'][mode]['stdout_hex']==library_runs['rust'][mode]['stdout_hex']and not library_runs['reference'][mode]['stderr_hex']for mode in ['ordinary','copied']}
    library_balanced={}
    for mode in ['ordinary','copied']:
        match=re.fullmatch(rb'BOUNDARY operations=(\d+) drops=(\d+) panic_abort=true\r?\nSTATES created=(\d+) collected=(\d+) destroyed=(\d+)\r?\nLIBRARIES opened=(\d+) closed=(\d+)\r?\n',bytes.fromhex(library_runs['rust'][mode]['stderr_hex']))
        library_balanced[mode]=bool(match and match[1]==match[2]and match[3]==match[5]and int(match[6])==int(match[7])>0)
    result['native_library_loading_probe']=dict(runs=library_runs,observations_equal=library_equal,lifetimes_balanced=library_balanced,fixture='crates/deadlib-lua/tests/native-boundary/library-loading.lua',default_production_adapter=args.runner is None,complete_native_loading_proven=False)
    inactive_objects_complete=all(run['returncode']==0 and b'inactive-objects-complete\t17'in bytes.fromhex(run['stdout_hex'])for run in inactive_object_runs.values())
    inactive_objects_equal=inactive_objects_complete and inactive_object_runs['reference']['stdout_hex']==inactive_object_runs['rust']['stdout_hex']and not inactive_object_runs['reference']['stderr_hex']
    result['native_inactive_objects_probe']=dict(runs=inactive_object_runs,completed=inactive_objects_complete,observations_equal=inactive_objects_equal,fixture='crates/deadlib-lua/tests/native-boundary/inactive-objects.lua',complete_inactive_execution_proven=False)
    thread_calls_complete=all(run['returncode']==0 and b'thread-calls-complete\t72'in bytes.fromhex(run['stdout_hex'])for run in thread_call_runs.values())
    thread_calls_equal=thread_calls_complete and thread_call_runs['reference']['stdout_hex']==thread_call_runs['rust']['stdout_hex']and not thread_call_runs['reference']['stderr_hex']
    result['native_thread_calls_probe']=dict(runs=thread_call_runs,completed=thread_calls_complete,observations_equal=thread_calls_equal,fixture='crates/deadlib-lua/tests/native-boundary/thread-calls.lua',complete_inactive_execution_proven=False)
    api_yields_complete=all(run['returncode']==0 and b'api-yields-complete\t51'in bytes.fromhex(run['stdout_hex'])for run in api_yield_runs.values())
    api_yields_equal=api_yields_complete and api_yield_runs['reference']['stdout_hex']==api_yield_runs['rust']['stdout_hex']and not api_yield_runs['reference']['stderr_hex']
    result['native_api_yields_probe']=dict(runs=api_yield_runs,completed=api_yields_complete,observations_equal=api_yields_equal,fixture='crates/deadlib-lua/tests/native-boundary/api-yields.lua',complete_inactive_execution_proven=False)
    thread_loads_complete=all(run['returncode']==0 and b'thread-loads-complete\t47'in bytes.fromhex(run['stdout_hex'])for run in thread_load_runs.values())
    thread_loads_equal=thread_loads_complete and thread_load_runs['reference']['stdout_hex']==thread_load_runs['rust']['stdout_hex']and not thread_load_runs['reference']['stderr_hex']
    result['native_thread_loads_probe']=dict(runs=thread_load_runs,completed=thread_loads_complete,observations_equal=thread_loads_equal,fixture='crates/deadlib-lua/tests/native-boundary/thread-loads.lua',complete_inactive_execution_proven=False)
    reader_continuations_complete=all(run['returncode']==0 and b'reader-continuations-complete\t114'in bytes.fromhex(run['stdout_hex'])and b'reader-environments-complete\t8'in bytes.fromhex(run['stdout_hex'])for run in reader_continuation_runs.values())
    reader_continuations_equal=reader_continuations_complete and reader_continuation_runs['reference']['stdout_hex']==reader_continuation_runs['rust']['stdout_hex']and not reader_continuation_runs['reference']['stderr_hex']
    result['native_reader_continuations_probe']=dict(runs=reader_continuation_runs,completed=reader_continuations_complete,observations_equal=reader_continuations_equal,fixture='crates/deadlib-lua/tests/native-boundary/reader-continuations.lua',arbitrary_reader_continuations_proven=False)
    panic_complete=all(run['returncode']==0 and b'native-panic-complete\t54' in bytes.fromhex(run['stdout_hex'])for run in panic_runs.values())
    panic_equal=panic_complete and panic_runs['reference']['stdout_hex']==panic_runs['rust']['stdout_hex']and not panic_runs['reference']['stderr_hex']
    result['native_panic_probe']=dict(runs=panic_runs,completed=panic_complete,observations_equal=panic_equal,fixture='crates/deadlib-lua/tests/native-boundary/native-panic.lua',complete_error_routing_proven=False)
    panic_exit_expected={'default':(b'',b'PANIC: unprotected error in call to Lua API (native failure)\r\n'),'absent':(b'',b''),'returning':(b'panic-return\t2\t1\r\n',b'')}
    panic_exit_equal={mode:all(panic_exit_runs[engine][mode]['returncode']==1 and bytes.fromhex(panic_exit_runs[engine][mode]['stdout_hex'])==out and bytes.fromhex(panic_exit_runs[engine][mode]['stderr_hex'])==err for engine in ['reference','rust'])for mode,(out,err)in panic_exit_expected.items()}
    result['native_panic_exit_probe']=dict(runs=panic_exit_runs,observations_equal=panic_exit_equal,expected_exit_code=1)
    thread_files_equal={}
    for name,collection,marker,fixture_name in [('files',thread_file_runs,b'thread-files-complete\t53','thread-files.lua'),('stdin',thread_file_stdin_runs,b'thread-file-stdin-complete\t5','thread-file-stdin.lua'),('panic',thread_file_panic_runs,b'thread-file-panic-complete\t53','thread-file-panic.lua')]:
        complete=all(run['returncode']==0 and marker in bytes.fromhex(run['stdout_hex'])for run in collection.values())
        equal=complete and collection['reference']['stdout_hex']==collection['rust']['stdout_hex']and not collection['reference']['stderr_hex']
        thread_files_equal[name]=equal
        result['native_thread_file_'+name+'_probe']=dict(runs=collection,completed=complete,observations_equal=equal,fixture='crates/deadlib-lua/tests/native-boundary/'+fixture_name,complete_inactive_execution_proven=False)
    file_exit_equal={mode:all(thread_file_exit_runs[engine][mode]['returncode']==1 and bytes.fromhex(thread_file_exit_runs[engine][mode]['stdout_hex'])==out and bytes.fromhex(thread_file_exit_runs[engine][mode]['stderr_hex'])==err for engine in ['reference','rust'])for mode,(out,err)in panic_exit_expected.items()if mode!='default'}
    result['native_thread_file_exit_probe']=dict(runs=thread_file_exit_runs,observations_equal=file_exit_equal,expected_exit_code=1)
    allocation_gc_complete=all(run['returncode']==0 and b'allocation-gc-complete\t156'in bytes.fromhex(run['stdout_hex'])for run in allocation_gc_runs.values())
    allocation_gc_equal=allocation_gc_complete and allocation_gc_runs['reference']['stdout_hex']==allocation_gc_runs['rust']['stdout_hex']and not allocation_gc_runs['reference']['stderr_hex']
    result['native_allocation_gc_probe']=dict(runs=allocation_gc_runs,completed=allocation_gc_complete,observations_equal=allocation_gc_equal,fixture='crates/deadlib-lua/tests/native-boundary/allocation-gc.lua',complete_gc_timing_parity_proven=False)
    execution_equal={}
    for name,collection,marker,fixture_name in [('execution',execution_runs,b'thread-execution-complete\t54','thread-execution.lua'),('execution_panic',execution_panic_runs,b'thread-execution-panic-complete\t10','thread-execution-panic.lua'),('concat_yields',concat_yield_runs,b'concat-yields-complete\t40','concat-yields.lua')]:
        complete=all(run['returncode']==0 and marker in bytes.fromhex(run['stdout_hex'])for run in collection.values())
        equal=complete and collection['reference']['stdout_hex']==collection['rust']['stdout_hex']and not collection['reference']['stderr_hex']
        execution_equal[name]=equal
        result['native_thread_'+name+'_probe']=dict(runs=collection,completed=complete,observations_equal=equal,fixture='crates/deadlib-lua/tests/native-boundary/'+fixture_name,complete_inactive_execution_proven=False)
    core=set()
    waiting_errors_complete=all(run['returncode']==0 and b'waiting-errors-complete\t736'in bytes.fromhex(run['stdout_hex'])for run in waiting_error_runs.values())
    waiting_errors_equal=waiting_errors_complete and waiting_error_runs['reference']['stdout_hex']==waiting_error_runs['rust']['stdout_hex']and not waiting_error_runs['reference']['stderr_hex']
    result['native_waiting_errors_probe']=dict(runs=waiting_error_runs,completed=waiting_errors_complete,observations_equal=waiting_errors_equal,fixture='crates/deadlib-lua/tests/native-boundary/waiting-errors.lua',complete_cross_thread_errors_proven=False)
    unprotected_complete=all(run['returncode']==0 and b'thread-unprotected-complete\t960'in bytes.fromhex(run['stdout_hex'])for run in thread_unprotected_runs.values())
    unprotected_equal=unprotected_complete and thread_unprotected_runs['reference']['stdout_hex']==thread_unprotected_runs['rust']['stdout_hex']and not thread_unprotected_runs['reference']['stderr_hex']
    result['native_thread_unprotected_probe']=dict(runs=thread_unprotected_runs,completed=unprotected_complete,observations_equal=unprotected_equal,fixture='crates/deadlib-lua/tests/native-boundary/thread-unprotected.lua',complete_cross_thread_errors_proven=False)
    call_yields_complete=all(run['returncode']==0 and b'call-yields-complete\t1280'in bytes.fromhex(run['stdout_hex'])for run in call_yield_runs.values())
    call_yields_equal=call_yields_complete and call_yield_runs['reference']['stdout_hex']==call_yield_runs['rust']['stdout_hex']and not call_yield_runs['reference']['stderr_hex']
    result['native_call_yields_probe']=dict(runs=call_yield_runs,completed=call_yields_complete,observations_equal=call_yields_equal,fixture='crates/deadlib-lua/tests/native-boundary/call-yields.lua',complete_interleaved_execution_proven=False)
    gc_integers_complete=all(run['returncode']==0 and b'gc-integers-complete\t1063'in bytes.fromhex(run['stdout_hex'])for run in gc_integer_runs.values())
    gc_integers_equal=gc_integers_complete and gc_integer_runs['reference']['stdout_hex']==gc_integer_runs['rust']['stdout_hex']and not gc_integer_runs['reference']['stderr_hex']
    result['native_gc_integers_probe']=dict(runs=gc_integer_runs,completed=gc_integers_complete,observations_equal=gc_integers_equal,fixture='crates/deadlib-lua/tests/native-boundary/gc-integers.lua',complete_gc_work_and_accounting_proven=False)
    table_threads_complete=all(run['returncode']==0 and b'thread-tables-complete\t3468'in bytes.fromhex(run['stdout_hex'])for run in table_thread_runs.values())
    table_threads_equal=table_threads_complete and table_thread_runs['reference']['stdout_hex']==table_thread_runs['rust']['stdout_hex']and not table_thread_runs['reference']['stderr_hex']
    result['native_thread_tables_probe']=dict(runs=table_thread_runs,completed=table_threads_complete,observations_equal=table_threads_equal,fixture='crates/deadlib-lua/tests/native-boundary/thread-tables.lua',complete_inactive_execution_proven=False)
    table_yields_complete=all(run['returncode']==0 and b'table-yields-complete\t1078'in bytes.fromhex(run['stdout_hex'])for run in table_yield_runs.values())
    table_yields_equal=table_yields_complete and table_yield_runs['reference']['stdout_hex']==table_yield_runs['rust']['stdout_hex']and not table_yield_runs['reference']['stderr_hex']
    result['native_table_yields_probe']=dict(runs=table_yield_runs,completed=table_yields_complete,observations_equal=table_yields_equal,fixture='crates/deadlib-lua/tests/native-boundary/table-yields.lua',complete_interleaved_execution_proven=False)
    io_closers_complete=all(run['returncode']==0 and b'io-closers-complete\t231'in bytes.fromhex(run['stdout_hex'])for run in io_closer_runs.values())
    io_closers_equal=io_closers_complete and io_closer_runs['reference']['stdout_hex']==io_closer_runs['rust']['stdout_hex']and not io_closer_runs['reference']['stderr_hex']
    result['native_io_closers_probe']=dict(runs=io_closer_runs,completed=io_closers_complete,observations_equal=io_closers_equal,fixture='crates/deadlib-lua/tests/native-boundary/io-closers.lua',complete_io_callback_interleavings_proven=False)
    if not io_closers_equal:raise SystemExit('native I/O close callback observations differ')
    unprotected_exit_equal={key:all(row['returncode']==1 and row['stdout_hex']==row['expected_stdout_hex']and not row['stderr_hex']and row['buffered_file_hex']==b'waiting\0buffered\n'.hex()for row in [thread_unprotected_exit_runs[engine][key]for engine in ['reference','rust']])for key in thread_unprotected_exit_runs['reference']}
    result['native_thread_unprotected_exit_probe']=dict(runs=thread_unprotected_exit_runs,observations_equal=unprotected_exit_equal,expected_exit_code=1,fixture='crates/deadlib-lua/tests/native-boundary/thread-unprotected-exit.lua')
    gc_thread_equal={}
    for name,collection,marker,fixture_name in [('controls',thread_gc_runs,b'thread-gc-complete\t52','thread-gc.lua'),('panic',thread_gc_panic_runs,b'thread-gc-panic-complete\t100','thread-gc-panic.lua')]:
        complete=all(run['returncode']==0 and marker in bytes.fromhex(run['stdout_hex'])for run in collection.values())
        equal=complete and collection['reference']['stdout_hex']==collection['rust']['stdout_hex']and not collection['reference']['stderr_hex']
        gc_thread_equal[name]=equal
        result['native_thread_gc_'+name+'_probe']=dict(runs=collection,completed=complete,observations_equal=equal,fixture='crates/deadlib-lua/tests/native-boundary/'+fixture_name,complete_inactive_execution_proven=False,heap_count_bytes_equal_proven=False)
    for line in (root/'vendor/lua-src/lua-5.1.5/lua.h').read_text().splitlines():
        if line.startswith('LUA_API '):
            parenthesized=re.findall(r'\((lua_\w+)\)',line)
            core.add(parenthesized[0] if parenthesized else re.findall(r'\blua_\w+\b',line.split('(')[0])[-1])
    declarations=set(re.findall(r'^API[^;\n]*?\b(lua_\w+)\s*\(', (root/'crates/deadlib-lua/src/native_abi/api.h').read_text(),re.M))
    result['core_c_api_declarations']=dict(implemented=len(core&declarations),total=len(core),missing=sorted(core-declarations),complete_semantics_proven=False)
    result['native_file_complete']=file_loaders
    result['native_dump_complete']=dumpers
    result['native_thread_complete']=threads
    result['native_pointer_complete']=pointers
    result['native_dump_fixtures']={name:dict(sha256=hashlib.sha256(chunk).hexdigest(),hex=chunk.hex())for name,chunk in extra_chunks.items()}
    result['stdin_hex']=b'17 return 42\n'.hex()
    result['limitations']+=' Native reader/buffer/string/file loading is a development subset. Parser root observations are compared independently for fragmented text and binary input; arbitrary mutation of private parser state, allocator/OOM statuses, full native loader/C API parity remain unfinished.'
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print('program observations equal:',matched,'dispatch/drop guards balanced:',returned,'state lifetimes balanced:',states,flush=True)
    if not matched or not returned or not states or not scratch_complete or not result['parser_scratch_probe']['observations_equal'] or not roots_equal or not thread_stacks_equal or not resume_equal or not levels_equal or not reader_levels_equal or not debug_equal or not hooks_equal or not context_equal or not return_stacks_equal or not protection_hooks_equal or not library_stacks_equal or not package_stacks_equal or not all(body_stack_equal.values()) or not bodies_equal or not all(library_equal.values()) or not all(library_balanced.values()) or not inactive_objects_equal or not thread_calls_equal or not api_yields_equal or not thread_loads_equal or not reader_continuations_equal or not panic_equal or not all(panic_exit_equal.values()) or not all(thread_files_equal.values()) or not all(file_exit_equal.values()) or not allocation_gc_equal or not all(execution_equal.values()) or not all(gc_thread_equal.values()) or not waiting_errors_equal or not unprotected_equal or not all(unprotected_exit_equal.values()) or not call_yields_equal or not gc_integers_equal or not table_threads_equal or not table_yields_equal:raise SystemExit(1)

if __name__=='__main__':main()
