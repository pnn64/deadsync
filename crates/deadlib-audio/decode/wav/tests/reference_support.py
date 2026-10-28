"""Reference-only native builds; downloaded code is pinned and stays under target/."""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import urllib.request

DR_COMMIT = 'a466dbe88fe2576d46c43877d602f3d66b8561a7'
DR_SHA256 = '34109aa9605f657d35c02f853f283d1f231883d9397e6a24b6504cc05cc1cafd'


def build_symphonia(workspace, root):
    manifest = root / 'tests/oracle/symphonia/Cargo.toml'
    result = subprocess.run(['cargo', 'build', '--locked', '--manifest-path', str(manifest),
                             '--target-dir', str(workspace/'target/wav-symphonia')], capture_output=True)
    cache = workspace/'target/wav-symphonia'
    cache.mkdir(parents=True, exist_ok=True)
    (cache/'build.log').write_bytes(result.stdout+result.stderr)
    if result.returncode:
        raise RuntimeError('Symphonia reference build failed; see '+str(cache/'build.log'))
    output = cache/'debug'/('wav-symphonia-reference.exe' if os.name=='nt' else 'wav-symphonia-reference')
    paths=[manifest,manifest.parent/'Cargo.lock',manifest.parent/'src/main.rs']
    return output,dict(version='0.6.1',sources={p.relative_to(root).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in paths},
                       binary_sha256=hashlib.sha256(output.read_bytes()).hexdigest())


def build_dr_wav(workspace, root):
    cache = workspace / 'target/wav/oracle'
    cache.mkdir(parents=True, exist_ok=True)
    header = cache / 'dr_wav.h'
    url = f'https://raw.githubusercontent.com/mackron/dr_libs/{DR_COMMIT}/dr_wav.h'
    data = header.read_bytes() if header.exists() else urllib.request.urlopen(url, timeout=30).read()
    if hashlib.sha256(data).hexdigest() != DR_SHA256:
        raise RuntimeError('dr_wav source hash mismatch')
    header.write_bytes(data)
    source = root / 'tests/oracle/dr_wav.c'
    output = cache / ('dr_wav.exe' if os.name == 'nt' else 'dr_wav')
    if os.name == 'nt':
        vswhere = Path(os.environ.get('ProgramFiles(x86)', r'C:\Program Files (x86)')) / 'Microsoft Visual Studio/Installer/vswhere.exe'
        setup = subprocess.check_output([str(vswhere), '-latest', '-products', '*', '-find', 'VC/Auxiliary/Build/vcvars64.bat'], text=True).strip().splitlines()[0]
        response = cache / 'build.rsp'
        args = ['/nologo', '/O2', '/MT', '/D_CRT_SECURE_NO_WARNINGS', '/I'+str(cache), '/Fe'+str(output), str(source)]
        response.write_text('\n'.join('"'+arg+'"' for arg in args)+'\n')
        script = cache / 'build.cmd'
        script.write_text('@call "'+setup+'" >nul\n@cl @"'+str(response)+'"\n@exit /b %errorlevel%\n')
        command = ['cmd', '/d', '/c', str(script)]
    else:
        command = [shutil.which('cc') or 'cc', '-O2', '-I'+str(cache), str(source), '-lm', '-o', str(output)]
    result = subprocess.run(command, cwd=cache, capture_output=True)
    (cache/'build.log').write_bytes(result.stdout+result.stderr)
    if result.returncode:
        raise RuntimeError('dr_wav build failed; see '+str(cache/'build.log'))
    return output, dict(commit=DR_COMMIT, header_sha256=DR_SHA256, binary_sha256=hashlib.sha256(output.read_bytes()).hexdigest(),
                        driver_sha256=hashlib.sha256(source.read_bytes()).hexdigest())
