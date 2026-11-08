"""Build the checksum-pinned, unmodified official Lua 5.1.5 standalone oracle."""
from __future__ import annotations
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import urllib.request

URL='https://www.lua.org/ftp/lua-5.1.5.tar.gz'
SHA256='2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333'

def build(root: Path, target: str | None = None, fp_model: str | None = None) -> tuple[Path,dict]:
    if target is not None and (target != 'i686-unknown-linux-gnu' or sys.platform != 'linux'):
        raise SystemExit('the explicit reference target requires Linux i686-unknown-linux-gnu')
    if fp_model is not None and (fp_model != 'sse2' or target != 'i686-unknown-linux-gnu'):
        raise SystemExit('the explicit SSE2 model requires Linux i686-unknown-linux-gnu')
    cache=root/'target'/('lua-cli-reference'+('-'+target if target else '')+('-'+fp_model if fp_model else ''))
    cache.mkdir(parents=True,exist_ok=True)
    archive=cache/'lua-5.1.5.tar.gz'
    data=archive.read_bytes() if archive.exists() else urllib.request.urlopen(URL,timeout=30).read()
    if hashlib.sha256(data).hexdigest()!=SHA256:raise SystemExit('official Lua source checksum mismatch')
    archive.write_bytes(data)
    with tarfile.open(fileobj=io.BytesIO(data),mode='r:gz') as packed:
        if any(not (cache/m.name).resolve().is_relative_to(cache.resolve()) for m in packed.getmembers()):
            raise SystemExit('source archive escapes cache directory')
        packed.extractall(cache,filter='data')
    source=cache/'lua-5.1.5/src'
    files=sorted(p for p in source.glob('*.c') if p.name not in ('luac.c','print.c'))
    output=cache/('lua.exe' if os.name=='nt' else 'lua')
    inputs={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(source.iterdir()) if p.suffix in ('.c','.h')}
    if os.name=='nt':
        vswhere=Path(os.environ.get('ProgramFiles(x86)',r'C:\Program Files (x86)'))/'Microsoft Visual Studio/Installer/vswhere.exe'
        found=subprocess.check_output([str(vswhere),'-latest','-products','*','-find','VC/Auxiliary/Build/vcvars64.bat'],text=True).strip().splitlines()
        if not found:raise SystemExit('MSVC x64 tools are required for the Windows CLI oracle')
        command=['cl','/nologo','/O2','/MT','/D_CRT_SECURE_NO_WARNINGS',f'/I{source}',f'/Fe{output}',*[str(p) for p in files],'/link','kernel32.lib']
        response=cache/'build.rsp'
        response.write_text('\n'.join('"'+arg+'"' for arg in command[1:])+'\n')
        script=cache/'build.cmd'
        script.write_text('@call "'+found[0]+'" >nul\n@cl @"'+str(response)+'"\n@exit /b %errorlevel%\n')
        invocation=['cmd','/d','/c',str(script)]
    else:
        compiler=shutil.which('cc')
        if not compiler:raise SystemExit('a C compiler is required for the standalone oracle')
        command=[compiler,'-O2','-DLUA_USE_POSIX','-DLUA_USE_DLOPEN','-DLUA_USE_ISATTY','-I'+str(source),*[str(p) for p in files],'-Wl,-E','-lm','-ldl','-o',str(output)]
        if target:command.insert(1,'-m32')
        if fp_model:command[2:2]=['-msse2','-mfpmath=sse']
        invocation=command
    run=subprocess.run(invocation,cwd=cache,capture_output=True)
    (cache/'build.log').write_bytes(run.stdout+run.stderr)
    if run.returncode:raise SystemExit('official CLI build failed; see '+str(cache/'build.log'))
    metadata=dict(url=URL,archive_sha256=SHA256,source_hashes=inputs,command=command,
                  platform=sys.platform,binary_sha256=hashlib.sha256(output.read_bytes()).hexdigest())
    if target:metadata['target']=target
    if fp_model:metadata['floating_point_model']=fp_model
    (cache/'build.json').write_text(json.dumps(metadata,indent=2)+'\n')
    return output,metadata

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target',choices=['i686-unknown-linux-gnu'])
    parser.add_argument('--fp-model',choices=['sse2'])
    args=parser.parse_args()
    binary,_=build(Path(__file__).resolve().parents[1],args.target,args.fp_model)
    print(binary)
