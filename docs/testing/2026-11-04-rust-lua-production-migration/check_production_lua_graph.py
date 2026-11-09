"""Verify normal/build application edges with and without instrumentation."""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path
d = Path(__file__).resolve().parent
r = d.parents[2]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('stage', nargs='?', default='production-lua-graph-v1')
a = p.parse_args()
out = d / a.stage
out.mkdir(exist_ok=False)

def inputs():
    names = subprocess.check_output([
        'git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', '--',
        'Cargo.toml', 'Cargo.lock', '.cargo/config.toml', 'crates/**/Cargo.toml',
    ], cwd=r).decode('utf-8').split('\0')
    return {name.replace('\\', '/'): hashlib.sha256((r / name).read_bytes()).hexdigest()
            for name in sorted(set(names)) if name and (r / name).is_file()}

before = inputs()
(out / 'inputs.json').write_text(json.dumps(before, indent=2) + '\n')
rows = []
for name, features in [('normal', []), ('instrumented', ['--features', 'deadsync-song-lua/test-support'])]:
    cmd = ['cargo', 'tree', '--locked', '-p', 'deadsync', '--edges', 'normal,build',
           '--prefix', 'none', '--format', '{p}', *features]
    result = subprocess.run(cmd, cwd=r, capture_output=True)
    (out / (name + '.stdout')).write_bytes(result.stdout)
    (out / (name + '.stderr')).write_bytes(result.stderr)
    assert result.returncode == 0, result.stderr.decode(errors='replace')
    names = {line.split()[0] for line in result.stdout.decode().splitlines() if line.split()}
    forbidden = names & {'deadlib-lua-legacy', 'deadlib-lua-legacy-sys', 'mlua', 'mlua-sys',
                         'mlua_derive', 'lua-src', 'luajit-src'}
    assert not forbidden, forbidden
    assert 'deadlib-lua' in names
    rows.append(dict(name=name, command=cmd, package_names=len(names),
                     legacy_runtime_packages=sorted(forbidden),
                     stdout_sha256=hashlib.sha256(result.stdout).hexdigest()))
unchanged = before == inputs()
(out / 'guard.json').write_text(json.dumps(dict(inputs_unchanged=unchanged,
    input_files=len(before)), indent=2) + '\n')
assert unchanged
(out / 'result.json').write_text(json.dumps(rows, indent=2) + '\n')
print(json.dumps(rows), flush=True)
