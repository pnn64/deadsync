"""Compare independent Ogg container/CRC workers against pinned libogg.

All packet/page bytes are compared outside timing before valid inputs are used.
Production crates remain independent of the reference library.
"""
from __future__ import annotations
import argparse
import csv
from datetime import datetime, timezone
import json
import math
import os
from pathlib import Path
import platform
import random
import shutil
import statistics
import subprocess
import tomllib
import uuid
import zipfile
from benchmark import DECODE, REPO, PINS, label, sha, registry_packages


def invoke(command, log=None):
    result = subprocess.run([str(a) for a in command], cwd=REPO, capture_output=True)
    if log: log.write_bytes(result.stdout + result.stderr)
    if result.returncode: raise RuntimeError(f'{command}: {result.stderr.decode(errors="replace")[-6000:]}')
    return result.stdout


def rust_worker(build):
    artifacts = [json.loads(line) for line in build.decode().splitlines() if line.startswith('{')]
    matches = [a for a in artifacts if a.get('reason') == 'compiler-artifact' and a.get('executable')
               and a['target']['name'] == 'container' and 'deadlib-audio-decode-ogg' in a['package_id']]
    if len(matches) != 1: raise RuntimeError('cannot identify exact container worker')
    return Path(matches[0]['executable']), matches[0]['profile']


def baseline(revision, run):
    revision = invoke(['git','rev-parse','--verify','--end-of-options',revision+'^{commit}']).decode().strip()
    base = run / 'baseline'; base.mkdir(); package = base / 'ogg'; package.mkdir()
    for p in (DECODE/'ogg').iterdir():
        if p.name in ('src','target'): continue
        if p.is_dir(): shutil.copytree(p,package/p.name)
        else: shutil.copy2(p,package/p.name)
    archive = base/'production-source.zip'; prefix='crates/deadlib-audio/decode/'
    invoke(['git','archive','--format=zip',f'--output={archive}',revision,prefix+'ogg/src'])
    with zipfile.ZipFile(archive) as z:
        for entry in z.infolist():
            if entry.is_dir(): continue
            relative=Path(entry.filename.removeprefix(prefix))
            if not entry.filename.startswith(prefix+'ogg/src/') or relative.is_absolute() or '..' in relative.parts:
                raise ValueError('unexpected source archive entry')
            destination=base/relative;destination.parent.mkdir(parents=True,exist_ok=True);destination.write_bytes(z.read(entry))
    cargo='[workspace]\nmembers = ["ogg"]\nresolver = "3"\n'
    profiles=tomllib.loads((REPO/'Cargo.toml').read_text(encoding='utf-8'))['profile']
    for name in ('release','bench'):
        cargo+=f'\n[profile.{name}]\n'
        for key,value in profiles[name].items():
            if isinstance(value,dict): raise ValueError('requires flat bench/release profile settings')
            cargo+=f'{key} = {json.dumps(value)}\n'
    (base/'Cargo.toml').write_bytes(cargo.encode());shutil.copy2(REPO/'Cargo.lock',base/'Cargo.lock')
    build=invoke(['cargo','bench','--offline','--manifest-path',base/'Cargo.toml','-p','deadlib-audio-decode-ogg',
                  '--bench','container','--no-run','--message-format=json','--target-dir',base/'target'],run/'baseline-build.log')
    binary,profile=rust_worker(build)
    return binary,{'revision':revision,'binary_sha256':sha(binary),'source_archive_sha256':sha(archive),
        'cargo_artifact_profile':profile,'registry_packages':registry_packages(build.decode()),
        'adapter_source_sha256':{p.relative_to(base).as_posix():sha(p) for p in (package/'benches').rglob('*') if p.is_file()},
        'source_sha256':{p.relative_to(base).as_posix():sha(p) for p in (package/'src').rglob('*') if p.is_file()}}


def table(path,rows):
    with path.open('w',newline='',encoding='utf-8') as f:
        w=csv.DictWriter(f,fieldnames=list(rows[0]),delimiter='\t',lineterminator='\n');w.writeheader();w.writerows(rows)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('files',type=Path,nargs='*')
    parser.add_argument('--run-root',type=Path)
    parser.add_argument('--reference-root',type=Path,default=REPO/'target/deadlib-audio-reference')
    parser.add_argument('--repeats',type=int,default=7)
    parser.add_argument('--min-seconds',type=float,default=0.15)
    parser.add_argument('--seed',type=int,default=1337)
    parser.add_argument('--baseline-revision')
    args=parser.parse_args()
    if args.repeats<3 or not math.isfinite(args.min_seconds) or args.min_seconds<=0:parser.error('invalid repeat count/duration')
    run=(args.run_root or REPO/'target/deadlib-audio-reference'/('ogg-'+uuid.uuid4().hex)).resolve();run.mkdir(parents=True,exist_ok=False)
    reference=args.reference_root.resolve();ogg=reference/'ogg'
    if invoke(['git','-C',ogg,'rev-parse','HEAD']).decode().strip()!=PINS['ogg']:raise RuntimeError('libogg source pin mismatch')
    if invoke(['git','-C',ogg,'status','--porcelain','--untracked-files=no']).strip():raise RuntimeError('libogg tracked source changed')
    build=invoke(['cargo','bench','--locked','-p','deadlib-audio-decode-ogg','--bench','container','--no-run','--message-format=json'],run/'rust-build.log')
    native,profile=rust_worker(build);workers={'native':native};old=None
    if args.baseline_revision:
        workers['native-baseline'],old=baseline(args.baseline_revision,run)
        if old['cargo_artifact_profile']!=profile or old['registry_packages']!=registry_packages(build.decode()):raise RuntimeError('baseline profile/dependencies differ')
    cbuild=reference/'ogg-benchmark-build'
    configure=['cmake','-S',DECODE/'ogg/benches/oracle','-B',cbuild,f'-DREFERENCE_ROOT={reference}','-DCMAKE_BUILD_TYPE=Release']
    if os.name=='nt':configure+=['-A','x64']
    invoke(configure,run/'c-configure.log')
    invoke(['cmake','--build',cbuild,'--config','Release','--target','deadlib_ogg_benchmark','--parallel','4'],run/'c-build.log')
    workers['libogg']=cbuild/('Release/deadlib_ogg_benchmark.exe' if os.name=='nt' else 'deadlib_ogg_benchmark')
    generated=run/'generated';invoke([native,'generate',generated])
    files=args.files or [DECODE/'vorbis/tests/fixtures/mono.ogg',DECODE/'vorbis/tests/fixtures/stereo.ogg',REPO/'assets/music/in_two (loop).ogg']
    if not args.files and (reference/'corpus/itgmania/goin-under.ogg').exists():files.append(reference/'corpus/itgmania/goin-under.ogg')
    files=[p.resolve() for p in files]+sorted(generated.glob('*.ogg'))
    sources=[p for folder in ('ogg/src','ogg/benches') for p in (DECODE/folder).rglob('*') if p.is_file()]
    sources += [Path(__file__).resolve(),DECODE/'scripts/benchmark.py',DECODE/'ogg/Cargo.toml',REPO/'Cargo.toml',REPO/'Cargo.lock',REPO/'.cargo/config.toml']
    manifest={'started_utc':datetime.now(timezone.utc).isoformat(),'git_head':invoke(['git','rev-parse','HEAD']).decode().strip(),
        'rustc':invoke(['rustc','-Vv']).decode(),'platform':platform.platform(),'processor':platform.processor(),'logical_cpus':os.cpu_count(),
        'reference_pin':PINS['ogg'],'repeats':args.repeats,'seed':args.seed,'min_seconds':args.min_seconds,'iteration_cap':1000000,
        'workers':{k:{'path':label(v),'sha256':sha(v)} for k,v in workers.items()},'baseline':old,'cargo_artifact_profile':profile,
        'registry_packages':registry_packages(build.decode()),'cargo_profiles':tomllib.loads((REPO/'Cargo.toml').read_text(encoding='utf-8'))['profile'],
        'cargo_config':tomllib.loads((REPO/'.cargo/config.toml').read_text(encoding='utf-8')),
        'rustflags_environment':{k:os.environ.get(k) for k in ('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS')},
        'cmake_release_flags':[s for s in (cbuild/'CMakeCache.txt').read_text().splitlines() if s.startswith(('CMAKE_C_FLAGS_RELEASE:','CMAKE_CXX_FLAGS_RELEASE:','CMAKE_EXE_LINKER_FLAGS_RELEASE:'))],
        'source_sha256':{label(p):sha(p) for p in sorted(sources)},'input_sha256':{label(p):sha(p) for p in files},'verification':[],
        'scope':'Warm resident-input container APIs; fresh context plus cleanup each iteration; no file load or retained output. Native strict physical grammar is broader than libogg page/packet checks. CRC compares contiguous native input with libogg header/body checksum_set including zero/store fields. No codec synthesis.'}
    for i,path in enumerate(files):
        expected=None
        for engine,binary in workers.items():
            wire=invoke([binary,'verify',path]);(run/f'verify-{i}-{engine}.wire').write_bytes(wire)
            if expected is None:expected=wire
            elif expected!=wire:raise RuntimeError(f'page/packet bytes disagree: {path}, {engine}')
        manifest['verification'].append({'path':label(path),'wire_bytes':len(expected),'wire_sha256':__import__('hashlib').sha256(expected).hexdigest(),'all_workers_byte_exact':True})
    rows=[];rng=random.Random(args.seed);expected_counts={}
    def worker(engine,phase,iterations,source):
        fields=invoke([workers[engine],phase,iterations,source]).decode().strip().split('\t')
        identity='native' if engine=='native-baseline' else engine
        if len(fields)!=10 or fields[:2]!=[identity,phase]:raise RuntimeError(f'invalid worker output: {fields}')
        n,elapsed,pages,packets,payload,reads,consumed,checksum=map(int,fields[2:])
        if n!=iterations or elapsed<=0:raise RuntimeError('invalid count/timing')
        values=(pages,packets,payload,checksum)
        if any(v%n for v in values):raise RuntimeError('nonintegral repeated counts')
        count=tuple(v//n for v in values)
        key=(str(source),phase)
        if expected_counts.setdefault(key,count)!=count:raise RuntimeError(f'count/checksum disagreement: {key}, {engine}')
        if phase!='startup' and consumed//n!=(int(source) if phase=='crc' else Path(source).stat().st_size):raise RuntimeError('source byte count disagrees')
        return {'path':f'crc:{source}' if phase=='crc' else label(source),'engine':engine,'phase':phase,'iterations':n,'elapsed_ns':elapsed,
                'ns_per_run':elapsed/n,'pages':pages,'packets':packets,'payload_bytes':payload,'read_calls':reads,'bytes_read':consumed,'checksum_sum':checksum}
    cases=[(p,phase) for p in files for phase in ('pages','demux','stream4096','startup')]
    cases += [(n,'crc') for n in (64,256,4096,65025,1048576)]
    for source,phase in cases:
        iterations={e:min(1000000,max(1,math.ceil(args.min_seconds*1e9/worker(e,phase,1,source)['elapsed_ns']))) for e in workers}
        for trial in range(args.repeats):
            order=list(workers);rng.shuffle(order)
            for engine in order:
                row=worker(engine,phase,iterations[engine],source);row['trial']=trial;rows.append(row)
        table(run/'trials.tsv',rows);print(f'{label(source) if isinstance(source,Path) else source}: {phase} verified and timed',flush=True)
    summary=[]
    for key in dict.fromkeys((r['path'],r['engine'],r['phase']) for r in rows):
        group=[r for r in rows if (r['path'],r['engine'],r['phase'])==key];times=[r['ns_per_run'] for r in group];median=statistics.median(times);first=group[0];n=first['iterations']
        summary.append(dict(zip(('path','engine','phase'),key))|{'median_ns':median,'min_ns':min(times),'max_ns':max(times),'mad_ns':statistics.median(abs(v-median) for v in times),
            **{k+'_per_run':first[k]/n for k in ('pages','packets','payload_bytes','read_calls','bytes_read','checksum_sum')},
            'MiB_per_second':(first['bytes_read']/n/1048576*1e9/median) if first['phase']!='startup' else ''})
    table(run/'summary.tsv',summary)
    for group in ('source_sha256','input_sha256'):
        for name,h in manifest[group].items():
            if sha(REPO/name)!=h:raise RuntimeError(f'source/input drift: {name}')
    for engine,binary in workers.items():
        if sha(binary)!=manifest['workers'][engine]['sha256']:raise RuntimeError('worker drift')
    manifest.update(completed_utc=datetime.now(timezone.utc).isoformat(),timing_rows=len(rows),source_input_binary_hashes_rechecked=True)
    (run/'manifest.json').write_bytes((json.dumps(manifest,indent=2)+'\n').encode());print(f'Ogg evidence retained: {run}',flush=True)


if __name__=='__main__':main()
