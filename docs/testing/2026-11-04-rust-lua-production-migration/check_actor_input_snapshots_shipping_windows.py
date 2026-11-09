"""Check the Rust song adapter without test support on the native host."""
import hashlib
import json
import os
import subprocess
import time
from pathlib import Path

d = Path(__file__).resolve().parent
r = d.parents[2]
out = d / 'actor-input-snapshots-shipping-windows-v1'
out.mkdir(exist_ok=False)
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
assert os.name == 'nt'

def sources():
    names = subprocess.check_output([
        'git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--',
        *[f'crates/{name}' for name in ['deadlib-lua', 'deadsync-song-lua',
          'deadsync-noteskin', 'deadsync-theme-simply-love', 'deadsync-assets',
          'deadsync-shell', 'deadsync-notefield']]], cwd=r).decode().split('\0')
    paths = {r / name for name in names if name and Path(name).suffix in {'.rs', '.lua', '.toml'}}
    paths.update(r / name for name in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml'])
    return {p.relative_to(r).as_posix(): sha(p) for p in sorted(paths) if p.is_file()}

before = sources()
(out / 'sources.json').write_text(json.dumps(before, indent=2) + '\n')
env = os.environ.copy()
env.update(CARGO_TARGET_DIR=str(r / 'target/rust-adapters'), CARGO_INCREMENTAL='0',
           CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0', CARGO_BUILD_JOBS='4')
cmd = ['cargo', 'check', '--locked', '-p', 'deadsync-song-lua', '--features', 'rust-engine', '--lib']
started = time.monotonic()
print('actor-input-snapshots shipping Windows check started', flush=True)
with (out / 'run.log').open('wb') as stream:
    result = subprocess.run(cmd, cwd=r, env=env, stdout=stream, stderr=subprocess.STDOUT)
report = dict(command=cmd, exit_code=result.returncode, seconds=time.monotonic()-started,
              sources_unchanged=before == sources(), log_sha256=sha(out / 'run.log'))
(out / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report), flush=True)
assert result.returncode == 0 and report['sources_unchanged']
