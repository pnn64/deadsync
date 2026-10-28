"""Replay fresh independent seeds and preserve deterministic mutation evidence."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time
from fuzz_corpus import write_corpus


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--output',type=Path,required=True,help='new run stem; existing evidence is never overwritten')
    ap.add_argument('--iterations',type=int,default=1000000)
    ap.add_argument('--seed',type=int,default=20261028)
    ap.add_argument('--target',help='optional Cargo target, for example i686-pc-windows-msvc')
    ap.add_argument('--corpus',type=Path,help='replay/mutate copies of an existing flat corpus instead of fresh independent seeds')
    args=ap.parse_args()
    if args.iterations<0 or not 0<args.seed<2**64: ap.error('invalid iteration count/seed')
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3]
    stem=args.output.resolve();stem.parent.mkdir(parents=True,exist_ok=True)
    paths=sorted((root/'src').rglob('*.rs'))+[root/'tests/fuzz_support/mod.rs',root/'examples/mutate.rs',root/'tests/fuzz_corpus.py',Path(__file__),root/'Cargo.toml',workspace/'Cargo.toml',workspace/'Cargo.lock',workspace/'.cargo/config.toml']
    before={p.relative_to(workspace).as_posix():sha(p) for p in paths}
    for suffix in ('.json','.stdout','.stderr','.build.log','-corpus'):
        if Path(str(stem)+suffix).exists(): ap.error('choose a new output stem')
    if args.corpus:
        directory=Path(str(stem)+'-corpus');directory.mkdir()
        seeds={}
        for path in sorted(args.corpus.iterdir()):
            if path.is_file():
                data=path.read_bytes();(directory/path.name).write_bytes(data)
                seeds[path.name]=hashlib.sha256(data).hexdigest()
        if not seeds: ap.error('corpus is empty')
    else:
        seeds=write_corpus(Path(str(stem)+'-corpus'))
    command=['cargo','build','--locked','--release','-p','deadlib-audio-decode-wav','--example','mutate','--target-dir','target/wav','--message-format=json']
    if args.target: command+=['--target',args.target]
    # The main workspace's release profile aborts on panic. This test-only
    # override makes catch_unwind save a reproducer and enables arithmetic checks.
    env=os.environ.copy()
    overrides={'CARGO_PROFILE_RELEASE_PANIC':'unwind','CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS':'true','CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS':'true'}
    env.update(overrides)
    build=subprocess.run(command,cwd=workspace,env=env,capture_output=True,text=True)
    Path(str(stem)+'.build.log').write_text(build.stdout+build.stderr)
    if build.returncode: raise RuntimeError('build failed; inspect preserved build log')
    artifacts=[json.loads(line) for line in build.stdout.splitlines() if line.startswith('{')]
    binary=Path(next(a['executable'] for a in artifacts if a.get('executable') and a.get('target',{}).get('name')=='mutate'))
    command=[str(binary),str(stem)+'-corpus',str(args.iterations),str(args.seed),str(stem)+'.failure']
    run=subprocess.run(command,cwd=workspace,capture_output=True)
    Path(str(stem)+'.stdout').write_bytes(run.stdout);Path(str(stem)+'.stderr').write_bytes(run.stderr)
    after={p.relative_to(workspace).as_posix():sha(p) for p in paths}
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),engine='deterministic xorshift64 mutation; not coverage guided',profile_overrides=overrides,seed=args.seed,seed_source=str(args.corpus.resolve()) if args.corpus else 'independent fuzz_corpus.py fixtures',platform=platform.platform(),target=args.target,rustc=subprocess.check_output(['rustc','-Vv'],text=True),sources=before,seeds=seeds,binary_sha256=sha(binary),command=command,exit_code=run.returncode,inputs_unchanged=before==after)
    if run.returncode==0: report['result']=json.loads(run.stdout)
    Path(str(stem)+'.json').write_text(json.dumps(report,indent=2)+'\n')
    print(run.stdout.decode(),end='')
    if run.returncode or before!=after:
        print(run.stderr.decode(),end='');raise SystemExit(1)


if __name__=='__main__': main()
