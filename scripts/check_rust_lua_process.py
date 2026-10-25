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
    return '"' + ''.join(chr(b) if 32 <= b < 127 and b not in (34, 92)
                         else '\\"' if b == 34 else '\\\\' if b == 92
                         else f'\\{b:03d}' for b in text.encode('utf-8')) + '"'


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--extra-rust-binary', type=Path, help='Retain an additional implementation comparison without weakening the three-engine checks')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    sources = list((root/'crates/deadlib-lua').rglob('*.rs'))
    sources += [root/'crates/deadlib-lua/Cargo.toml', Path(__file__).resolve()]
    def hashes() -> dict[str, str]:
        return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in sorted(sources)}
    before = hashes()
    command = ['cargo', 'build', '--release', '-p', 'deadlib-lua', '--example', 'os_process', '--bin', 'deadlua']
    subprocess.run(command, cwd=root, check=True)
    extension = '.exe' if os.name == 'nt' else ''
    binary = root/f'target/release/examples/os_process{extension}'
    cli = root/f'target/release/deadlua{extension}'
    fixture_root = root/'target/lua-process-fixtures'
    fixture_root.mkdir(parents=True, exist_ok=True)
    results = []
    cases: list[tuple[str,str,dict[str,str|None]]] = []
    def case(name: str, source: str, env: dict[str,str|None]|None=None) -> None:
        cases.append((name,source,env or {}))
    case('shell_query', "print(os.execute(),os.execute(nil),os.execute(nil,false),select('#',os.execute()))")
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
                elif engine=='cli':invocation=[str(cli),str(script)]
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
            row['equal']=observations('reference')==observations('rust')==observations('cli')
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
