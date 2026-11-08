"""Paired flat-reader regression and current wavl timing in isolated source copies."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time
import zipfile


def sha(b):return hashlib.sha256(b).hexdigest()


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);ap.add_argument('--baseline',default='491b3c88d');args=ap.parse_args()
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3];stem=args.output.resolve();directory=Path(str(stem)+'-sources');directory.mkdir(parents=True,exist_ok=False)
    manifest='''[package]
name = "deadlib-audio-decode-wav"
version = "0.1.0"
edition = "2024"
rust-version = "1.97"
[workspace]
[[bench]]
name = "adpcm_file"
harness = false
[profile.release]
opt-level = 3
overflow-checks = false
debug-assertions = false
lto = true
debug = false
panic = "abort"
[profile.bench]
debug = 1
'''
    inputs=sorted((root/'src').rglob('*.rs'))+[root/'README.md',root/'Cargo.toml',root/'benches/adpcm_file.rs',Path(__file__),workspace/'Cargo.toml',workspace/'.cargo/config.toml']
    current={p.relative_to(workspace).as_posix():sha(p.read_bytes()) for p in inputs};builds={};runs=[]
    source_names=subprocess.check_output(['git','ls-tree','-r','--name-only',args.baseline,'--',(root/'src').relative_to(workspace).as_posix()],cwd=workspace,text=True).splitlines()
    for version in ('baseline','current'):
        project=directory/version;project.mkdir();files={'Cargo.toml':manifest.encode(),'benches/adpcm_file.rs':(root/'benches/adpcm_file.rs').read_bytes()}
        if version=='baseline':
            for name in source_names:files[Path(name).relative_to(root.relative_to(workspace)).as_posix()]=subprocess.check_output(['git','show',args.baseline+':'+name],cwd=workspace)
            files['README.md']=subprocess.check_output(['git','show',args.baseline+':'+(root/'README.md').relative_to(workspace).as_posix()],cwd=workspace)
        else:
            for p in sorted((root/'src').rglob('*.rs'))+[root/'README.md']:files[p.relative_to(root).as_posix()]=p.read_bytes()
        with zipfile.ZipFile(str(stem)+'-'+version+'.zip','x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
            for name,data in sorted(files.items()):
                p=project/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
                zi=zipfile.ZipInfo(name,date_time=(1980,1,1,0,0,0));zi.create_system=3;zi.external_attr=0o600<<16;archive.writestr(zi,data)
        p=subprocess.run(['cargo','generate-lockfile','--offline'],cwd=project,capture_output=True);assert p.returncode==0,p.stderr.decode(errors='replace')
        command=['cargo','bench','--locked','--bench','adpcm_file','--no-run','--message-format=json']
        p=subprocess.run(command,cwd=project,capture_output=True,text=True);Path(str(stem)+'-'+version+'.build.log').write_text(p.stdout+p.stderr);assert p.returncode==0,p.stderr
        artifacts=[json.loads(line) for line in p.stdout.splitlines() if line.startswith('{')];binary=Path(next(a['executable'] for a in artifacts if a.get('executable') and a.get('target',{}).get('name')=='adpcm_file'))
        builds[version]=dict(binary=str(binary),binary_sha256=sha(binary.read_bytes()),sources={name:sha(data) for name,data in files.items()},lock_sha256=sha((project/'Cargo.lock').read_bytes()),archive_sha256=sha(Path(str(stem)+'-'+version+'.zip').read_bytes()),command=command)
    for label,version,flags in [('control1','baseline',['--flat-only']),('current1','current',['--flat-only']),('control2','baseline',['--flat-only']),('current2','current',['--flat-only']),('segmented','current',[])]:
        command=[builds[version]['binary']]+flags;p=subprocess.run(command,cwd=workspace,capture_output=True)
        Path(str(stem)+'-'+label+'.tsv').write_bytes(p.stdout);Path(str(stem)+'-'+label+'.stderr').write_bytes(p.stderr)
        runs.append(dict(label=label,version=version,command=command,exit_code=p.returncode,output_sha256=sha(p.stdout),stderr_sha256=sha(p.stderr)));assert p.returncode==0,p.stderr.decode(errors='replace');print(label,'passed',flush=True)
    unchanged=current=={p.relative_to(workspace).as_posix():sha(p.read_bytes()) for p in inputs}
    cpu=platform.processor()
    if os.name=='nt':cpu=subprocess.check_output(['powershell','-NoProfile','-Command','Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json -Compress'],text=True).strip()
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),baseline_revision=args.baseline,platform=platform.platform(),cpu=cpu,rustc=subprocess.check_output(['rustc','-Vv'],text=True),sources=current,inputs_unchanged=unchanged,builds=builds,runs=runs,scope='three warmed trials; resident Cursor input; static MSVC runtime via workspace .cargo config; no affinity/frequency control; heap/RSS excluded; standalone copies use identical explicit bench profiles')
    Path(str(stem)+'.json').write_text(json.dumps(report,indent=2)+'\n');assert unchanged


if __name__=='__main__':main()
