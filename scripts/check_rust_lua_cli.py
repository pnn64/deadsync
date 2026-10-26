"""Audit standalone CLI behavior against checksum-pinned official Lua 5.1.5."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from build_lua_cli_reference import build

def main() -> None:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--rust-binary',type=Path,help='Development-only prebuilt runner; its source provenance is not established by this audit')
    args=parser.parse_args()
    root=Path(__file__).resolve().parents[1]
    sources=list((root/'crates/deadlib-lua').rglob('*.rs'))+[root/'crates/deadlib-lua/Cargo.toml',Path(__file__).resolve(),root/'scripts/build_lua_cli_reference.py']
    def hashes():return {p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(sources)}
    before=hashes()
    if args.rust_binary is None:
        subprocess.run(['cargo','build','--release','-p','deadlib-lua','--bin','deadlua'],cwd=root,check=True)
    reference,metadata=build(root)
    binary=args.rust_binary.resolve() if args.rust_binary else root/('target/release/deadlua.exe' if os.name=='nt' else 'target/release/deadlua')
    with tempfile.TemporaryDirectory(prefix='lua-cli-chunk-') as temporary:
        source="local f=assert(io.open('fixture.bin','wb'));f:write(string.dump(function(a,...)print(a,select('#',...),...);print(_G.arg[0])end));f:close();local f=assert(io.open('fixture-error.bin','wb'));f:write(string.dump(function(a,...)print(a,select('#',...),...);print(arg[0])end));f:close()"
        subprocess.run(['lua','-e',source],executable=reference,cwd=temporary,check=True,capture_output=True)
        native=(Path(temporary)/'fixture.bin').read_bytes()
        native_error=(Path(temporary)/'fixture-error.bin').read_bytes()
    cases=[]
    def case(name,arguments,stdin=b'',env=None,files=None):
        cases.append(dict(name=name,arguments=arguments,stdin=stdin,environment=env or {},files=files or {}))
    script=b"print(arg[0],arg[-1],arg[-2],arg[-3]);print(select('#',...),...);print(arg[1],arg[2],arg[3],arg[4]);print(x)"
    case('piped_stdin',[],b"print('stdin',arg==nil,select('#',...))")
    case('empty_stdin',[])
    case('command_chain',['-ex=3','-e','print(x,arg==nil)'])
    case('empty_command',['-e',''],b"error('stdin should not execute')")
    case('module_then_command',['-lmath','-eprint(math.sqrt(81))'])
    case('module_then_stdin',['-l','math'],b'print(math.sqrt(81),arg==nil)')
    case('override_require',['-l','x','-eprint("end")'],env={'LUA_INIT':"require=function(s)print('require',s)end"})
    case('script_arguments',['probe.lua','one','','three'],files={'probe.lua':script})
    case('options_before_script',['-e','x=7','probe.lua','a','b'],files={'probe.lua':script})
    case('options_after_script',['probe.lua','-e','-l','--'],files={'probe.lua':script})
    case('dash_stdin_arguments',['-','one','two'],script)
    case('dash_after_delimiter_is_filename',['--','-','one'],files={'-':script})
    case('option_named_file',['--','-e','one'],files={'-e':script})
    case('delimiter_stdin',['--'],b'print(arg==nil)')
    case('argument_quoting',['probe.lua','a b','a"b',r'c:\test\path'],files={'probe.lua':script})
    case('initialization_string',['-eprint(x)'],env={'LUA_INIT':"print('init',arg==nil);x=17"})
    case('initialization_file',['-eprint(x)'],env={'LUA_INIT':'@init.lua'},files={'init.lua':b"#!header\nprint('init',arg==nil);x=19"})
    case('internal_loader_survives_override',['probe.lua','a'],env={'LUA_INIT':"loadfile=nil;dofile=nil;collectgarbage('collect')"},files={'probe.lua':script})
    case('source_file_name',['probe.lua'],files={'probe.lua':b"print(debug.getinfo(1,'S').source)"})
    case('shebang_and_crlf',['probe.lua','a'],files={'probe.lua':b"#!header\r\nprint(...,debug.getinfo(1,'S').source)\r\n"})
    case('binary_script_arguments',['probe.lua','one','two'],files={'probe.lua':native})
    case('binary_script_header',['probe.lua','one','two'],files={'probe.lua':b'#!header\r\n'+native})
    case('binary_script_error',['probe.lua','one','two'],files={'probe.lua':native_error})
    case('stdin_header',['-'],b'#!header\r\nprint(42)\r\n')
    case('init_hooks',['probe.lua'],env={'LUA_INIT':"debug.sethook(function(e)print('hook',e)end,'cr')"},files={'probe.lua':b'local a=1;print(a)'})
    case('syntax_error',['-e','local ='])
    case('script_syntax_error',['probe.lua'],files={'probe.lua':b'local ='})
    case('initialization_syntax_error',['-eprint("never")'],env={'LUA_INIT':'local ='})
    case('missing_file',['missing.lua'])
    case('runtime_error',['-e',"error('oops')"])
    case('runtime_error_without_traceback',['-e',"error('oops')"],env={'LUA_INIT':'debug.traceback=nil'})
    case('nonstring_error',['-e','error({})'])
    case('numeric_error',['-e','error(17)'])
    case('custom_traceback',['-e',"error('oops')"],env={'LUA_INIT':"debug.traceback=function(e,l)return 'handled:'..e..':'..l end"})
    case('nil_error',['-e','error(nil)'])
    case('empty_error_without_traceback',['-e',"error('',0)"],env={'LUA_INIT':'debug.traceback=nil'})
    case('nil_handler_result',['-e',"error('bad')"],env={'LUA_INIT':'debug.traceback=function()end'})
    case('object_handler_result',['-e',"error('bad')"],env={'LUA_INIT':'debug.traceback=function()return {} end'})
    case('numeric_handler_result',['-e',"error('bad')"],env={'LUA_INIT':'debug.traceback=function()return 17 end'})
    case('missing_debug',['-e',"error('bad')"],env={'LUA_INIT':'debug=nil'})
    case('callable_handler_is_ignored',['-e',"error('bad')"],env={'LUA_INIT':"debug.traceback=setmetatable({},{__call=function()error('wrong')end})"})
    case('traceback_lookup_callback',['-e',"error('bad')"],env={'LUA_INIT':"debug=setmetatable({},{__index=function(t,k)if k=='traceback' then return function(e,l)return e..':'..l end end end})"})
    case('failing_handler',['-e',"error('bad')"],env={'LUA_INIT':"debug.traceback=function()error('handler failed')end"})
    case('live_faulting_locals',['-e',"local function f()local x=7;error('bad')end;f()"],env={'LUA_INIT':"debug.traceback=function(e,l)local n,v=debug.getlocal(4,1);debug.setlocal(4,1,9);collectgarbage();return n..':'..v..':'..l end"})
    reader="local f,e=load(function()error('reader',0)end);print(f,e)"
    trace_identity="local old;debug.traceback=function(e,l)local f=debug.getinfo(2,'f').func;local same=old==f;old=f;return tostring(same)..':'..e end"
    case('reader_inherits_handler_identity',['-e',reader+';'+reader],env={'LUA_INIT':trace_identity})
    case('successive_entries_create_distinct_handlers',['-e',reader,'-e',reader],env={'LUA_INIT':trace_identity})
    case('handler_environment_mutation',['-e',reader+';'+reader],env={'LUA_INIT':"debug.traceback=function()local caller=debug.getinfo(2,'f').func;debug.setfenv(caller,{debug={traceback=function()return 'replaced'end}});return 'first'end"})
    case('handler_result_gc_root',['-e',"error('bad')"],env={'LUA_INIT':"local weak=setmetatable({},{__mode='v'});debug.traceback=function()local result={};weak[1]=result;local p=newproxy(true);getmetatable(p).__gc=function()print('gc',weak[1]~=nil)end;return result end"})
    case('collection_after_error_can_replace_the_error',['-e',"local p=newproxy(true);getmetatable(p).__gc=function()error('gc failed')end;p=nil;error('original')"])
    case('root_environment_does_not_retain_failed_call_locals',['-e',"local p=newproxy(true);getmetatable(p).__gc=function()error('gc failed')end;local env={p=p,error=error};p=nil;setfenv(1,env);error('original')"])
    case('outer_main_debug_fields',['-e',"local d=debug.getinfo(2,'Suf');print(d.what,d.source,d.short_src,d.linedefined,d.lastlinedefined,d.nups,type(d.func),getfenv(d.func)==_G,debug.getinfo(3)==nil)"])
    case('unprotected_argument_callback_error',['probe.lua'],env={'LUA_INIT':"setmetatable(_G,{__newindex=function(t,k,v)if k=='arg' then error('arg failed')end;rawset(t,k,v)end});debug.sethook(function(e)print('hook',e)end,'cr')"},files={'probe.lua':b"print('never')"})
    case('error_binary_message',['-e',"error('a\\000b',0)"])
    case('reader_handler_is_disabled_inside_lua_pcall',['-e',"local ok,a,b=pcall(function()return load(function()error('reader',0)end)end);print(ok,a,b)"],env={'LUA_INIT':"debug.traceback=function(e)return 'handled:'..e end"})
    case('error_handler_call_and_return_hooks',['-e',"error('bad')"],env={'LUA_INIT':"debug.sethook(function(e)print('hook',e)end,'cr')"})
    case('recursive_handler_depth',['-e',"error('bad')"],env={'LUA_INIT':"local n=0;holder=newproxy(true);getmetatable(holder).__gc=function()print('handlers',n)end;debug.traceback=function()n=n+1;error('again',0)end"})
    case('lua_stack_overflow',['-e',"local function f()local x=f();return x end;f()"])
    case('arg_global_metamethod',['probe.lua'],env={'LUA_INIT':"setmetatable(_G,{__newindex=function(t,k,v)if k=='arg' then print('arg',v[0])end;rawset(t,k,v)end})"},files={'probe.lua':b'print(arg[0])'})
    case('replaced_arg_table_can_be_collected',['probe.lua','a'],files={'probe.lua':b"local w=setmetatable({arg},{__mode='v'});arg=nil;collectgarbage('collect');print(w[1]==nil,...)"})
    case('argument_roots_through_global_callback',['probe.lua','a','b'],env={'LUA_INIT':"setmetatable(_G,{__newindex=function(t,k,v)if k=='arg' then collectgarbage('collect');return end;rawset(t,k,v)end})"},files={'probe.lua':b"collectgarbage('collect');print(arg==nil,...)"})
    for name,arguments in [('missing_e',['-e']),('missing_l',['-l']),('unknown_option',['-q']),('invalid_version_tail',['-vv']),('version',['-v']),('interactive',['-i'])]:
        case(name,arguments,b'=1\n')
    results=[]
    with tempfile.TemporaryDirectory(prefix='deadlua-cli-') as temporary:
        directory=Path(temporary)
        for item in cases:
            row={k:v for k,v in item.items() if k not in ('stdin','files')}
            row['stdin_hex']=item['stdin'].hex()
            row['files']={k:v.hex() for k,v in item['files'].items()}
            for engine,executable in [('reference',reference),('rust',binary)]:
                work=directory/engine
                work.mkdir(exist_ok=True)
                for name in ('probe.lua','init.lua','-','-e'):(work/name).unlink(missing_ok=True)
                for name,contents in item['files'].items():(work/name).write_bytes(contents)
                env=os.environ.copy()
                for name in ('LUA_INIT','LUA_PATH','LUA_CPATH'):env.pop(name,None)
                env['LUA_PATH']='?.lua;./?.lua'
                env.update(item['environment'])
                try:
                    run=subprocess.run(['lua',*item['arguments']],executable=executable,cwd=work,input=item['stdin'],env=env,capture_output=True,timeout=10)
                    row[engine]=dict(returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
                except subprocess.TimeoutExpired:row[engine]=dict(timeout=True)
            row['equal']=row['reference']==row['rust']
            results.append(row)
            print(item['name'],'pass' if row['equal'] else 'FAIL',flush=True)
    if hashes()!=before:raise SystemExit('sources changed during CLI audit')
    result=dict(source_hashes=before,reference=metadata,rust_binary=dict(path=str(binary),sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),prebuilt=args.rust_binary is not None),cases=results,passed=sum(r['equal'] for r in results),
                limitations='Fixed isolated CLI audit. ASCII Windows argv/path contract; full interactive, diagnostic and environment semantics remain required where mismatches are recorded.')
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(result,indent=2)+'\n')
    print(f"{result['passed']}/{len(results)} CLI probes match")
    if result['passed']!=len(results):raise SystemExit(1)

if __name__=='__main__':main()
