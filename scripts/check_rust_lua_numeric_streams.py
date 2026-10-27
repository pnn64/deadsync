"""Audit numeric stream consumption and math argument errors through real CLIs."""
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


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def literal(value):
    return '"'+''.join('\\%03d'%byte for byte in value)+'"'


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--run-root',type=Path,required=True)
    args=parser.parse_args()
    root=Path(__file__).resolve().parents[1]
    sources=list((root/'crates/deadlib-lua').rglob('*.rs'))+[root/'crates/deadlib-lua/Cargo.toml',Path(__file__).resolve(),root/'scripts/build_lua_cli_reference.py']
    def hashes():return {p.relative_to(root).as_posix():sha(p)for p in sorted(sources)}
    before=hashes()
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(root/'target'))).resolve()
    subprocess.run(['cargo','build','--release','-p','deadlib-lua','--bin','deadlua'],cwd=root,check=True)
    reference,metadata=build(root)
    directory=args.run_root.resolve()/str(time.time_ns());directory.mkdir(parents=True)
    binaries={}
    for engine,source in [('reference',reference),('rust',target/'release'/('deadlua.exe'if os.name=='nt'else'deadlua'))]:
        path=directory/engine/('lua.exe'if os.name=='nt'else'lua');path.parent.mkdir()
        shutil.copyfile(source,path)
        if os.name!='nt':path.chmod(0o755)
        binaries[engine]=path
    tokens=set([b'',b' ',b'\t\r\n',b'+',b'-',b'.',b'-.',b'+.',b'0x',b'0x.',b'0x.p1',b'0x1p',
                b'0x1p+',b'0x1p--1',b'1e',b'1e+',b'1e--2',b'1e999999999999999999',b'nan',b'-nan(ind)',
                b'nan(snan)',b'nan(foo)',b'nan()',b'nan(',b'nan(x y)',b'inf',b'infinity',b'infi',b'infinite!',
                b'0x1p-1074',b'0x1.fffffffffffffp-1023',b'0x1.fffffffffffff8p1023',b'123\x00tail'])
    for prefix in [b'',b'+',b'-',b'0',b'1',b'0x',b'0x1',b'0x1.',b'1.',b'1e',b'1e+',b'nan',b'nan(',b'inf',b'infi']:
        for suffix in [b'',b'e',b'e+',b'p',b'p+',b'x',b'.',b'(',b')',b'\x00',b'\n',b'\r',b'\xff',b'!!',b'1',b'-1',b'_tag']:
            tokens.add(prefix+suffix)
    boundaries=[b'',b'+',b'0',b'00',b'0x',b'0x1.',b'1e+',b'0x1p+',b'i',b'in',b'inf',b'infi',
                b'infinity',b'n',b'na',b'nan',b'nan(',b'nan(s',b'nan(i',b'nan(ind)',b'nan(snan)',
                b'nan(foo)',b'nan(sind)',b'nan(snind)',b'nan(snaind)',b'nan(snanind)']
    for prefix in boundaries:
        for byte in range(256):tokens.add(prefix+bytes([byte]))
        for sentinel in [b'\0',b'\xff']:
            for suffix in [b'\0',b'\xff',b'12',b'inity',b'(foo)',b'(ind)',b'(snan)',b'()',b'!23',b'e+2',b'p-1']:
                tokens.add(prefix+sentinel+suffix)
    random=0x4d595df4d0f33173
    for _ in range(300):
        random=(random*6364136223846793005+1)&((1<<64)-1)
        exponent=random%2200-1100
        tokens.add(('%s0x%x.%013xp%+d'%('-'if random&1 else'',(random>>60)|1,random&0xfffffffffffff,exponent)).encode())
    alphabet=b'0123abcdefxXpPeE+-.()Nainfty_ \t'
    for _ in range(300):
        data=bytearray()
        for _ in range(1+(random%25)):
            random=(random*6364136223846793005+1)&((1<<64)-1);data.append(alphabet[random%len(alphabet)])
        tokens.add(bytes(data))
    inputs=sorted({token+suffix for token in tokens for suffix in [b'',b'!']})
    rows=','.join(literal(value)for value in inputs)
    streams="assert(os.setlocale('C'));local inputs={"+rows+"}\n"+r'''
local function hex(s)return(s:gsub('.',function(c)return string.format('%02x',c:byte())end))end
local function number(n)
  if n==nil then return 'nil' elseif n~=n then return 'nan'
  elseif n==math.huge then return 'inf' elseif n==-math.huge then return '-inf'
  else return string.format('%.17g',n)end
end
for id,text in ipairs(inputs)do
  local f=assert(io.tmpfile());assert(f:write(text));assert(f:seek('set',0))
  local value=f:read('*n');local nonempty=f:read(0)~=nil
  local position=assert(f:seek());local tail=f:read('*a');assert(f:close())
  print(id,number(value),position,nonempty,hex(tail))
end
'''
    expressions=[]
    for name in ['pow','fmod','mod','atan2','ldexp']:
        for arguments in ['', 'nil','false','nil,nil','false,false','{},false','false,{}','1,false','false,2',"'3',false","false,'2'",'1,2']:
            expressions.append('return math.'+name+'('+arguments+')')
        expressions.extend(['local t={f=math.'+name+'};return t:f(false)',
                            'local t={f=setmetatable({},{__call=math.'+name+'})};return t:f(1)'])
    math="assert(os.setlocale('C'));local expressions={"+','.join(literal(s.encode())for s in expressions)+"}\n"+r'''
local function hex(s)return(s:gsub('.',function(c)return string.format('%02x',c:byte())end))end
for id,source in ipairs(expressions)do
  local f=assert(loadstring(source,'=(argument-order)'));local ok,value=pcall(f)
  print(id,ok,ok and tostring(value)or hex(value))
end
'''
    groups={}
    for name,source in [('streams',streams),('math',math)]:
        outcomes={}
        for engine,binary in binaries.items():
            cwd=directory/engine/name;cwd.mkdir()
            path=cwd/'case.lua';path.write_bytes(source.encode('utf-8'))
            environment={k:v for k,v in os.environ.items()if k not in ['LUA_INIT','LUA_PATH','LUA_CPATH']}
            run=subprocess.run(['lua','case.lua'],executable=binary,cwd=cwd,env=environment,capture_output=True,timeout=90)
            assert sha(path)==hashlib.sha256(source.encode()).hexdigest()
            outcomes[engine]=dict(returncode=run.returncode,stdout_hex=run.stdout.hex(),stderr_hex=run.stderr.hex())
        lines={e:bytes.fromhex(v['stdout_hex']).decode('ascii').splitlines()for e,v in outcomes.items()}
        expected=len(inputs)if name=='streams'else len(expressions)
        assert all(len(v)==expected for v in lines.values()),(name,{e:len(v)for e,v in lines.items()})
        mismatches=[dict(index=i+1,input_hex=inputs[i].hex()if name=='streams'else None,
                         expression=expressions[i]if name=='math'else None,reference=a,rust=b)
                    for i,(a,b)in enumerate(zip(lines['reference'],lines['rust']))if a!=b]
        equal=outcomes['reference']==outcomes['rust']and outcomes['reference']['returncode']==0
        groups[name]=dict(case_count=expected,source=source,source_sha256=hashlib.sha256(source.encode()).hexdigest(),
                          inputs_hex=[v.hex()for v in inputs]if name=='streams'else None,
                          expressions=expressions if name=='math'else None,runs=outcomes,equal=equal,mismatches=mismatches)
        print(name,expected-len(mismatches),'/',expected,'observations match',flush=True)
    if before!=hashes():raise SystemExit('sources changed during numeric probes')
    result=dict(source_hashes=before,reference=metadata,executables={e:dict(path=str(p),sha256=sha(p))for e,p in binaries.items()},
                groups=groups,equal=all(v['equal']for v in groups.values()),
                limitations='Standalone C-locale file scanning and math-error probes. Finite values use 17-digit formatting; NaN observations compare classification rather than payload bits. Raw stdout/status and unread bytes are retained. This corpus does not establish complete Lua 5.1 parity or cross-CRT equality.')
    args.output.resolve().parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(result,indent=2)+'\n')
    if not result['equal']:raise SystemExit(1)


if __name__=='__main__':main()
