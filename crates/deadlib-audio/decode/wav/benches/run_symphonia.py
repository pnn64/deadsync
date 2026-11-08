"""Isolated, bit-gated native/ADPCM Symphonia timings and real-file memory."""
import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tarfile
import time
import tomllib
import zipfile
from reference_fixtures import matrix,memory_matrix


def sha(data):return hashlib.sha256(data).hexdigest()


def archive(path,files):
    with zipfile.ZipFile(path,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as z:
        for name,data in sorted(files.items()):
            info=zipfile.ZipInfo(name,date_time=(1980,1,1,0,0,0));info.create_system=3;info.external_attr=0o600<<16
            z.writestr(info,data,compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)
    with zipfile.ZipFile(path) as z:assert {n:sha(z.read(n)) for n in z.namelist()}=={n:sha(b) for n,b in files.items()}
    return dict(archive_sha256=sha(path.read_bytes()),entries={n:sha(b) for n,b in files.items()})


def parse(data):return list(csv.DictReader((l for l in data.decode().splitlines() if not l.startswith('#')),delimiter='\t'))


def verify_registry(manifest):
    """Bind compiled registry sources to the published, lock-pinned .crate bytes."""
    m=json.loads(subprocess.check_output(['cargo','metadata','--locked','--format-version','1','--manifest-path',str(manifest)]))
    lock=tomllib.loads(manifest.with_name('Cargo.lock').read_text())
    checksums={(p['name'],p['version']):p['checksum'] for p in lock['package'] if 'checksum' in p};rows=[]
    for package in m['packages']:
        if not (package.get('source') or '').startswith('registry+'):continue
        directory=Path(package['manifest_path']).parent
        cached=directory.parents[2]/'cache'/directory.parent.name/(directory.name+'.crate')
        expected=checksums[package['name'],package['version']];assert sha(cached.read_bytes())==expected
        files={}
        with tarfile.open(cached,'r:gz') as archive:
            for member in archive.getmembers():
                if not member.isfile():continue
                name=Path(member.name).relative_to(directory.name).as_posix()
                assert '..' not in Path(name).parts
                data=archive.extractfile(member).read();assert (directory/name).read_bytes()==data,(package['name'],name)
                files[name]=sha(data)
        rows.append(dict(name=package['name'],version=package['version'],crate_checksum=expected,files=files))
    return rows


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);ap.add_argument('--smoke',action='store_true');ap.add_argument('--target');args=ap.parse_args()
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3];stem=args.output.resolve()
    for suffix in ('.json','.sources.zip','.fixtures.zip','.raw.zip','-sources','-fixtures','-raw'):
        if Path(str(stem)+suffix).exists():ap.error('preserve evidence; choose a new stem')
    stem.parent.mkdir(parents=True,exist_ok=True);project=Path(str(stem)+'-sources');project.mkdir();fixtures=Path(str(stem)+'-fixtures');fixtures.mkdir();raw=Path(str(stem)+'-raw');raw.mkdir()
    oracle=root/'tests/oracle/performance'
    paths=sorted((root/'src').rglob('*.rs'))+sorted((oracle/'src').glob('*.rs'))+[oracle/'Cargo.toml',oracle/'Cargo.lock',root/'README.md',root/'Cargo.toml',root/'benches/support/affinity.rs',root/'benches/native_fixtures.py',root/'benches/reference_fixtures.py',root/'benches/verify_native_files.py',root/'tests/adpcm_wavl_fixtures.py',root/'tests/ima_extensible_fixtures.py',root/'tests/ima3_fixtures.py',Path(__file__),workspace/'.cargo/config.toml',workspace/'Cargo.toml']
    inputs={p.relative_to(workspace).as_posix():sha(p.read_bytes()) for p in paths}
    # The library copy needs no dev dependencies or outer application workspace.
    minimal=b'[package]\nname="deadlib-audio-decode-wav"\nversion="0.1.0"\nedition="2024"\nrust-version="1.97"\n[workspace]\n'
    files={'wav/Cargo.toml':minimal}
    for p in sorted((root/'src').rglob('*.rs'))+[root/'README.md',root/'benches/support/affinity.rs']+sorted((oracle/'src').glob('*.rs'))+[oracle/'Cargo.toml',oracle/'Cargo.lock']:
        files['wav/'+p.relative_to(root).as_posix()]=p.read_bytes()
    for p in paths:files['context/worktree/'+p.relative_to(workspace).as_posix()]=p.read_bytes()
    files['REPRODUCE.txt']=b'Build wav/tests/oracle/performance/Cargo.toml --locked --release. For an extracted copy outside the original repository, copy context/worktree/.cargo/config.toml to .cargo/config.toml at archive root. Within the original repository the ancestor config is already applied once; context is deliberately not loaded. Cargo.lock pins reference packages/checksums. No third-party source is embedded. Fixture archive recipe: header + wire * wire_repetitions + wire[:wire_tail_bytes] + tail; period is independent container-scale i32, native float words, or ADPCM i16 truth. The Rust adapter explicitly left-aligns native integer truth/output to i32 for Symphonia comparison.\n'
    for name,data in files.items():p=project/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
    source_meta=archive(Path(str(stem)+'.sources.zip'),files)
    reference_packages=verify_registry(project/'wav/tests/oracle/performance/Cargo.toml')
    command=['cargo','build','--locked','--release','--manifest-path',str(project/'wav/tests/oracle/performance/Cargo.toml'),'--message-format=json']
    if args.target:command+=['--target',args.target]
    build=subprocess.run(command,cwd=project,capture_output=True,text=True);Path(str(stem)+'.build.log').write_text(build.stdout+build.stderr)
    if build.returncode:raise RuntimeError('private build failed; preserved log')
    messages=[json.loads(l) for l in build.stdout.splitlines() if l.startswith('{')]
    binary=Path(next(m['executable'] for m in messages if m.get('executable') and m.get('target',{}).get('name')=='wav-performance-reference'));binary_hash=sha(binary.read_bytes())
    cases=matrix(fixtures)
    if args.smoke:
        cases=[c for c in cases if c['container']=='RIFF' and c['channels']==2 and c['metadata_chunks']<=1 and c['memory_offset']==0 and c['valid_bits']==c['bits']]
    memory_cases=memory_matrix(fixtures,args.smoke)
    blobs={}
    for case in cases+memory_cases:
        for label,h in case['sections'].items():
            data=(fixtures/(case['name']+'.'+label)).read_bytes();assert sha(data)==h;blobs[h]=data
        digest=hashlib.sha256();digest.update(blobs[case['sections']['header']]);wire=blobs[case['sections']['wire']]
        for _ in range(case['wire_repetitions']):digest.update(wire)
        digest.update(wire[:case['wire_tail_bytes']]);digest.update(blobs[case['sections']['tail']]);assert digest.hexdigest()==case['fixture_sha256']
    fixture_files={'manifest.json':(json.dumps(dict(cases=cases,memory_cases=memory_cases,recipe='header + wire * wire_repetitions + wire[:wire_tail_bytes] + tail; native i32 container-scale truth is explicitly left-aligned by the adapter'),indent=2)+'\n').encode()}
    fixture_files.update({'blobs/'+h:data for h,data in blobs.items()});fixture_meta=archive(Path(str(stem)+'.fixtures.zip'),fixture_files)
    cpu=platform.processor()
    if os.name=='nt':cpu=subprocess.check_output(['powershell','-NoProfile','-Command','Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json -Compress'],text=True).strip()
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),completed=False,platform=platform.platform(),cpu=cpu,target=args.target,rustc=subprocess.check_output(['rustc','-Vv'],text=True),sources=inputs,reference_packages=reference_packages,source_archive=source_meta,fixture_archive=fixture_meta,build_command=command,binary_sha256=binary_hash,runs=[],memory_probes=[],disk_runs=[],environment={k:v for k,v in os.environ.items() if k in ('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','CARGO_BUILD_RUSTFLAGS') or k.startswith('CARGO_PROFILE_')},scope='isolated opt-level 3/LTO/debug-1 panic-abort release; Symphonia 0.6.1 wav/pcm/adpcm/opt-simd; three warmed trials with 10% (1..10000) untimed warmup iterations; benchmark thread pinned, frequency/other processes uncontrolled; common interleaved native-width floats, left-aligned i32 for integer PCM, i16 for ADPCM; RSS fresh process and includes validation; disk is OS-cache warm, validation outside timing; allocation counts exclude caller/input/truth buffers and realloc transient physical peaks')
    def run(case,mode,backend):
        command=[str(binary),mode,str(fixtures/(case['name']+'.wav')),case['kind'],str(case['channels']),str(case['frames']),str(case['bits']),str(case['memory_offset']),str(case['wire_bytes']),backend]
        p=subprocess.run(command,capture_output=True);label=mode+'-'+backend+'-'+case['name'];out=raw/(label+'.tsv');err=raw/(label+'.stderr');out.write_bytes(p.stdout);err.write_bytes(p.stderr)
        row=dict(case=case,command=command,exit_code=p.returncode,output_sha256=sha(p.stdout),stderr_sha256=sha(p.stderr),rows=parse(p.stdout) if p.returncode==0 else [],gates=[l.split('\t')[1:] for l in p.stdout.decode(errors='replace').splitlines() if l.startswith('# gate\t')],raw_stdout=out.name,raw_stderr=err.name)
        return row
    try:
        for i,case in enumerate(cases):
            row=run(case,'bench','both');report['runs'].append(row);assert row['exit_code']==0,(case['name'],(raw/row['raw_stderr']).read_text())
            assert ['native/full_stream','exact'] in row['gates'] and ['native/seek128','exact'] in row['gates']
            if (i+1)%10==0:print(i+1,'resident cases passed',flush=True)
        for case in memory_cases:
            for backend in ('baseline','native','symphonia'):
                row=run(case,'rss',backend);report['memory_probes'].append(row);assert row['exit_code']==0,(case['name'],backend,(raw/row['raw_stderr']).read_text())
                assert len(row['rows'])==1
            for backend in ('native','symphonia'):
                row=run(case,'disk',backend);report['disk_runs'].append(row);assert row['exit_code']==0,(case['name'],backend,(raw/row['raw_stderr']).read_text())
            print(case['name'],'physical file probes passed',flush=True)
        assert sha(binary.read_bytes())==binary_hash
        report['completed']=True
    except Exception as exc:
        report['exception']=repr(exc)
        raise
    finally:
        report['inputs_unchanged']=inputs=={p.relative_to(workspace).as_posix():sha(p.read_bytes()) for p in paths}
        report['reference_packages_unchanged']=reference_packages==verify_registry(project/'wav/tests/oracle/performance/Cargo.toml')
        report['raw_archive']=archive(Path(str(stem)+'.raw.zip'),{p.name:p.read_bytes() for p in raw.iterdir() if p.is_file()})
        Path(str(stem)+'.json').write_text(json.dumps(report,indent=2)+'\n')
    assert report['inputs_unchanged'] and report['reference_packages_unchanged']
    print(len(report['runs']),'resident,',len(report['memory_probes']),'RSS,',len(report['disk_runs']),'cached file runs passed')


if __name__=='__main__':main()
