"""Audit standalone CLI behavior against checksum-pinned official Lua 5.1.5."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib
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
    case('piped_stdin_runtime_error',[],b"error('stdin')")
    case('piped_stdin_syntax_error',[],b'local =')
    case('piped_stdin_nil_error',[],b'error(nil)')
    case('module_then_stdin_error',['-lmath'],b"error('stdin')")
    case('explicit_stdin_error',['-'],b"error('stdin')")
    case('piped_stdin_finalizer_error',[],b"local p=newproxy(true);getmetatable(p).__gc=function()error('gc failed')end;p=nil;error('original')")
    case('native_select_int_narrowing',['-e',"print(select(4294967297,'a',nil,false));print(pcall(select,1/0,'a'));print(select(-4294967297,'a','b'))"])
    case('native_unpack_int_narrowing',['-e',"local t={[0]='zero','one','two'};print(unpack(t,4294967296,4294967298));print(unpack(t,0/0,0/0))"])
    case('native_character_int_narrowing',['-e',"io.write(string.char(4294967361,1/0,4294967551));print(pcall(string.char,4294967552))"])
    case('native_repeat_int_narrowing',['-e',"print(string.rep('xy',4294967299),string.rep('x',1/0),string.rep('x',-4294967297))"])
    case('native_unpack_result_limit',['-e',"unpack({},1,7998)"])
    case('native_byte_result_limit',['-e',"string.byte(string.rep('a',7998),1,7998)"])
    case('native_unpack_empty_and_overflow',['-e',"print(pcall(unpack,{},3,1));print(pcall(unpack,{},-2147483648,2147483647))"])
    for n in (7996,7997,7998,7999):
        case(f'native_unpack_range_{n}',['-e',f"local function count(...)return select('#',...)end;local ok,n=pcall(function()return count(unpack({{}},1,{n}))end);print(ok,n)"])
        case(f'native_byte_range_{n}',['-e',f"local function count(...)return select('#',...)end;local ok,n=pcall(function()return count(string.byte(string.rep('a',{n}),1,{n}))end);print(ok,n)"])
    case('native_unused_arguments_count_toward_limit',['-e',"print(pcall(unpack,{},1,7997,true));print(pcall(string.byte,string.rep('a',7997),1,7997,true))"])
    for name,arguments in [('empty',"{},'',-2147483648,2147483647"),('boolean',"{false},'',1,10000002"),('second',"{'a'},'',1,10000002"),('separator',"{},false,1,10000002")]:
        case(f'table_concat_wide_{name}',['-e',f'print(pcall(table.concat,{arguments}))'])
    case('table_insert_large_gap',['-e',"local t={4,5};table.insert(t,-10000001,'x');print(t[-10000001],t[-10000000],t[0],t[1],t[2],t[3])"])
    for name,expression in [('find',"string.find(s,'^a*a*a*b')"),('match',"string.match(s,'^a*a*a*b')"),('gmatch',"string.gmatch(string.rep('a',140),'a*a*a*b')()"),('gsub',"string.gsub(s,'^a*a*a*b','x')")]:
        case(f'pattern_finite_backtracking_{name}',['-e',f"local s=string.rep('a',400);print({expression})"])
    case('pattern_deferred_error_after_long_search',['-e',"local s=string.rep('a',500000)..'1';local p=string.rep('a',31)..'%d[';print(pcall(string.find,s,p))"])
    case('sort_discards_unused_roots',['-e',"local weak=setmetatable({},{__mode='v'});local function extra()local p={};weak[1]=p;return p end;local gone;local t={2,1};table.sort(t,function(a,b)collectgarbage();gone=weak[1]==nil;return a<b end,extra());print(gone,t[1],t[2])"])
    case('sort_return_hook_optional_slot',['-e',"local name,kind;local sort=table.sort;debug.sethook(function(e)if e=='return' and debug.getinfo(2,'f').func==sort then local n,v=debug.getlocal(2,2);name=n;kind=type(v)end end,'r');sort({});debug.sethook();print(name,kind)"])
    case('sort_live_native_arguments',['-e',"local out={};local a={3,1,2};local b={7,6,5};local function second(x,y)out[#out+1]='new:'..x..':'..y;return x<y end;table.sort(a,function(x,y)out[#out+1]='old:'..x..':'..y;debug.setlocal(2,1,b);debug.setlocal(2,2,second);return x<y end);print(table.concat(out,'|'),table.concat(a,','),table.concat(b,','))"])
    case('sort_releases_replaced_comparator',['-e',"local weak=setmetatable({},{__mode='v'});local gone=false;local function second(a,b)collectgarbage();gone=weak[1]==nil;return a<b end;local function make()local function first(a,b)debug.setlocal(2,2,second);return a<b end;weak[1]=first;return first end;local t={3,2,1};table.sort(t,make());print(gone,table.concat(t,','))"])
    case('sort_releases_replaced_table',['-e',"local weak=setmetatable({},{__mode='v'});local other={7,6,5};local gone=false;local first=true;local function make()local t={3,2,1};weak[1]=t;return t end;table.sort(make(),function(a,b)if first then first=false;debug.setlocal(2,1,other);collectgarbage();gone=weak[1]==nil end;return a<b end);print(gone,table.concat(other,','))"])
    case('sort_callable_replacement',['-e',"local n=0;local replacement=setmetatable({},{__call=function(self,a,b)n=n+1;return a<b end});local t={3,2,1};table.sort(t,function(a,b)debug.setlocal(2,2,replacement);return a<b end);print(n,table.concat(t,','))"])
    case('sort_invalid_replacement_call',['-e',"local t={3,2,1};print(pcall(function()table.sort(t,function(a,b)debug.setlocal(2,2,false);return a<b end)end))"])
    case('sort_metamethod_replaces_comparator',['-e',"local out={};local function second(a,b)out[#out+1]='new';return a.n<b.n end;local mt={__lt=function(a,b)out[#out+1]='meta';debug.setlocal(2,2,second);return a.n<b.n end};local t={};for i=1,3 do t[i]=setmetatable({n=4-i},mt)end;table.sort(t);print(t[1].n,t[2].n,t[3].n,table.concat(out,','))"])
    case('native_early_return_after_table_change',['-e',"print(table.foreach({7},function()debug.setlocal(2,1,false);return 42 end));print(table.foreachi({7},function()debug.setlocal(2,1,false);return 43 end));local n=0;print(pcall(function()table.sort({1,2,3},function(a,b)n=n+1;if n==3 then debug.setlocal(2,1,false)end;return a<b end)end))"])
    for function in ('foreach','foreachi'):
        case(f'traversal_callable_arguments_{function}',['-e',f"local out={{}};local replacement=setmetatable({{}},{{__call=function(self,k,v)out[#out+1]='new:'..k..':'..v end}});table.{function}({{4,5,6}},function(k,v)out[#out+1]='old:'..k..':'..v;debug.setlocal(2,1,{{7,8,9}});debug.setlocal(2,2,replacement)end);print(table.concat(out,'|'))"])
    case('traversal_discards_wide_native_results',['-e',"local key={1,2,3,4,5,6};print(select('#',table.foreach({[key]=0},unpack)))"])
    case('native_operand_error_locations',['-e',"print(pcall(nil));print(pcall(false));print(pcall(function()return next({},17)end));print(pcall(function()return table.sort({true,false})end));print(pcall(function()return true<false end));print(pcall(function()local t={};setmetatable(t,{__index=t});return string.gsub('a','.',t)end));print(pcall(function()return rawset({},nil,1)end));print(pcall(function()local t={};t[nil]=1 end))"])
    case('protected_call_required_slots',['-e',"print(pcall(function()return pcall()end));print(pcall(function()return xpcall()end));print(pcall(function()return xpcall(nil)end))"])
    case('debug_console_caller',['-e',"local x=17;local n=select('#',debug.debug('kept',false));print('after',n,x,shared)"],b"shared=42\nlocal n,v=debug.getlocal(3,1);print(n,v);debug.setlocal(3,1,19)\ncont\n")
    case('debug_console_recovers',['-e',"local n=0;print(xpcall(function()debug.debug();return 'done'end,function(e)n=n+1;return 'wrong'end));print('handlers',n)"],b"local =\nerror('failed',0)\nerror(17,0)\nprint('alive')\ncont\n")
    case('debug_console_reader_handler',['-e',"print(xpcall(function()debug.debug()end,function()return 'wrong'end))"],b"local f,e=load(function()error('reader',0)end);print(f,e)\ncont\n")
    case('debug_console_uses_original_streams',['-e',"io.input('data.txt');io.stdin=nil;io.stderr=nil;collectgarbage();debug.debug();print(io.read())"],b"print('debug')\ncont\n",files={'data.txt':b'file line\n'})
    case('debug_console_shared_stdin',['-e',"debug.debug();print('done')"],b"print(io.read())\nconsumed\ncont\n")
    case('debug_console_nested',['-e',"debug.debug();print('done',n)"],b"n=1;debug.debug();n=n+10\nn=n+2\ncont\nprint(n)\ncont\n")
    case('debug_console_fragment_limit',['-e',"debug.debug()"],b'--'+b'x'*247+b"print('fragment')\ncont\n")
    case('debug_console_cont_without_newline',['-e',"debug.debug();print('done')"],b'cont')
    case('debug_console_no_expression_shorthand',['-e',"debug.debug();print('done')"],b'=17\ncont\n')
    case('debug_console_embedded_nul',['-e',"debug.debug();print('done')"],b"print('before')\x00error('ignored')\ncont\n")
    case('debug_console_crlf_control_eof',['-e',"debug.debug();print('done')"],b"print(17)\r\nprint(19)\x1a\nprint(23)\n")
    case('debug_console_initial_argument_roots',['-e',"weak=setmetatable({},{__mode='v'});local function arg()local t={};weak[1]=t;return t end;debug.debug(arg());print('done')"],b"collectgarbage();print(weak[1]~=nil,debug.getlocal(2,1)~=nil)\ncollectgarbage();print(weak[1]==nil,debug.getlocal(2,1)==nil)\ncont\n")
    case('debug_console_thread_environment',['-e',"local g=_G;setfenv(0,setmetatable({answer=42},{__index=g}));debug.debug()"],b"print(answer,debug.getinfo(1,'S').source)\ncont\n")
    case('debug_console_coroutine',['-e',"local co=coroutine.create(function()debug.debug();return 42 end);print(coroutine.resume(co))"],b"coroutine.yield(17)\nprint('recovered')\ncont\n")
    case('debug_console_initial_and_return_locals',['-e',"local fn=debug.debug;debug.sethook(function(e)if debug.getinfo(2,'f').func==fn then local n,v=debug.getlocal(2,1);print(e,n,type(v),type(v)=='number' and v or '')end end,'cr');fn(17);fn(19);debug.sethook()"],b"print(debug.getlocal(2,1))\nprint(debug.getlocal(2,1))\ncont\ncont\n")
    case('debug_console_discards_results',['-e',"debug.debug();print('done')"],b"return 17,false,nil,'ignored'\nprint(select('#',...))\ncont\n")
    case('debug_console_binary_errors',['-e',"debug.debug();print('done')"],b"error('a\\000b',0)\ncont\n")
    case('debug_console_parser_depth',['-e',"local function f(n)if n==0 then debug.debug()else pcall(f,n-1)end end;f(180);print('done')"],b'print('+b'('*30+b'42'+b')'*30+b')\ncont\n')
    for n in range(7957,7963):
        for name,comparator in [('nil','nil'),('invalid','false')]:
            case(f'sort_stack_{n}_{name}',['-e',f"local args={{}};for i=1,{n} do args[i]=true end;print(pcall(function()return table.sort({{}},{comparator},unpack(args))end))"])
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
    case('invalid_interactive_tail',['-ii'])
    case('require_global_lookup_callback',['-lx'],env={'LUA_INIT':"require=nil;setmetatable(_G,{__index=function(t,k)if k=='require' then return function(n)print('required',n)end end end})"})
    case('interactive_empty',['-i'])
    case('interactive_results_and_globals',['-i'],b'x=7\n=x, nil, false\nreturn x+1\n')
    case('interactive_continuations',['-i'],b'function f(a)\nreturn a*3\nend\n=f(7)\n')
    case('interactive_incomplete_eof',['-i'],b'function f()\n')
    case('interactive_unterminated_string',['-i'],b"='abc\n=9\n")
    case('interactive_long_string',['-i'],b'=[=[one\ntwo]=]\n')
    case('interactive_syntax_recovery',['-i'],b'local =\n=17\n')
    case('interactive_runtime_recovery',['-i'],b"error('bad')\n=17\n")
    case('interactive_nil_and_object_errors',['-i'],b'error(nil)\nerror({})\n=17\n')
    case('interactive_custom_handler',['-i'],b"error('bad')\n=17\n",env={'LUA_INIT':"debug.traceback=function(e,l)return 'handled:'..e..':'..l end"})
    case('interactive_prompts',['-i'],b'if true then\nprint(7)\nend\n',env={'LUA_INIT':"_PROMPT='one\\000hidden';_PROMPT2=17"})
    case('interactive_prompt_lookup_callback',['-i'],b'if true then\nprint(7)\nend\n',env={'LUA_INIT':"local n=0;setmetatable(_G,{__index=function(t,k)if k=='_PROMPT' or k=='_PROMPT2' then n=n+1;collectgarbage();return k..n..':' end end})"})
    case('interactive_prompt_lookup_failure',['-i'],b'=17\n',env={'LUA_INIT':"setmetatable(_G,{__index=function(t,k)if k=='_PROMPT' then error('prompt failed')end end})"})
    case('interactive_replaced_print',['-i'],b'=1, nil, false\n',env={'LUA_INIT':"print=function(...)io.write(select('#',...),':',tostring(...),'\\n')end"})
    case('interactive_callable_print',['-i'],b'=17\n',env={'LUA_INIT':"print=setmetatable({},{__call=function(t,...)io.write(type(t),':',...,'\\n')end})"})
    case('interactive_print_failure',['-i'],b'=1\n=2\n',env={'LUA_INIT':"print=function()error('print failed')end;debug.traceback=function()return 'wrong'end"})
    case('interactive_print_nonstring_errors',['-i'],b'=1\n=2\n',env={'LUA_INIT':"local n=0;print=function()n=n+1;if n==1 then error(nil)else error({})end end"})
    case('interactive_print_lookup_result_roots',['-i'],b"=setmetatable({},{__tostring=function()return 'alive'end})\n",env={'LUA_INIT':"local old=print;print=nil;setmetatable(_G,{__index=function(t,k)if k=='print' then collectgarbage();return old end end})"})
    case('interactive_print_lookup_failure',['-i'],b'=17\n',env={'LUA_INIT':"print=nil;setmetatable(_G,{__index=function(t,k)if k=='print' then error('lookup failed')end end})"})
    case('interactive_reader_in_print_has_no_handler',['-i'],b'=17\n',env={'LUA_INIT':"print=function()local f,e=load(function()error('reader',0)end);io.write(tostring(f),':',e,'\\n')end;debug.traceback=function()return 'wrong'end"})
    case('interactive_shared_stdin',['-i'],b'=io.read()\nconsumed\n=9\n')
    case('interactive_stdin_after_init_read',['-i'],b'before\n=3\n',env={'LUA_INIT':"print('init',io.read())"})
    case('interactive_after_script',['-i','probe.lua','one'],b'=arg[0],arg[1],x\n',files={'probe.lua':b"x=9;print('script',...)"})
    case('interactive_skipped_after_script_error',['-i','probe.lua'],b'=17\n',files={'probe.lua':b"error('script failed')"})
    case('interactive_uses_original_stdin',['-i'],b'=io.read()\n=9\n',env={'LUA_INIT':"io.input('data.txt');io.stdin=nil;collectgarbage()"},files={'data.txt':b'file line\n'})
    case('interactive_input_fragment_limit',['-i'],b'--'+b'x'*509+b'\n=3\n')
    case('interactive_embedded_nul',['-i'],b'=1\x00ignored\n=3\n')
    case('interactive_crlf_and_control_eof',['-i'],b'=1\r\n=3\x1a\n=9\n')
    case('interactive_call_return_hooks',['-i'],b'=17\n',env={'LUA_INIT':"debug.sethook(function(e)io.write('hook:',e,'\\n')end,'cr')"})
    case('interactive_finalizer_error_escapes',['-i'],b"local p=newproxy(true);getmetatable(p).__gc=function()error('gc failed')end;p=nil;error('original')\n=17\n")
    case('interactive_missing_print',['-i'],b'=17\n',env={'LUA_INIT':'print=nil'})
    case('interactive_print_binary_error',['-i'],b'=17\n',env={'LUA_INIT':"print=function()error('a\\000b',0)end"})
    case('interactive_prompt_fallbacks',['-i'],b'if true then\nend\n',env={'LUA_INIT':'_PROMPT=false;_PROMPT2={}'})
    case('interactive_returned_closure_survives_lookup_gc',['-i'],b'=function()return 42 end\n',env={'LUA_INIT':"print=nil;setmetatable(_G,{__index=function(t,k)if k=='print' then collectgarbage();return function(f)io.write(f(),'\\n')end end end})"})
    case('interactive_command_then_console',['-i','-ex=9'],b'=x\n')
    case('interactive_repeated_flag',['-i','-i','-v'],b'=9\n')
    version=tomllib.loads((root/'crates/deadlib-lua/Cargo.toml').read_text())['package']['version']
    newline=b'\r\n' if os.name=='nt' else b'\n'
    banners={'reference':b'Lua 5.1.5  Copyright (C) 1994-2012 Lua.org, PUC-Rio'+newline,
             'rust':f'DeadLua {version} (Lua 5.1)'.encode()+newline}
    results=[]
    with tempfile.TemporaryDirectory(prefix='deadlua-cli-') as temporary:
        directory=Path(temporary)
        for item in cases:
            row={k:v for k,v in item.items() if k not in ('stdin','files')}
            row['stdin_hex']=item['stdin'].hex()
            row['files']={k:v.hex() for k,v in item['files'].items()}
            for engine,executable in [('reference',reference),('rust',binary)]:
                work=directory/item['name']/engine
                work.mkdir(parents=True)
                for name,contents in item['files'].items():(work/name).write_bytes(contents)
                env=os.environ.copy()
                for name in ('LUA_INIT','LUA_PATH','LUA_CPATH'):env.pop(name,None)
                env['LUA_PATH']='?.lua;./?.lua'
                env.update(item['environment'])
                try:
                    run=subprocess.run(['lua',*item['arguments']],executable=executable,cwd=work,input=item['stdin'],env=env,capture_output=True,timeout=10)
                    row[engine]=dict(returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex(),
                                     files={p.relative_to(work).as_posix():p.read_bytes().hex() for p in sorted(work.rglob('*')) if p.is_file()})
                except subprocess.TimeoutExpired:row[engine]=dict(timeout=True)
            row['equal']=row['reference']==row['rust']
            def without_banner(engine):
                observation=row[engine].copy()
                raw=bytes.fromhex(observation.get('stderr_hex',''))
                if raw.startswith(banners[engine]):observation['stderr_hex']=raw[len(banners[engine]):].hex()
                return observation
            row['equal_without_version_banner']=without_banner('reference')==without_banner('rust')
            row['version_banner_only']=not row['equal'] and row['equal_without_version_banner']
            results.append(row)
            print(item['name'],'pass' if row['equal'] else 'banner only' if row['version_banner_only'] else 'FAIL',flush=True)
    if hashes()!=before:raise SystemExit('sources changed during CLI audit')
    result=dict(source_hashes=before,reference=metadata,rust_binary=dict(path=str(binary),sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),prebuilt=args.rust_binary is not None),cases=results,passed=sum(r['equal'] for r in results),
                equal_without_version_banner=sum(r['equal_without_version_banner'] for r in results),version_banners={k:v.hex() for k,v in banners.items()},
                limitations='Fixed isolated CLI audit. Raw comparisons retain all banner differences. An additional comparison removes at most one exact known leading version banner from stderr. ASCII Windows argv/path contract; terminal/signal and broader diagnostic semantics remain required.')
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(result,indent=2)+'\n')
    print(f"{result['passed']}/{len(results)} CLI probes match")
    if result['passed']!=len(results):raise SystemExit(1)

if __name__=='__main__':main()
