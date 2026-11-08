"""Reproducible resident metadata benchmark against pinned dr_wav.

Fixtures and checksum truth are independent of both implementations. Timed work
includes neither disk reads nor fixture construction. Raw trials stay unmodified.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import struct
import subprocess
import sys
import time

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tests'))
from reference_support import build_dr_wav


def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()


def fixture(points,texts,width):
    def ck(tag,data): return tag+struct.pack('<I',len(data))+data+b'\0'*(len(data)&1)
    checksum=0;rows=[]
    for i in range(points):
        fields=(i+7,0,0,0,i*17)
        rows.append(struct.pack('<II4sIII',fields[0],fields[1],b'data',*fields[2:]))
        checksum+=sum(fields)+sum(b'data')
    info=b'INFO'
    for i in range(texts):
        text=bytes(33+(j*13+i)%90 for j in range(width))
        info+=ck(b'INAM',text+b'\0');checksum+=sum(text)
    frames=points*17+1
    audio=b'\0'*(frames*4)
    fmt=struct.pack('<HHIIHH',1,2,48000,192000,4,16)
    metadata=ck(b'cue ',struct.pack('<I',points)+b''.join(rows))+ck(b'LIST',info)
    body=b'WAVE'+ck(b'fmt ',fmt)+metadata+ck(b'data',audio)
    return b'RIFF'+struct.pack('<I',len(body))+body,dict(cues=points,info_entries=texts,text_bytes_per_entry=width,metadata_bytes=len(metadata),audio_bytes=len(audio),checksum=checksum)


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
    root=ROOT;workspace=root.parents[3];stem=args.output.resolve();stem.parent.mkdir(parents=True,exist_ok=True)
    for suffix in ('.json','.tsv','.build.log','.reference.build.log','.stderr','-fixtures'):
        if Path(str(stem)+suffix).exists():ap.error('choose a new output stem')
    fixtures=Path(str(stem)+'-fixtures');fixtures.mkdir()
    paths=sorted((root/'src').rglob('*.rs'))+[root/'Cargo.toml',root/'benches/metadata.rs',root/'benches/support/mod.rs',Path(__file__),root/'tests/oracle/dr_wav_metadata_bench.c',root/'tests/reference_support.py',workspace/'Cargo.toml',workspace/'Cargo.lock',workspace/'.cargo/config.toml']
    before={p.relative_to(workspace).as_posix():sha(p) for p in paths}
    reference,provenance=build_dr_wav(workspace,root,'dr_wav_metadata_bench')
    Path(str(stem)+'.reference.build.log').write_bytes((workspace/'target/wav/oracle/dr_wav_metadata_bench.build.log').read_bytes())
    command=['cargo','bench','--locked','-p','deadlib-audio-decode-wav','--bench','metadata','--no-run','--message-format=json','--target-dir','target/wav']
    build=subprocess.run(command,cwd=workspace,capture_output=True,text=True);Path(str(stem)+'.build.log').write_text(build.stdout+build.stderr)
    if build.returncode:raise RuntimeError('benchmark build failed; inspect log')
    artifacts=[json.loads(line) for line in build.stdout.splitlines() if line.startswith('{')]
    binary=Path(next(a['executable'] for a in artifacts if a.get('executable') and a.get('target',{}).get('name')=='metadata'))
    cases=[];raw=[];errors=[];exit_codes=[]
    for index,(name,points,texts,width,iters) in enumerate([('small',32,4,32,50000),('medium',4096,64,64,5000),('large',16384,512,256,1000)]):
        data,truth=fixture(points,texts,width);path=fixtures/(name+'.wav');path.write_bytes(data)
        order=[('rust',binary),('dr_wav',reference)] if index%2==0 else [('dr_wav',reference),('rust',binary)]
        for label,exe in order:
            run=subprocess.run([str(exe),str(path),str(iters),str(truth['checksum'])],cwd=workspace,capture_output=True)
            exit_codes.append(run.returncode);errors.append(run.stderr)
            raw.append(f'# fixture={name}; implementation={label}; file_bytes={len(data)}; truth={json.dumps(truth,sort_keys=True)}\n'.encode()+run.stdout)
        cases.append(dict(name=name,fixture_sha256=sha(path),file_bytes=len(data),iterations=iters,order=[v[0] for v in order],**truth))
    Path(str(stem)+'.tsv').write_bytes(b'\n'.join(raw));Path(str(stem)+'.stderr').write_bytes(b'\n'.join(errors))
    after={p.relative_to(workspace).as_posix():sha(p) for p in paths}
    cpu=platform.processor()
    if os.name=='nt':cpu=subprocess.check_output(['powershell','-NoProfile','-Command','Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json -Compress'],text=True).strip()
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),platform=platform.platform(),cpu=cpu,rustc=subprocess.check_output(['rustc','-Vv'],text=True),reference=provenance,binary_sha256=sha(binary),sources=before,inputs_unchanged=before==after,cases=cases,exit_codes=exit_codes,command=command,timing_scope='five warmed trials per operation; resident leading RIFF metadata; no disk I/O, affinity or frequency control; processes run sequentially',heap_scope='single warmed operation; requested allocator bytes only; input and retained views excluded; not process RSS',checksum_scope='all six cue fields including FourCC bytes, and all INFO text bytes excluding final NUL')
    Path(str(stem)+'.json').write_text(json.dumps(report,indent=2)+'\n')
    print(b'\n'.join(raw).decode(),end='')
    if any(exit_codes) or before!=after:raise SystemExit(1)


if __name__=='__main__':main()
