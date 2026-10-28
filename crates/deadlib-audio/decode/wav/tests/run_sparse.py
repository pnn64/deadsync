"""Physical RF64/BW64 sparse-file checks and isolated Windows working-set evidence."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import struct
import subprocess
import time


def sha(data):return hashlib.sha256(data).hexdigest()


def markers(frames,offset):
    boundary=(2**32-offset)//4
    return sorted(set(p for p in (0,1,2**30-1,2**30,2**32-1,2**32,frames-1,boundary-1,boundary) if p<frames))


def word(frame):
    rotated=((frame>>29)|(frame<<35))&((1<<64)-1)
    return ((frame^(frame>>32)^rotated)*1664525+1013904223)&0xffffffff


def build(workspace,target=None):
    command=['cargo','build','--locked','--release','-p','deadlib-audio-decode-wav','--example','sparse_seek','--message-format=json','--target-dir','target/wav']
    if target:command+=['--target',target]
    result=subprocess.run(command,cwd=workspace,capture_output=True,text=True)
    if result.returncode:raise RuntimeError(result.stderr)
    artifacts=[json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
    binary=Path(next(a['executable'] for a in artifacts if a.get('executable') and a.get('target',{}).get('name')=='sparse_seek'))
    return binary,result.stdout+result.stderr,command


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
    if platform.system()!='Windows':ap.error('this evidence driver uses Windows sparse and process-memory APIs')
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3];output=args.output.resolve();output.parent.mkdir(parents=True,exist_ok=True)
    directory=Path(str(output)+'-files')
    if output.exists() or directory.exists():ap.error('choose a new output; fixtures/evidence are never overwritten')
    directory.mkdir()
    sources=sorted((root/'src').rglob('*.rs'))+[root/'examples/sparse_seek.rs',Path(__file__),root/'Cargo.toml',workspace/'Cargo.toml',workspace/'.cargo/config.toml']
    before={p.relative_to(workspace).as_posix():sha(p.read_bytes()) for p in sources}
    binary,log,command=build(workspace);Path(str(output)+'.x64.build.log').write_text(log)
    binary32,log32,command32=build(workspace,'i686-pc-windows-msvc');Path(str(output)+'.i686.build.log').write_text(log32)
    results=[];fixtures=[]
    for container in ('rf64','bw64'):
        for codec in ('pcm','ima'):
            for size,frames in (('small',2**16+37),('large',2**32+37)):
                path=directory/f'{container}-{codec}-{size}.wav'
                for label,exe,mode in [('x64-create',binary,'create'),('x64-verify',binary,'verify'),('i686-verify',binary32,'verify')]:
                    process=subprocess.run([str(exe),mode,str(path),container,codec,str(frames)],capture_output=True)
                    if process.returncode:
                        results.append(dict(container=container,codec=codec,size=size,run=label,exit_code=process.returncode,stderr=process.stderr.decode(errors='replace'),passed=False));continue
                    row=json.loads(process.stdout)
                    checks=[]
                    with path.open('rb') as source:
                        header=source.read(row['data_offset']);header_sha=sha(header)
                        for frame in markers(frames,row['data_offset']):
                            source.seek(row['data_offset']+frame*4);actual=source.read(4)
                            expected=struct.pack('<HBB',word(frame)&0xffff,0,0) if codec=='ima' else struct.pack('<I',word(frame))
                            checks.append(dict(frame=frame,offset=row['data_offset']+frame*4,bytes=actual.hex(),passed=actual==expected))
                        source.seek(row['data_offset']+42*4);hole=source.read(4)
                        source.seek(0,2);physical_length=source.tell()
                    passed=physical_length==row['file_bytes'] and row['allocated_bytes']<2**20 and row['frames']==frames and row['marker_samples']==len(checks) and all(c['passed'] for c in checks) and hole==b'\0'*4
                    results.append(dict(container=container,codec=codec,size=size,run=label,exit_code=process.returncode,passed=passed,header_sha256=header_sha,marker_checks=checks,zero_hole=hole.hex(),**row))
                fixtures.append(dict(path=str(path),container=container,codec=codec,size=size,frames=frames,file_bytes=path.stat().st_size if path.exists() else None))
    after={p.relative_to(workspace).as_posix():sha(p.read_bytes()) for p in sources}
    volume=subprocess.check_output(['powershell','-NoProfile','-Command','Get-Volume -DriveLetter C | Select-Object FileSystem,AllocationUnitSize,Size,SizeRemaining | ConvertTo-Json -Compress'],text=True).strip()
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),platform=platform.platform(),volume=volume,rustc=subprocess.check_output(['rustc','-Vv'],text=True),sources=before,inputs_unchanged=before==after,build_commands=[command,command32],binaries=dict(x64=sha(binary.read_bytes()),i686=sha(binary32.read_bytes())),memory_scope='GetProcessMemoryInfo whole-process working-set/private-commit snapshots; peak includes startup and any fixture creation, measured before JSON formatting; no affinity/cache control',data_scope='random marker and zero-hole samples plus EOF/invalid seeks; no full-file throughput or non-sparse-disk claim',fixtures=fixtures,passed=sum(c['passed'] for c in results),total=len(results),cases=results)
    output.write_text(json.dumps(report,indent=2)+'\n');print(f"{report['passed']}/{report['total']} physical sparse-file architecture checks passed",flush=True)
    for c in results:
        print(c['container'],c['codec'],c['size'],c['run'],c.get('file_bytes'),c.get('allocated_bytes'),c.get('peak_working_set'),c['passed'])
    if report['passed']!=report['total'] or before!=after:raise SystemExit(1)


if __name__=='__main__':main()
