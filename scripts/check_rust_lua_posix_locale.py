"""Compare Linux/glibc locale behavior with the pinned standalone Lua 5.1 oracle."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import time
from build_lua_cli_reference import build


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--run-root', required=True, type=Path)
    args = parser.parse_args()
    if not sys.platform.startswith('linux'):
        raise SystemExit('This oracle matrix requires Linux/glibc and localedef data.')
    root = Path(__file__).resolve().parents[1]
    sources = list((root/'crates/deadlib-lua').rglob('*.rs'))
    sources += [root/'crates/deadlib-lua/Cargo.toml', Path(__file__).resolve(), root/'scripts/build_lua_cli_reference.py']
    def hashes():
        return {p.relative_to(root).as_posix(): sha(p) for p in sorted(sources)}
    before = hashes()
    target = Path(os.environ.get('CARGO_TARGET_DIR', str(root/'target'))).resolve()
    subprocess.run(['cargo','build','--release','-p','deadlib-lua','--bin','deadlua'],cwd=root,check=True)
    reference, metadata = build(root)
    directory = args.run_root.resolve()/str(time.time_ns())
    directory.mkdir(parents=True)
    abi_source = '''#define _GNU_SOURCE
#include <locale.h>
#include <langinfo.h>
#include <stddef.h>
#include <stdio.h>
#include <time.h>
int main(void) {
  printf("%d %d %d %d %d %d %d %d %d %d %zu %zu %zu %zu %zu\\n",
    LC_ALL, LC_COLLATE, LC_CTYPE, LC_MONETARY, LC_NUMERIC, LC_TIME,
    LC_ALL_MASK, RADIXCHAR, _NL_LOCALE_NAME(LC_CTYPE),
    _NL_LOCALE_NAME(LC_IDENTIFICATION), sizeof(int), sizeof(locale_t),
    sizeof(struct tm), offsetof(struct tm, tm_gmtoff), offsetof(struct tm, tm_zone));
  return 0;
}
'''
    abi_file = directory/'abi.c';abi_file.write_text(abi_source)
    abi_binary = directory/'abi'
    abi_command = [metadata['command'][0], str(abi_file), '-o', str(abi_binary)]
    subprocess.run(abi_command,check=True,capture_output=True)
    abi_output = subprocess.check_output([abi_binary]).decode().strip()
    width = struct.calcsize('P')
    assert [int(v)for v in abi_output.split()] == [6,3,0,4,1,2,8127,65536,65535,851967,4,width,
                                                  56 if width==8 else 44,40 if width==8 else 36,48 if width==8 else 40]
    headers = [Path('/usr/include')/name for name in ['locale.h','langinfo.h','ctype.h','time.h']]
    abi = dict(source=abi_source,source_sha256=sha(abi_file),command=abi_command,output=abi_output,
               binary=dict(path=str(abi_binary),sha256=sha(abi_binary)),headers={str(p):sha(p)for p in headers})
    locale_root = directory/'locales'
    locale_root.mkdir()
    definitions = [('de_DE','ISO-8859-1','de_DE.iso88591'),
                   ('fr_FR','UTF-8','fr_FR.utf8'), ('tr_TR','ISO-8859-9','tr_TR.iso88599')]
    locale_commands = []
    locale_inputs = {}
    for language, encoding, name in definitions:
        command = ['localedef','--no-archive','-i',language,'-f',encoding,str(locale_root/name)]
        subprocess.run(command,check=True,capture_output=True)
        locale_commands.append(command)
        for p in [Path('/usr/share/i18n/locales')/language,Path('/usr/share/i18n/charmaps')/(encoding+'.gz')]:
            locale_inputs[str(p)] = sha(p)
    locale_data = {str(p):sha(p) for p in sorted(locale_root.rglob('*')) if p.is_file()}
    executables = {}
    for engine, source in [('reference', reference), ('rust', target/'release/deadlua')]:
        binary = directory/engine/'bin/lua'
        binary.parent.mkdir(parents=True)
        shutil.copyfile(source, binary)
        binary.chmod(0o755)
        executables[engine] = binary
    cases = []
    def case(name, source, environment=None):
        cases.append(dict(name=name,source=source,environment=environment or {}))
    case('queries_and_failures', r"""
        assert(os.setlocale('C'));print(os.setlocale(),os.setlocale(nil,'numeric'))
        for _,category in ipairs{'all','collate','ctype','monetary','numeric','time'}do
          print(category,os.setlocale('POSIX',category),os.setlocale(nil,category))
        end
        for _,args in ipairs{{false},{true,'bad'},{'C','bad'},{'C',false},{'C','NUMERIC'},
          {'a locale which does not exist'},{'C\0ignored','numeric\0ignored'}}do
          print(pcall(os.setlocale,unpack(args)))
        end
    """)
    # Programs emit hexadecimal byte observations where encodings can differ.
    encode = r"""
      local function hex(s)return (s:gsub('.',function(c)return string.format('%02x',c:byte())end))end
    """
    names = ['C','POSIX','C.UTF-8',*[item[2] for item in definitions]]
    for name in names:
        setup = 'assert(os.setlocale('+json.dumps(name)+'));'
        case('numeric_'+name, setup+r"""
          for _,text in ipairs{'3.4','3,4',' 3,4 ','0x1.8p1','0x1,8p1','0x1p1',
            '1e3','1,25e-3','-0,0','nan','inf','-inf','1.2,3','3\0tail'}do
            local n=tonumber(text)
            if n and n~=n then print(text,'nan')else print(text,n,n and 1/n)end
          end
          print(tostring(3.4),string.format('%08.2f|%+.3e|%#.0f|%.4g',3.4,12.5,4,0.00125))
          print('concat',3.4 .. '|' .. table.concat({3.4,2.5},';'))
          print(assert(loadstring('return 3.4,.4,4.,4.+.5'))())
          local f,e=loadstring('return 4.5.');print(f==nil,e)
          local f=assert(io.tmpfile());f:write(' 3,4|0x1,8p1|5.6|');f:seek('set')
          for i=1,3 do print(f:read('*n'),f:read(1))end;print(f:read('*a'));f:close()
        """)
        case('bytes_and_lexer_'+name, setup+encode+r"""
          local bytes={};for i=0,255 do bytes[#bytes+1]=string.char(i)end;bytes=table.concat(bytes)
          print('lower',hex(string.lower(bytes)));print('upper',hex(string.upper(bytes)))
          for class in string.gmatch('aAcCdDlLpPsSuUwWxXzZ','.')do
            local text,n=string.gsub(bytes,'%'..class,'*');print(class,hex(text),n)
            text,n=string.gsub(bytes,'[^%'..class..']','*');print(class,hex(text),n)
          end
          for i=128,255 do
            local c=string.char(i)
            for _,source in ipairs{'local '..c..'=7;return '..c,'return "\\'..c..'"'}do
              local f,e=loadstring(source,'=(locale)')
              if f then local v=f();print(i,type(v),type(v)=='string'and hex(v)or v)
              else print(i,hex(e))end
            end
          end
        """)
        case('collation_'+name, setup+r"""
          local strings={'','a','z',string.char(225)..'lo','alo','amo','A','a\0b','A\0c','a\0','a\0\0'}
          for i,a in ipairs(strings)do for j,b in ipairs(strings)do print(i,j,a<b,a<=b,a==b)end end
        """)
        case('calendar_'+name, setup+encode+r"""
          for _,stamp in ipairs{0,946684800,1704067200}do
            print(hex(os.date('!%A|%a|%B|%b|%c|%x|%X|%p|%Y-%m-%d',stamp)))
          end
        """)
        case('error_bytes_'+name, setup+encode+r"""
          for _,mode in ipairs{'r','w',''}do
            local f,e,n=io.open('',mode);print(f==nil,hex(e),n)
          end
          local f,e=loadfile('missing-locale-fixture');print(f==nil,hex(e))
          local ok,e=pcall(io.lines,'missing-locale-fixture');print(ok,hex(e))
        """)
    case('categories_and_restore', r"""
        assert(os.setlocale('C'));assert(os.setlocale('de_DE.iso88591','numeric'))
        print(os.setlocale());print(os.setlocale(nil,'ctype'),os.setlocale(nil,'numeric'))
        print(string.upper(string.char(233)),tonumber('3,4'),tonumber('3.4'))
        assert(os.setlocale('fr_FR.utf8','time'));print(os.date('!%A|%B|%x|%X',0))
        local original=os.setlocale();assert(os.setlocale('tr_TR.iso88599','ctype'))
        print(os.setlocale());print(string.upper('i'),string.lower('I'))
        print(os.setlocale('deadlua-invalid-locale-name')==nil,os.setlocale())
        print(os.setlocale(original));print(os.setlocale());print(os.setlocale('C'))
    """)
    case('pattern_invalidation', r"""
        assert(os.setlocale('C'));local c=string.char(233);local p='%a'
        print(string.match(c,p));local it=string.gmatch('a'..c..'b',p)
        print(it());assert(os.setlocale('de_DE.iso88591','ctype'));print(it());print(string.match(c,p))
        assert(os.setlocale('C','ctype'));print(it())
        print(string.gsub('a'..c..'b',p,function(x)
          if x=='a'then assert(os.setlocale('de_DE.iso88591','ctype'));string.match(c,p)end;return 'x'
        end))
        assert(os.setlocale('C'))
    """)
    for env in [dict(LANG='de_DE.iso88591'), dict(LANG='C',LC_NUMERIC='de_DE.iso88591',LC_TIME='fr_FR.utf8'),
                dict(LC_ALL='tr_TR.iso88599',LANG='de_DE.iso88591')]:
        case('environment_'+str(len(cases)), r"""
          print(os.setlocale(''));print(os.setlocale(nil,'ctype'),os.setlocale(nil,'numeric'),os.setlocale(nil,'time'))
          print(tostring(3.4),os.date('!%A|%B',0))
        """, env)
    results = []
    for item in cases:
        row = dict(name=item['name'], source=item['source'], source_sha256=hashlib.sha256(item['source'].encode()).hexdigest(), environment=item['environment'])
        for engine, binary in executables.items():
            cwd = directory/engine/item['name'];cwd.mkdir()
            program = cwd/'case.lua';program.write_text(item['source'],encoding='utf-8')
            environment = {k:v for k,v in os.environ.items() if not k.startswith('LC_') and k not in ['LANG','LANGUAGE','LUA_INIT','LUA_PATH','LUA_CPATH']}
            environment.update(LANG='C',LOCPATH=str(locale_root))
            environment.update(item['environment'])
            run = subprocess.run(['lua','case.lua'],executable=binary,cwd=cwd,env=environment,capture_output=True,timeout=20)
            assert sha(program) == row['source_sha256']
            row[engine] = dict(returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
        row['equal'] = row['reference'] == row['rust'] and row['reference']['returncode'] == 0
        results.append(row)
        print(item['name'], 'pass' if row['equal'] else 'FAIL',flush=True)
    if before != hashes():raise SystemExit('sources changed during POSIX locale probes')
    result = dict(source_hashes=before,reference=metadata,abi=abi,localedef_commands=locale_commands,
                  locale_inputs=locale_inputs,locale_data=locale_data,executables={k:dict(path=str(p),sha256=sha(p))for k,p in executables.items()},
                  cases=results,passed=sum(r['equal']for r in results),
                  limitations='Linux/glibc standalone process comparisons with private generated locales. Finite values, signed zero, binary strings, source diagnostics and errno bytes are compared directly; NaN number observations use a classification label, excluding payloads. The Rust engine shares owned locale objects between VMs; host process-global libc locale mutation and arbitrary external native bindings remain outside this proof. Other POSIX libc ABIs remain unsupported. This matrix is correctness coverage, not complete Lua 5.1 parity.')
    args.output.resolve().parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(result['passed'],'/',len(results),'locale programs match',flush=True)
    if not all(r['equal']for r in results):raise SystemExit(1)


if __name__ == '__main__':main()
