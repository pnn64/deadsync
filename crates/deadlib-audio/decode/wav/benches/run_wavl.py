"""Build/run wavl regression with stable input fingerprints and preserved raw trials."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--output', type=Path, required=True, help='new output stem')
    args = ap.parse_args()
    root = Path(__file__).resolve().parents[1]
    workspace = root.parents[3]
    stem = args.output.resolve()
    paths = sorted((root/'src').rglob('*.rs')) + [root/'Cargo.toml', root/'benches/wavl.rs', Path(__file__),
              workspace/'Cargo.toml', workspace/'Cargo.lock', workspace/'.cargo/config.toml']
    for suffix in ('.tsv', '.json', '.build.log', '.stderr'):
        if Path(str(stem)+suffix).exists():
            ap.error('preserve prior runs; choose a new output stem')
    stem.parent.mkdir(parents=True, exist_ok=True)
    before = {p.relative_to(workspace).as_posix(): sha(p) for p in paths}
    command = ['cargo','bench','--locked','-p','deadlib-audio-decode-wav','--bench','wavl',
               '--no-run','--message-format=json','--target-dir','target/wav']
    build = subprocess.run(command, cwd=workspace, capture_output=True, text=True)
    Path(str(stem)+'.build.log').write_text(build.stdout+build.stderr)
    if build.returncode:
        raise RuntimeError('build failed; see build log')
    after = {p.relative_to(workspace).as_posix(): sha(p) for p in paths}
    if before != after:
        raise RuntimeError('build inputs changed during compilation')
    artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith('{')]
    binary = Path(next(a['executable'] for a in artifacts if a.get('executable') and a.get('target',{}).get('name')=='wavl'))
    cpu = platform.processor()
    if os.name == 'nt':
        cpu = subprocess.check_output(['powershell','-NoProfile','-Command',
              'Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json -Compress'],text=True).strip()
    run = subprocess.run([str(binary)], cwd=workspace, capture_output=True)
    Path(str(stem)+'.tsv').write_bytes(run.stdout)
    Path(str(stem)+'.stderr').write_bytes(run.stderr)
    metadata = dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),
                    platform=platform.platform(),machine=platform.machine(),cpu=cpu,
                    rustc=subprocess.check_output(['rustc','-Vv'],text=True),command=command,
                    binary_sha256=sha(binary),sources=before,exit_code=run.returncode,
                    cycle_counter='QueryThreadCycleTime' if os.name=='nt' else 'unavailable (zero)',
                    heap_scope='single warmed operation, requested allocations only; caller input/output excluded',
                    timing_scope='three warmed in-process trials; resident input; no affinity or frequency control')
    Path(str(stem)+'.json').write_text(json.dumps(metadata,indent=2)+'\n')
    print(run.stdout.decode(),end='')
    if run.returncode:
        raise RuntimeError('benchmark failed; see preserved stderr')


if __name__ == '__main__':
    main()
