"""Audit bounded Lua 5.1 standard-library argument handling against official Lua.

The matrix omits APIs that mutate shared environments, debug locals or type
metatables, and omits exit/shell/file APIs. Failures remain failures, including
unsupported native module loading; no per-function allowlist changes the exit.
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
from build_lua_cli_reference import build

functions=['assert','type','tonumber','rawequal','rawget','rawset','getmetatable','setmetatable','getfenv','setfenv','next','pairs','ipairs','unpack','select','collectgarbage','newproxy','string.byte','string.char','string.sub','string.rep','string.find','string.match','string.gmatch','string.gsub','string.format','string.dump','string.len','string.lower','string.upper','string.reverse','table.getn','table.setn','table.insert','table.remove','table.concat','table.maxn','table.foreach','table.foreachi','table.sort','math.abs','math.acos','math.atan2','math.ceil','math.frexp','math.fmod','math.ldexp','math.log','math.max','math.min','math.modf','math.pow','math.random','math.randomseed','debug.getinfo','debug.getlocal','debug.setlocal','debug.getupvalue','debug.setupvalue','debug.setfenv','debug.setmetatable','debug.getmetatable','debug.getfenv','debug.sethook','coroutine.create','coroutine.resume','coroutine.status','coroutine.wrap','package.seeall','package.loadlib']
values=['nil','false','true','0','1','-1',"''","'2'","'bad'","'a\\000b'",'{}','function()end','coroutine.create(function()end)','newproxy(true)']

def matrix():
    cases=[]
    for function in functions:
        if function in ['setfenv','debug.setfenv','debug.setlocal','debug.setmetatable','debug.sethook']:
            continue
        cases.append((function,''))
        for index,value in enumerate(values):
            if function in ['string.rep','math.random','math.randomseed'] and index==5:
                continue
            for arguments in [value,value+',nil',value+',false',value+',1',value+",'bad'",value+',{}']:
                cases.append((function,arguments))
    source='local function obs(v) local t=type(v);if t=="string"then return string.format("%q",v)elseif t=="number"or t=="boolean"or t=="nil"then return tostring(v)else return t end end\n'
    for index,(function,arguments)in enumerate(cases):
        invocation=function+(','+arguments if arguments else '')
        source+=f'do local ok,a,b,c=pcall({invocation});debug.sethook();print({index},ok,obs(a),obs(b),obs(c))end\n'
    return cases,source.encode()

def observations(output):
    starts=list(re.finditer(rb'(?m)^(\d+)\t(?:true|false)\t',output))
    return {int(match.group(1)):output[match.start():starts[n+1].start() if n+1<len(starts) else len(output)]for n,match in enumerate(starts)}

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--rust-binary',type=Path,help='Prebuilt executable; this audit does not establish its source provenance')
    args=parser.parse_args()
    root=Path(__file__).resolve().parents[1]
    sources=[p for p in (root/'crates/deadlib-lua').rglob('*')if p.suffix in {'.rs','.c','.h'}]+[root/'crates/deadlib-lua/Cargo.toml',Path(__file__).resolve(),root/'scripts/build_lua_cli_reference.py']
    sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
    hashes=lambda:{p.relative_to(root).as_posix():sha(p)for p in sorted(sources)}
    before=hashes()
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(root/'target')))
    if not target.is_absolute():target=root/target
    target=target.resolve()
    if args.rust_binary is None:
        subprocess.run(['cargo','build','--release','-p','deadlib-lua','--bin','deadlua'],cwd=root,check=True)
    binary=args.rust_binary.resolve()if args.rust_binary else target/'release'/('deadlua.exe'if os.name=='nt'else'deadlua')
    reference,metadata=build(root)
    cases,source=matrix()
    runs={};rows={}
    for engine,executable in [('reference',reference),('rust',binary)]:
        with tempfile.TemporaryDirectory(prefix='deadlua-stdlib-')as temporary:
            env=os.environ.copy()
            for key in ['LUA_INIT','LUA_PATH','LUA_CPATH']:env.pop(key,None)
            try:
                run=subprocess.run([str(executable),'-'],input=source,cwd=temporary,env=env,capture_output=True,timeout=60)
                runs[engine]=dict(returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
                rows[engine]=observations(run.stdout)
            except subprocess.TimeoutExpired:
                runs[engine]=dict(timeout=True);rows[engine]={}
    mismatches=[]
    for index,(function,arguments)in enumerate(cases):
        left=rows['reference'].get(index);right=rows['rust'].get(index)
        if left is None or right is None or left!=right:
            mismatches.append(dict(index=index,function=function,arguments=arguments,reference_hex=None if left is None else left.hex(),rust_hex=None if right is None else right.hex()))
    assert hashes()==before,'sources changed during audit'
    complete=all(set(items)==set(range(len(cases)))for items in rows.values())
    equal=runs['reference']==runs['rust']
    result=dict(source_hashes=before,reference=dict(**metadata,binary_path=str(reference)),rust_binary=dict(path=str(binary),sha256=sha(binary),prebuilt=args.rust_binary is not None),generated_source_sha256=hashlib.sha256(source).hexdigest(),case_count=len(cases),observed_cases={engine:len(items)for engine,items in rows.items()},passed=len(cases)-len(mismatches),mismatches=mismatches,runs=runs,limitations=__doc__+' Values observe primitive bytes and object types, not pointer identity. The reference contract is the pinned Windows/MSVC build; C argument evaluation order can vary by toolchain. Full library and standalone compatibility remain required.')
    result['complete_matrix']=complete
    result['equal_processes']=equal
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
    print(f"{result['passed']}/{len(cases)} library argument cases match; {len(mismatches)} mismatches",flush=True)
    if mismatches or not complete or not equal or any(run.get('returncode')!=0 or run.get('stderr_hex')for run in runs.values()):raise SystemExit(1)

if __name__=='__main__':main()
