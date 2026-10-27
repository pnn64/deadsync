"""Compare fixed Lua 5.1 process probes in isolated Rust/reference children."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


def lua(text: str) -> str:
    return lua_bytes(text.encode('utf-8'))


def lua_bytes(data: bytes) -> str:
    return '"' + ''.join(chr(b) if 32 <= b < 127 and b not in (34, 92)
                         else '\\"' if b == 34 else '\\\\' if b == 92
                         else f'\\{b:03d}' for b in data) + '"'


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--extra-rust-binary', type=Path, help='Retain an additional implementation comparison without weakening the three-engine checks')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    sources = list((root/'crates/deadlib-lua').rglob('*.rs'))
    sources += [root/'crates/deadlib-lua/Cargo.toml', Path(__file__).resolve()]
    sources += [root/'docs/testing/lua51-gc-finalizer-phase.lua']
    def hashes() -> dict[str, str]:
        return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in sorted(sources)}
    before = hashes()
    command = ['cargo', 'build', '--release', '-p', 'deadlib-lua', '--example', 'os_process', '--bin', 'deadlua']
    subprocess.run(command, cwd=root, check=True)
    extension = '.exe' if os.name == 'nt' else ''
    target = (root/Path(os.environ.get('CARGO_TARGET_DIR','target'))).resolve()
    binary = target/f'release/examples/os_process{extension}'
    cli = target/f'release/deadlua{extension}'
    fixture_root = root/'target/lua-process-fixtures'
    fixture_root.mkdir(parents=True, exist_ok=True)
    results = []
    cases: list[tuple[str,str,dict[str,str|None]]] = []
    def case(name: str, source: str, env: dict[str,str|None]|None=None) -> None:
        cases.append((name,source,env or {}))
    native_source = """
        local a,b=...;local x=4
        local function f(n)x=x+n;return x end
        local t={a,b,'\\r\\n\\026'};local sum=0
        for k,v in ipairs(t)do if type(v)=='number' then sum=sum+k*v end end
        return f(sum),f(-1),#t[3],debug.getinfo(1,'S').source
    """
    with tempfile.TemporaryDirectory(prefix='lua-native-') as temporary:
        source_path=Path(temporary)/'source.lua'
        source_path.write_text(native_source,encoding='utf-8')
        native = subprocess.run([str(binary),'dump',str(source_path)],capture_output=True,check=True).stdout
    literal=lua_bytes(native)
    case('concat_ranges', """
        local trace={};local mt={};local weak=setmetatable({},{__mode='v'})
        mt.__concat=function(a,b)
          collectgarbage('collect');assert(weak[1]~=nil)
          trace[#trace+1]=a.s..':'..b.s;return setmetatable({s=a.s..b.s},mt)
        end
        local function f(i)local t=setmetatable({s=tostring(i)},mt);weak[i]=t;return t end
        local r=f(1)..f(2)..f(3)..f(4);print(r.s,table.concat(trace,'|'))
        local source="return 'a'..1 ..2 ..3,'a'..(false and 'b'..'c' or 'd')..'e'"
        local g=assert(loadstring(string.dump(assert(loadstring(source)))))
        print(g())
    """)
    case('concat_hooks', """
        local f=assert(loadstring('return function(a,b,c)\\nreturn a\\n ..b\\n ..c\\nend'))()
        local trace={}
        debug.sethook(function(event,line)
          local d=debug.getinfo(2,'fl')
          if d.func==f then trace[#trace+1]=event..':'..tostring(line)..':'..d.currentline end
        end,'l',1)
        local result=f('a',1,2);debug.sethook();print(result,table.concat(trace,'|'))
    """)
    case('dump_long_branches', """
        local body=string.rep('x=x+1;',120000)
        for _,source in ipairs({'local x=0;if x==1 then '..body..'end;return x',
          'local x=0;for i=1,2 do '..body..'end;return x'})do
          local f=assert(loadstring(source,'=long branch'));local s=string.dump(f)
          local g=assert(loadstring(s));print(g(),s==string.dump(g))
        end
    """)
    case('dump_relay_scope', """
        local source='local x=0\\nif x==1 then\\nlocal hidden=9\\n'..string.rep('x=x+1\\n',80000)..'end\\nreturn x'
        local original=assert(loadstring(source,'=relay scope'));local f=assert(loadstring(string.dump(original)))
        local valid=true;local visits=0
        debug.sethook(function()
          local d=debug.getinfo(2,'fl');if d.func==f then
            visits=visits+1;local name=debug.getlocal(2,2)
            if name=='hidden' or (d.currentline>2 and d.currentline<80004) then valid=false end
          end
        end,'',1)
        local result=f();debug.sethook();print(result,valid,visits>0)
    """)
    case('dump_roundtrip', """
        local make=assert(loadstring('local x=7;return function(a)return x+a end','=dump fixture'))
        local f=make();local s=string.dump(f,true);print(string.byte(s,1,12))
        local g=assert(loadstring(s,'=ignored'));print(debug.getupvalue(g,1));debug.setupvalue(g,1,11)
        print(g(3),f(3),string.dump(g)==string.dump(g,false),debug.getinfo(g,'S').source)
        local source='return function(a,...)return a,arg.n,arg[1]end'
        local h=assert(loadstring(source))();print(assert(loadstring(string.dump(h)))(1,2,3))
    """)
    case('dump_native_files', f"local original={literal};"+"""
        local f=assert(loadstring(original));local s=string.dump(f)
        local file=assert(io.open('output.dat','wb'));file:write('#!lua\\r\\n',s);file:close()
        local g=assert(loadfile('output.dat'));print(g(3,5));print(s==string.dump(g))
        local at=0;local h=assert(load(function()at=at+1;return s:sub(at,at)end));print(at,#s,h(7,8))
    """)
    case('dump_errors_hooks', """
        for _,args in ipairs({{},{false},{true},{3},{'abc'},{{}},{print},{string.dump}})do print(pcall(string.dump,unpack(args)))end
        local function f()return 4 end
        debug.sethook(function(e)
          if e=='return' and debug.getinfo(2,'f').func==string.dump then
            for i=1,4 do local n,v=debug.getlocal(2,i);print(n,type(v))end
            collectgarbage()
          end
        end,'r')
        local s=string.dump(f,newproxy(),newproxy());debug.sethook();print(assert(loadstring(s))())
    """)
    case('dump_generated', """
        local hash=0
        for n=1,32 do
          local source='return function(a,...)local out={...};local sum=0;for k,v in ipairs(out)do local x=v+a;sum=sum+(function()return x end)() end;return sum,'..n..' end'
          local f=assert(loadstring(source))();local g=assert(loadstring(string.dump(f)))
          local x,y=g(n,2,3,4);hash=(hash*31+x+y)%10000019
        end
        print(hash)
    """)
    case('binary_execute', f"local s={literal};local f=assert(loadstring(s,'=ignored'));print(f(3,5));collectgarbage();print(f(7,11));print(debug.getinfo(f,'S').source)")
    case('binary_execute_readers', f"local s={literal};"+"""
        for width=1,7 do local at,calls=0,0
          local f=assert(load(function()calls=calls+1;collectgarbage();local b=s:sub(at+1,at+width);at=at+#b;return b end,'=ignored'))
          print(width,calls,at,#s,f(3,5))
        end
        local s=s..'unused';local at=0
        local f=assert(load(function()at=at+1;return s:sub(at,at)end));print(at,#s-6,f(2,4))
    """)
    case('binary_execute_files',f"local s={literal};"+"""
        for _,prefix in ipairs({'','#!/usr/bin/lua\\r\\n'})do
          local file=assert(io.open('output.dat','wb'));file:write(prefix,s);file:close()
          local f=assert(loadfile('output.dat'));print(f(3,5));print(dofile('output.dat'))
        end
    """)
    case('shell_query', "print(os.execute(),os.execute(nil),os.execute(nil,false),select('#',os.execute()))")
    binary_header = "local h='\\27Lua'..string.char(81,0,1,4,8,4,8,0);"
    case('binary_header_errors', binary_header+"""
        for _,s in ipairs({'\\27',h:sub(1,11),h:sub(1,4)..'Z'..h:sub(6),
          h..string.rep('\\0',8)..string.rep('\\255',4)})do
          print(loadstring(s,'=probe'));print(loadstring(s))
        end
    """)
    case('binary_reader_errors',binary_header+"""
        local s=h..string.rep('\\0',8)..string.rep('\\255',4)..'unused'
        for size=1,4 do local at,calls=0,0
          local f,e=load(function()calls=calls+1;local b=s:sub(at+1,at+size);at=at+#b;return b end,'=probe')
          print(f,e,calls,at)
        end
        local marker={};local calls=0
        local ok,f,e=pcall(load,function()calls=calls+1;if calls==1 then return '\\27Lua'else error(marker,0)end end)
        print(ok,f,e==marker,calls)
    """)
    case('binary_file_errors', """
        local bytes='\\27LuaQ\\26'..string.char(1,4,8,4,8,0)
        for _,prefix in ipairs({'','#!/usr/bin/lua\\r\\n','# early \\27junk\\n'})do
          local f=assert(io.open('output.dat','wb'));f:write(prefix,bytes);f:close()
          print(loadfile('output.dat'));print(pcall(dofile,'output.dat'))
        end
    """)
    if os.name == 'nt':
        case('locale_queries', """
            assert(os.setlocale('C'));print(os.setlocale(),os.setlocale(nil,'numeric'))
            for _,category in ipairs({'all','collate','ctype','monetary','numeric','time'})do
              print(category,os.setlocale('C',category),os.setlocale(nil,category))
            end
            for _,args in ipairs({{false},{true,'bad'},{'C','bad'},{'C',false},{'C','NUMERIC'},
              {'a locale which does not exist'},{'C\\0ignored','numeric\\0ignored'}})do
              print(pcall(os.setlocale,unpack(args)))
            end
        """)
        for name in ('ptb','fr-FR','.UTF-8','trk'):
            setup = f"assert(os.setlocale('C'));local name={lua(name)};print(assert(os.setlocale(name)));"
            case('locale_numeric_'+name, setup+"""
                for _,text in ipairs({'3.4','3,4','  3,4  ','0x1.8p1','0x1,8p1','0x1p1',
                  '1e3','1,25e-3','-0,0','nan','inf','-inf','1.2,3','3\\0tail'})do
                  local n=tonumber(text);print(text,n,n and 1/n)
                end
                print(tostring(3.4),string.format('%08.2f|%+.3e|%#.0f|%.4g',3.4,12.5,4,0.00125))
                print('concat',3.4 .. '|' .. table.concat({3.4,2.5},';'))
                print(assert(loadstring('return 3.4,.4,4.,4.+.5'))())
                local f,e=loadstring('return 4.5.');print(f==nil,e)
                print(tonumber(string.char(160)..'42'..string.char(160)),tonumber(string.char(160)..'ff'..string.char(160),16))
                os.setlocale('C')
            """)
            case('locale_bytes_'+name, setup+"""
                local bytes={};for i=0,255 do bytes[#bytes+1]=string.char(i)end;bytes=table.concat(bytes)
                local function encode(s)
                  local out={};for i=1,#s do out[i]=string.format('%02x',string.byte(s,i))end
                  return table.concat(out)
                end
                print('lower',encode(string.lower(bytes)));print('upper',encode(string.upper(bytes)))
                for class in string.gmatch('aAcCdDlLpPsSuUwWxXzZ','.')do
                  local value,n=string.gsub(bytes,'%'..class,'*');print(class,encode(value),n)
                  value,n=string.gsub(bytes,'[%'..class..']','*');print(class,encode(value),n)
                  value,n=string.gsub(bytes,'[^%'..class..']','*');print(class,encode(value),n)
                end
                for _,c in ipairs({128,160,178,192,201,233,253,255})do
                  local byte=string.char(c);local f,e=loadstring('local '..byte..'=7;return '..byte)
                  print(c,f and f()or e)
                end
                os.setlocale('C')
            """)
        case('locale_pattern_invalidation', """
            assert(os.setlocale('C'));local c=string.char(233);local p='%a'
            print(string.match(c,p));local it=string.gmatch('a'..c..'b',p)
            print(it());assert(os.setlocale('ptb','ctype'));print(it());print(string.match(c,p))
            assert(os.setlocale('C','ctype'));print(it())
            print(string.gsub('a'..c..'b',p,function(x)
              if x=='a'then assert(os.setlocale('ptb','ctype'));string.match(c,p)end;return 'x'
            end))
            print(string.gsub('a'..c..'b','^%a',function(x)
              assert(os.setlocale('C','ctype'));string.match(c,p);return 'X'
            end))
            assert(os.setlocale('C'))
        """)
        case('locale_lexer_bytes', """
            for _,name in ipairs({'C','ptb','.UTF-8'})do
              assert(os.setlocale(name));print(name)
              for i=128,255 do
                local c=string.char(i)
                for _,source in ipairs({'local '..c..'=7;return '..c,'local a'..c..'=8;return a'..c,
                  'return "\\\\'..c..'"','return "\\\\1'..c..'"'})do
                  local f,e=loadstring(source)
                  if f then local v=f();print(i,type(v),type(v)=='string'and string.byte(v)or v)
                  else print(i,e)end
                end
              end
            end
            assert(os.setlocale('C'))
        """)
        case('locale_category_independence', """
            assert(os.setlocale('C'));assert(os.setlocale('ptb','numeric'))
            print(os.setlocale(nil,'all'));print(os.setlocale(nil,'ctype'),os.setlocale(nil,'numeric'))
            print(string.upper(string.char(233)),tonumber('3,4'),tonumber('3.4'))
            assert(os.setlocale('fr-FR','time'));print(os.date('!%A|%B|%x|%X',0))
            assert(os.setlocale('ptb','ctype'));print(os.setlocale(nil,'all'))
            assert(os.setlocale('C'));print(os.setlocale())
        """)
        case('locale_collation', """
            local strings={'','a','z',string.char(225)..'lo','alo','amo','A','a\\0b','A\\0c','a\\0','a\\0\\0'}
            for _,name in ipairs({'C','ptb','fr-FR','.UTF-8'})do
              assert(os.setlocale(name,'collate'));print(name)
              for i,a in ipairs(strings)do for j,b in ipairs(strings)do print(i,j,a<b,a<=b,a==b)end end
            end
            os.setlocale('C')
        """)
        case('locale_stream_numbers', """
            assert(os.setlocale('C'));assert(os.setlocale('ptb'))
            local f=assert(io.tmpfile());f:write(' 3,4|0x1,8p1|5.6|'..string.char(160)..'42|')
            f:seek('set')
            for i=1,4 do print(f:read('*n'),f:read(1))end;print(f:read('*a'));f:close()
            f=assert(io.tmpfile());f:write(3.4,'|',string.format('%.2f',2.5));f:seek('set');print(f:read('*a'));f:close()
            assert(os.setlocale('C'))
        """)
        case('locale_digit_grammar', """
            for _,name in ipairs({'C','ptb','.UTF-8','trk'})do
              assert(os.setlocale(name));print(name)
              for _,code in ipairs({178,179,185})do
                local c=string.char(code)
                print(pcall(string.match,'x','%'..c))
                for _,middle in ipairs({c,'2'..c,c..'2','.'..c,c..'.2',c..c..c})do
                  for _,conversion in ipairs({'d','f','s','c','q'})do
                    local format='%'..middle..conversion
                    print(code,format,pcall(string.format,format,65))
                  end
                end
              end
            end
            assert(os.setlocale('C'))
        """)
    gc_setup = """
        collectgarbage('restart');collectgarbage('setpause',200);collectgarbage();collectgarbage('stop')
        local weak=setmetatable({},{__mode='v'});local keep={}
        for i=1,1000 do local t={};weak[i]=t;keep[i]=t end
        local function pressure()
          keep=nil;for i=1,10000 do local t={}end;return next(weak)==nil
        end
    """
    for name, operation in (
        ('gc_full_restart', "collectgarbage();print(pressure())"),
        ('gc_explicit_stop', "collectgarbage();collectgarbage('stop');print(pressure())"),
        ('gc_step0_restart', "collectgarbage('step',0);print(pressure())"),
        ('gc_step_big_restart', "collectgarbage('step',1000000);print(pressure())"),
        ('gc_restart_large_pause', "collectgarbage('setpause',2000000000);collectgarbage();collectgarbage('stop');collectgarbage('restart');print(pressure())"),
        ('gc_setpause_deferred', "collectgarbage();collectgarbage('setpause',2000000000);print(pressure())"),
        ('gc_finalizer_stop_restore', "local hit=false;local p=newproxy(true);getmetatable(p).__gc=function()hit=true;collectgarbage('stop')end;p=nil;collectgarbage();print(hit,pressure())"),
        ('gc_finalizer_error_restart', "local tag={};local p=newproxy(true);getmetatable(p).__gc=function()error(tag,0)end;p=nil;local ok,e=pcall(collectgarbage);print(ok,e==tag,pressure())"),
        ('gc_finalizer_error_stop', "local tag={};local p=newproxy(true);getmetatable(p).__gc=function()collectgarbage('stop');error(tag,0)end;p=nil;local ok,e=pcall(collectgarbage);print(ok,e==tag,pressure())"),
        ('gc_hook_return_stop', "local hit=0;debug.sethook(function(e)if e=='return' and debug.getinfo(2,'f').func==collectgarbage then hit=hit+1;collectgarbage('stop')end end,'r');collectgarbage();debug.sethook();print(hit,pressure())"),
        ('gc_invalid_stays_stopped', "local ok,e=pcall(collectgarbage,'restart',{});print(ok,type(e),pressure())"),
        ('gc_finalizer_coroutine', "local co;local p=newproxy(true);getmetatable(p).__gc=function()co=coroutine.create(function()collectgarbage('stop');coroutine.yield(7);collectgarbage('restart');return 11 end);local ok,n=coroutine.resume(co);assert(ok and n==7)end;p=nil;collectgarbage();local automatic=pressure();local ok,n=coroutine.resume(co);print(automatic,ok,n,coroutine.status(co))"),
        ('gc_zero_multiplier', "collectgarbage('setstepmul',0);print(collectgarbage('step',0),pressure())"),
        ('gc_negative_step_budget', "print(collectgarbage('step',-1),pressure())"),
    ):
        case(name, gc_setup+operation)
    case('gc_finalizer_phase', (root/'docs/testing/lua51-gc-finalizer-phase.lua').read_text(encoding='utf-8'))
    case('gc_mark_mutation', """
        collectgarbage('stop');local roots={};local weak=setmetatable({},{__mode='v'})
        for i=1,512 do roots[i]={i};weak[i]={i}end
        collectgarbage('step',0)
        for i=1,512 do local v=weak[i] or{i};roots[i].child=v
          if i%8==0 then collectgarbage('step',0)end
        end
        collectgarbage();local n=0
        for i=1,512 do n=n+roots[i].child[1]end;print(n)
    """)
    case('gc_mark_graph_mutation', """
        collectgarbage('stop');local step=collectgarbage;local final=0
        local tree={};local weak=setmetatable({},{__mode='v'})
        for i=1,512 do weak[i]={value=i}end
        for i=1,64 do
          step('step',0)
          local state=weak[i] or {value=i};local obj={state=state}
          obj.call=function()return state end;obj.env=function()return number end
          setfenv(obj.env,{number=i})
          obj.co=coroutine.create(function()coroutine.yield(state);return state end)
          local ok,v=coroutine.resume(obj.co);assert(ok and v==state)
          obj.guard=newproxy(true);getmetatable(obj.guard).__gc=function()final=final+1 end
          debug.setfenv(obj.guard,{number=i})
          setmetatable(obj,{__index={number=i}});tree[i]=obj
          step('step',0)
        end
        step('collect');local sum=0
        for i,obj in ipairs(tree)do
          local ok,v=coroutine.resume(obj.co)
          assert(ok and v==obj.state and obj.call()==v)
          assert(obj.env()==i and obj.number==i and debug.getfenv(obj.guard).number==i)
          sum=sum+v.value
        end
        local before=final;tree=nil;step('collect');print(sum,before,final)
    """)
    case('gc_sweep_churn', """
        collectgarbage('stop');local held={}
        for i=1,3000 do local t={'garbage'..i}end
        for i=1,500 do local s=tostring(i);held[i]={s=s,call=function()return s end}
          if i%7==0 then collectgarbage('step',0)end
        end
        collectgarbage();local n=0
        for i=1,500 do assert(held[i].call()==tostring(i));n=n+#held[i].s end
        print(n,#held)
    """)
    case('calendar_native_zone', "print(os.date('%Z|%z',0),os.date('!%Z|%z',0));print(type(os.time()),type(os.date()),select('#',os.time(nil,false)))")
    for index, fmt in enumerate(('%Q','%E','%O','%#d','%','%n','%t','%F','%g','%V','%s','%10Y')):
        case(f'calendar_format_{index}',f"print('before');print(os.date({lua(fmt)},0));print('after')")
    case('calendar_invalid_time_before_format', "print(os.date('%Q',math.huge),os.date('%#d',0/0),os.date('!*t',-9223372036854775808))")
    timezone_source="""
        local dates={{year=2024,month=3,day=10,hour=2,min=30},
          {year=2024,month=11,day=3,hour=1,min=30},
          {year=2000,month=2,day=30,hour=25,sec=60},
          {year=1969,month=12,day=31,hour=23,min=59,sec=59}}
        for _,date in ipairs(dates)do for _,hint in ipairs({{},{value=false},{value=true}})do
          date.isdst=hint.value;local n=os.time(date)
          print(tostring(n),date.year,date.month,date.day,date.hour,tostring(date.isdst))
          if n then print(os.date('%Y-%m-%d %H:%M:%S|%z|%Z',n),os.date('!%Y-%m-%d %H:%M:%S|%z|%Z',n))end
        end end
    """
    for timezone in ('UTC0','EST5EDT','CET-1CEST'):
        case('calendar_timezone_'+timezone,timezone_source,{'TZ':timezone})
    case('clock_contexts', """
        local clock=os.clock;local a=clock();local t={clock=clock}
        local c=coroutine.create(function()return t:clock(false,{},nil)end)
        local ok,b=coroutine.resume(c)
        print(type(a),ok,type(b),b>=a,select('#',clock(false)),debug.getinfo(clock).what)
    """)
    case('rename_numeric_names', """
        for _,pair in ipairs({{42,43},{1.5,2.25}}) do
          local a,b=pair[1],pair[2];local f=assert(io.open(tostring(a),'wb'))
          f:write('contents');f:close()
          print(select('#',os.rename(a,b,false,{})))
          f=assert(io.open(tostring(b),'rb'));print(f:read('*a'));f:close()
          print(os.rename(b,a));assert(os.remove(tostring(a)))
        end
    """)
    for status in (0,1,7,127,255,256,-1,2147483647,-2147483648):
        shell = f'exit /b {status}' if os.name == 'nt' else f'exit {status}'
        case(f'execute_status_{status}',f"print(os.execute({lua(shell)}),select('#',os.execute({lua(shell)})))")
    for index, text in enumerate(('', 'echo alpha', 'echo "a b"', 'echo left & echo right',
                                   'echo a^&b' if os.name == 'nt' else "printf 'a&b\\n'",
                                   'echo first\x00 & echo unreachable',
                                   'echo one\r\necho two')):
        case(f'execute_text_{index}', f'print("status",os.execute({lua(text)}))')
    case('process_argument_errors', """
        for _,f in ipairs({os.execute,os.exit}) do
          for _,v in ipairs({false,{},function()end}) do
            local ok,e=pcall(f,v);print(ok,e)
          end
        end
        local t={execute=os.execute,exit=os.exit}
        local ok,e=pcall(function()return t:execute()end);print(ok,e)
        local ok,e=pcall(function()return t:exit()end);print(ok,e)
    """)
    for code in ('nil','0','1','7','255','256','-1','.9','1.9','-1.9',
                 '2147483648','4294967299','0/0','math.huge','-math.huge',"'3.9'"):
        case('exit_'+code, f"print('before');os.exit({code});print('unreachable')")
    case('exit_no_arguments', "io.write('before');os.exit();print('unreachable')")
    case('exit_bypasses_protected_call', "print('before');pcall(os.exit,7);print('unreachable')")
    case('exit_bypasses_xpcall', "xpcall(function()os.exit(3)end,function()print('handler')end);print('unreachable')")
    case('exit_bypasses_finalizers', """
        local p=newproxy(true);getmetatable(p).__gc=function()print('finalizer')end
        io.write('before');os.exit(4)
    """)
    case('exit_flushes_open_files', """
        local a=assert(io.open('output.dat','wb'));assert(a:setvbuf('full',8192))
        local b=assert(io.open('other.dat','w'));assert(b:setvbuf('full',8192))
        assert(a:write('first\\0last'));assert(b:write('line1\\nline2'))
        io.write('stdout');io.stderr:write('stderr');os.exit(5)
    """)
    case('exit_flushes_collected_or_closed_streams', """
        local f=assert(io.open('output.dat','w'));f:write('first');f:close()
        local f=assert(io.open('other.dat','w'));f:write('second');f=nil
        collectgarbage();os.exit(0)
    """)
    shell = 'exit /b 9' if os.name == 'nt' else 'exit 9'
    case('execute_does_not_flush_named_files', f"""
        local f=assert(io.open('output.dat','wb'));f:setvbuf('full',8192);f:write('buffered')
        print('status',os.execute({lua(shell)}))
        local g=assert(io.open('output.dat','rb'));print('size',#g:read('*a'));g:close()
        f:close()
    """)
    case('execute_preserves_stdout_order', "print('before');os.execute('echo child');print('after')")
    for count in (4095,4096,4097,8193):
        case(f'stdout_boundary_{count}',f"io.write(string.rep('p',{count}));os.execute('echo child');io.flush()")
    for mode in ('no','full','line'):
        for count in (15,16,17):
            case(f'stdout_{mode}_{count}',f"io.stdout:setvbuf('{mode}',16);print(string.rep('p',{count}));os.execute('echo child');io.flush()")
    case('stdout_gc_before_execute_exit', "print('before');io.stdout=nil;io.output(io.tmpfile());collectgarbage();os.execute('echo child');os.exit(0)")
    case('exit_from_coroutine', "local c=coroutine.create(function()os.exit(6)end);coroutine.resume(c);print('unreachable')")
    case('exit_from_hook', "debug.sethook(function()os.exit(8)end,'c');print('unreachable')")
    with tempfile.TemporaryDirectory(prefix='run-', dir=fixture_root) as temporary:
        directory = Path(temporary)
        if os.name == 'nt':
            cmd = Path(os.environ['SystemRoot'])/'System32/cmd.exe'
            shutil.copyfile(cmd,directory/'local-shell.exe')
            shutil.copyfile(cmd,directory/'local-binary.cmd')
            (directory/'local-shell').write_bytes(b'not executable')
            (directory/'invalid.exe').write_bytes(b'not executable')
            (directory/'invalid.cmd').write_bytes(b'@exit /b 23\r\n')
            (directory/'directory.exe').mkdir()
            spaced=directory/'space; shell'
            spaced.mkdir()
            shutil.copyfile(cmd,spaced/'cmd.exe')
            query="print('query',os.execute());print('status',os.execute('exit /b 9'))"
            for name, value in (('unset',None),('empty',''),('missing',str(directory/'missing.exe')),
                                ('directory',str(directory/'directory.exe')),
                                ('invalid',str(directory/'invalid.exe')),
                                ('batch',str(directory/'invalid.cmd')),
                                ('binary_cmd',str(directory/'local-binary.cmd')),
                                ('relative','local-shell.exe'),('extensions','local-shell')):
                case('comspec_'+name,query,{'COMSPEC':value})
            for name,path in (('empty',''),('quoted',f'"{spaced}"'),
                              ('leading_empty',f';"{spaced}"'),
                              ('double_empty',f'nonexistent;;"{spaced}"')):
                case('path_'+name,query,{'COMSPEC':None,'PATH':path})
            case('shell_commandline_metadata', "print('status',os.execute('echo %CMDCMDLINE%'))")
            case('shell_relative_metadata', "print('status',os.execute('echo %CMDCMDLINE%'))", {'COMSPEC':'local-shell.exe'})
            case('shell_fallback_metadata', "print('status',os.execute('echo %CMDCMDLINE%'))", {'COMSPEC':None,'PATH':f'"{spaced}"'})
        for name, source, env_changes in cases:
            script=directory/'probe.lua'
            script.write_bytes(source.encode('utf-8'))
            env=os.environ.copy()
            env.pop('LUA_INIT',None)
            for key,value in env_changes.items():
                if value is None:env.pop(key,None)
                else:env[key]=value
            row={'name':name,'source':source,'env_changes':env_changes}
            engines=('reference','rust','cli') + (('extra',) if args.extra_rust_binary else ())
            for engine in engines:
                if engine=='extra' and name=='comspec_binary_cmd':
                    row[engine]={'not_run':'The historical std launcher would parse a PE executable as batch-file commands. Native/reference/CLI checks still run.'}
                    continue
                for filename in ('output.dat','other.dat'):
                    (directory/filename).unlink(missing_ok=True)
                started=time.monotonic()
                if engine=='extra':invocation=[str(args.extra_rust_binary.resolve()),str(script)]
                elif engine=='cli':invocation=[str(cli),script.name]
                else:invocation=[str(binary),engine,str(script)]
                try:
                    run=subprocess.run(invocation,cwd=directory,env=env,capture_output=True,timeout=10)
                    row[engine]=dict(returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex(),
                                     seconds=time.monotonic()-started,
                                     files={name:(directory/name).read_bytes().hex() if (directory/name).exists() else None
                                            for name in ('output.dat','other.dat')})
                except subprocess.TimeoutExpired:
                    row[engine]={'timeout':True,'seconds':time.monotonic()-started}
            def observations(engine: str) -> dict:
                return {k:v for k,v in row[engine].items() if k!='seconds'}
            row['equal']=all('returncode' in row[e] for e in ('reference','rust','cli')) and observations('reference')==observations('rust')==observations('cli')
            if args.extra_rust_binary:
                row['extra_equal']=observations('reference')==observations('extra')
            results.append(row)
            print(name,'pass' if row['equal'] else 'FAIL',flush=True)
            if not row['equal']:
                for engine in ('reference','rust','cli'):
                    print(engine,observations(engine),flush=True)
    if before != hashes():
        raise SystemExit('sources changed during process probes')
    output=args.output.resolve()
    output.parent.mkdir(parents=True,exist_ok=True)
    extra=None if not args.extra_rust_binary else dict(path=str(args.extra_rust_binary.resolve()),sha256=hashlib.sha256(args.extra_rust_binary.read_bytes()).hexdigest(),passed=sum(r['extra_equal'] for r in results),tested=sum('not_run' not in r['extra'] for r in results),skipped=[r['name'] for r in results if 'not_run' in r['extra']])
    output.write_text(json.dumps(dict(command=command,source_hashes=before,platform=sys.platform,extra_binary=extra,
                                      cases=results,passed=sum(r['equal'] for r in results),
                                      limitations='Fixed isolated process probes; ASCII Windows environment/path contract. Not a full OS-library or host-process resource audit.'),indent=2)+'\n',encoding='utf-8')
    print(f"{sum(r['equal'] for r in results)}/{len(results)} process probes pass; {output}")
    if any(not r['equal'] for r in results):raise SystemExit(1)


if __name__=='__main__':
    main()
