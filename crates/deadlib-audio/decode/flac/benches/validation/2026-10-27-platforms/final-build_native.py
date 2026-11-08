"""Build and preserve a benchmark with build-time source provenance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists() or Path(str(output)+'.json').exists():
        parser.error('output must be new; preserve previous benchmark binaries')
    root = Path(__file__).resolve().parents[1]
    repo = root.parents[3]
    paths = [repo/'Cargo.toml', repo/'Cargo.lock', root/'Cargo.toml', root/'README.md', root/'benches/decode.rs',
             root.parent/'ogg/Cargo.toml']
    paths += sorted((root/'src').glob('*.rs')) + sorted((root.parent/'ogg/src').rglob('*.rs'))
    paths += [p for p in (repo/'.cargo').glob('config*') if p.is_file()]
    paths += [repo/name for name in ['rust-toolchain', 'rust-toolchain.toml'] if (repo/name).is_file()]
    before = {p.relative_to(repo).as_posix(): sha(p) for p in paths}
    cargo = shutil.which('cargo') or str(Path.home()/'.cargo/bin/cargo')
    command = [cargo, 'bench', '--locked', '--color', 'never', '-p', 'deadlib-audio-decode-flac',
               '--bench', 'decode', '--no-run', '--target-dir', str(args.target_dir.resolve())]
    result = subprocess.run(command, cwd=repo, capture_output=True, text=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    Path(str(output)+'.build.log').write_text(result.stdout+result.stderr)
    if result.returncode:
        raise RuntimeError(f'build exit {result.returncode}; see {output}.build.log')
    after = {p.relative_to(repo).as_posix(): sha(p) for p in paths}
    if before != after:
        raise RuntimeError('build inputs changed during compilation; preserve a fresh stable build')
    artifact = Path(re.findall(r'Executable benches[\\/]decode.rs \(([^)]+)\)', result.stderr)[-1])
    if not artifact.is_absolute():
        artifact = repo/artifact
    shutil.copyfile(artifact, output)
    shutil.copymode(artifact, output)
    report = {'built_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
              'command': command, 'artifact': str(artifact), 'sha256': sha(output),
              'sources': before, 'driver_sha256': sha(Path(__file__)),
              'cargo': subprocess.check_output([cargo, '-Vv'], cwd=repo, text=True).strip(),
              'rustc': subprocess.check_output([str(Path(cargo).with_name('rustc')), '-Vv'], cwd=repo, text=True).strip(),
              'environment': {k: v for k, v in os.environ.items() if k in ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_TARGET', 'RUSTC_WRAPPER'] or k.startswith(('CARGO_PROFILE_BENCH_', 'CARGO_PROFILE_RELEASE_'))}}
    Path(str(output)+'.json').write_text(json.dumps(report, indent=2)+'\n')
    print(output, report['sha256'])


if __name__ == '__main__':
    main()
