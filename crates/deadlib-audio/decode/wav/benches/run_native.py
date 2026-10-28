"""Native-width/Hound timings and fresh-process physical-file memory evidence."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time
import zipfile
from native_fixtures import matrix,make,KINDS


def sha(b):return hashlib.sha256(b).hexdigest()


def rows(data):
    return list(csv.DictReader((l for l in data.decode().splitlines() if not l.startswith('#')),delimiter='\t'))


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);ap.add_argument('--smoke',action='store_true');args=ap.parse_args()
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3];stem=args.output.resolve()
    for suffix in ('.json','.build.log','.sources.zip','.fixtures.zip','-fixtures','-bin','-raw'):
        if Path(str(stem)+suffix).exists():ap.error('preserve evidence; choose a new output stem')
    stem.parent.mkdir(parents=True,exist_ok=True);fixtures=Path(str(stem)+'-fixtures');fixtures.mkdir();raw=Path(str(stem)+'-raw');raw.mkdir();binaries=Path(str(stem)+'-bin');binaries.mkdir()
    paths=sorted((root/'src').rglob('*.rs'))+[root/'README.md',root/'Cargo.toml',root/'benches/native.rs',root/'benches/support/native.rs',root/'benches/native_fixtures.py',Path(__file__),workspace/'Cargo.toml',workspace/'Cargo.lock',workspace/'.cargo/config.toml']
    inputs={p.relative_to(workspace).as_posix():sha(p.read_bytes()) for p in paths}
    with zipfile.ZipFile(str(stem)+'.sources.zip','x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as z:
        for p in paths:
            zi=zipfile.ZipInfo(p.relative_to(workspace).as_posix(),date_time=(1980,1,1,0,0,0));zi.create_system=3;zi.external_attr=0o600<<16;z.writestr(zi,p.read_bytes(),compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)
    cases=matrix(fixtures)
    if args.smoke:cases=[r for r in cases if r['container']=='RIFF' and r['channels']==2 and r['valid_bits']==r['bits'] and r['metadata_chunks']==0 and r['memory_offset']==0]
    memory_cases=[]
    if not args.smoke:
        for kind,(_,bits) in KINDS.items():
            for label,samples in (('small',8192),('large',(64<<20)//(bits//8))):memory_cases.append(make(fixtures,'RIFF',kind,2,samples=samples,suffix='-rss-'+label))
    # A small content-addressed archive reconstructs every exact large fixture
    # from its header, repeated 4096-sample wire block, tail and independent truth.
    blobs={}
    for case in cases+memory_cases:
        for label,h in case['sections'].items():
            b=(fixtures/(case['name']+'.'+label)).read_bytes();assert sha(b)==h
            blobs[h]=b
        remaining=case['samples'];wire=blobs[case['sections']['wire']];digest=hashlib.sha256();digest.update(blobs[case['sections']['header']])
        while remaining>=4096:digest.update(wire);remaining-=4096
        digest.update(wire[:remaining*case['bits']//8]);digest.update(blobs[case['sections']['tail']]);assert digest.hexdigest()==case['fixture_sha256']
        if os.name=='nt':
            import ctypes
            k=ctypes.WinDLL('kernel32',use_last_error=True);k.GetCompressedFileSizeW.argtypes=[ctypes.c_wchar_p,ctypes.POINTER(ctypes.c_uint32)];k.GetCompressedFileSizeW.restype=ctypes.c_uint32
            high=ctypes.c_uint32();ctypes.set_last_error(0);low=k.GetCompressedFileSizeW(str(fixtures/(case['name']+'.wav')),ctypes.byref(high))
            if low==0xffffffff and ctypes.get_last_error():raise OSError(ctypes.get_last_error(),'GetCompressedFileSizeW')
            case['allocated_file_bytes']=(high.value<<32)|low
    with zipfile.ZipFile(str(stem)+'.fixtures.zip','x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as z:
        archive_files={'manifest.json':(json.dumps(dict(cases=cases,memory_cases=memory_cases,recipe='header + wire * (samples // 4096) + wire[:(samples % 4096) * bits // 8] + tail; period contains independent little-endian i32 or native float truth'),indent=2)+'\n').encode()}
        archive_files.update({'blobs/'+h:b for h,b in blobs.items()})
        for name,b in sorted(archive_files.items()):
            zi=zipfile.ZipInfo(name,date_time=(1980,1,1,0,0,0));zi.create_system=3;zi.external_attr=0o600<<16;z.writestr(zi,b,compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)
    command=['cargo','bench','--locked','-p','deadlib-audio-decode-wav','--bench','native','--no-run','--message-format=json','--target-dir','target/wav']
    build=subprocess.run(command,cwd=workspace,capture_output=True,text=True);Path(str(stem)+'.build.log').write_text(build.stdout+build.stderr)
    if build.returncode:raise RuntimeError('benchmark build failed; inspect preserved log')
    artifacts=[json.loads(line) for line in build.stdout.splitlines() if line.startswith('{')];artifact=Path(next(a['executable'] for a in artifacts if a.get('executable') and a.get('target',{}).get('name')=='native'))
    binary=binaries/artifact.name;shutil.copyfile(artifact,binary)
    runs=[];rss=[];disk=[];failure=None
    def execute(case,mode,backend=None):
        command=[str(binary),mode,str(fixtures/(case['name']+'.wav')),case['kind'],str(case['channels']),str(case['frames']),str(case['memory_offset'])]
        if backend:command.append(backend)
        p=subprocess.run(command,cwd=workspace,capture_output=True);label=case['name']+('-'+mode+'-'+backend if backend else '')
        (raw/(label+'.tsv')).write_bytes(p.stdout);(raw/(label+'.stderr')).write_bytes(p.stderr)
        return p,dict(command=command,exit_code=p.returncode,stdout_sha256=sha(p.stdout),stderr_sha256=sha(p.stderr),rows=rows(p.stdout) if p.returncode==0 else [])
    try:
        for case in cases:
            p,run=execute(case,'bench');run['case']=case
            comments=[l for l in p.stdout.decode(errors='replace').splitlines() if l.startswith('#')];run['capabilities']=comments
            passed=p.returncode==0 and (not case['hound_decode_gate'] or any('hound_decode_exact=true' in c for c in comments));run['passed']=passed;runs.append(run)
            if not passed:raise RuntimeError('native case failed: '+case['name']+'; see raw stderr/capabilities')
            if len(runs)%6==0:print(f'{len(runs)}/{len(cases)} native cases passed',flush=True)
        for case in memory_cases:
            for backend in ('baseline','wav','hound'):
                if backend=='hound' and case['kind']=='f64':continue
                p,run=execute(case,'rss',backend);run.update(case=case,backend=backend);rss.append(run)
                if p.returncode:raise RuntimeError('memory probe failed: '+case['name']+' '+backend)
                if backend!='baseline':
                    p,run=execute(case,'disk',backend);run.update(case=case,backend=backend);disk.append(run)
                    if p.returncode:raise RuntimeError('file stream benchmark failed: '+case['name']+' '+backend)
        print(f'{len(runs)} native cases, {len(rss)} fresh-process memory probes and {len(disk)} file-stream runs passed',flush=True)
    except Exception as exc:failure=str(exc)
    unchanged=inputs=={p.relative_to(workspace).as_posix():sha(p.read_bytes()) for p in paths}
    cpu=platform.processor()
    if os.name=='nt':cpu=subprocess.check_output(['powershell','-NoProfile','-Command','Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json -Compress'],text=True).strip()
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),platform=platform.platform(),python=sys.version,cpu=cpu,rustc=subprocess.check_output(['rustc','-Vv'],text=True),hound_version='3.5.1',sources=inputs,inputs_unchanged=unchanged,source_archive_sha256=sha(Path(str(stem)+'.sources.zip').read_bytes()),fixture_archive_sha256=sha(Path(str(stem)+'.fixtures.zip').read_bytes()),binary_sha256=sha(binary.read_bytes()),build_command=command,smoke=args.smoke,total_cases=len(cases),passed_cases=sum(r['passed'] for r in runs),runs=runs,memory_probes=rss,file_stream_runs=disk,failure=failure,scope='three warmed resident trials; integer output is container-scale i32, float output retains native bits; wire/output rates distinct; heap is one warmed complete operation with caller storage excluded; no affinity/frequency control; RSS is whole fresh Windows process with common touched buffers, includes sample validation; baseline is raw-file drain, not isolated codec memory; file timing has an independent validated pass before warm cached-file measurements and excludes per-sample validation from timing')
    Path(str(stem)+'.json').write_text(json.dumps(report,indent=2)+'\n')
    if failure or not unchanged:raise RuntimeError(failure or 'inputs changed during measurement')


if __name__=='__main__':main()
