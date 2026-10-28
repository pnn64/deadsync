"""Build an isolated ASan/coverage target and preserve a fresh seeded campaign."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import time
from fuzz_corpus import write_corpus


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--output',type=Path,required=True,help='new evidence stem')
    ap.add_argument('--cargo-fuzz',type=Path,required=True)
    ap.add_argument('--toolchain',default='nightly')
    ap.add_argument('--runs',type=int,default=2000000)
    ap.add_argument('--seed',type=int,default=20261028)
    args=ap.parse_args()
    if args.runs<0 or not 0<args.seed<2**32: ap.error('invalid run count/seed')
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3]
    stem=args.output.resolve();stem.parent.mkdir(parents=True,exist_ok=True)
    for suffix in ('.json','.stdout','.stderr','.build.log','-corpus','-artifacts'):
        if Path(str(stem)+suffix).exists(): ap.error('choose a new output stem')
    corpus=Path(str(stem)+'-corpus');artifacts=Path(str(stem)+'-artifacts');artifacts.mkdir()
    seeds=write_corpus(corpus)
    paths=sorted((root/'src').rglob('*.rs'))+sorted((root/'fuzz').rglob('*.rs'))+[root/'tests/fuzz_support/mod.rs',root/'tests/fuzz_corpus.py',root/'tests/adpcm_wavl_fixtures.py',root/'tests/ima_extensible_fixtures.py',root/'tests/ima3_fixtures.py',Path(__file__),root/'Cargo.toml',root/'fuzz/Cargo.toml',root/'fuzz/Cargo.lock',root/'fuzz/wav.dict',workspace/'.cargo/config.toml']
    before={p.relative_to(workspace).as_posix():sha(p) for p in paths}
    tool=args.cargo_fuzz.resolve()
    env=os.environ.copy();env['RUSTUP_TOOLCHAIN']=args.toolchain
    runtime={}
    if os.name=='nt':
        vswhere=Path(os.environ.get('ProgramFiles(x86)',r'C:\Program Files (x86)'))/'Microsoft Visual Studio/Installer/vswhere.exe'
        matches=subprocess.check_output([str(vswhere),'-latest','-products','*','-find','**/clang_rt.asan_dynamic-x86_64.dll'],text=True).strip().splitlines()
        dll=Path(next(p for p in matches if 'Hostx64' in p))
        env['PATH']=str(dll.parent)+os.pathsep+env['PATH']
        runtime=dict(path=str(dll),sha256=sha(dll))
    rustc=subprocess.check_output(['rustc','-Vv'],env=env,text=True)
    triple=next(line.split(': ',1)[1] for line in rustc.splitlines() if line.startswith('host: '))
    target_dir=workspace/'target/wav-fuzz'
    build_command=[str(tool),'build','wav','--fuzz-dir',str(root/'fuzz'),'--target-dir',str(target_dir)]
    build=subprocess.run(build_command,cwd=workspace,env=env,capture_output=True)
    Path(str(stem)+'.build.log').write_bytes(build.stdout+build.stderr)
    if build.returncode: raise RuntimeError('ASan target build failed; inspect build log')
    binary=target_dir/triple/'release'/('wav.exe' if os.name=='nt' else 'wav')
    command=[str(binary),str(corpus),f'-runs={args.runs}',f'-seed={args.seed}','-max_len=32768','-rss_limit_mb=512','-malloc_limit_mb=64','-timeout=10','-print_final_stats=1','-dict='+str(root/'fuzz/wav.dict'),'-artifact_prefix='+str(artifacts)+os.sep]
    start=time.monotonic();run=subprocess.run(command,cwd=workspace,env=env,capture_output=True);elapsed=time.monotonic()-start
    Path(str(stem)+'.stdout').write_bytes(run.stdout);Path(str(stem)+'.stderr').write_bytes(run.stderr)
    after={p.relative_to(workspace).as_posix():sha(p) for p in paths}
    stats={k:int(v) for k,v in re.findall(rb'stat::([a-z_]+):\s+(\d+)',run.stderr)}
    stats={k.decode():v for k,v in stats.items()}
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),engine='LLVM libFuzzer with AddressSanitizer, debug assertions and overflow checks',platform=platform.platform(),rustc=rustc,tool=dict(version=subprocess.check_output([str(tool),'-V'],text=True).strip(),binary_sha256=sha(tool)),asan_runtime=runtime,sources=before,seeds=seeds,build_command=build_command,command=command,binary_sha256=sha(binary),elapsed_seconds=elapsed,exit_code=run.returncode,stats=stats,inputs_unchanged=before==after,final_corpus={p.name:sha(p) for p in sorted(corpus.iterdir()) if p.is_file()},artifacts={p.name:sha(p) for p in sorted(artifacts.iterdir()) if p.is_file()})
    Path(str(stem)+'.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(dict(exit_code=run.returncode,elapsed_seconds=elapsed,stats=stats,artifacts=report['artifacts'])))
    if run.returncode or before!=after: raise SystemExit(1)


if __name__=='__main__': main()
