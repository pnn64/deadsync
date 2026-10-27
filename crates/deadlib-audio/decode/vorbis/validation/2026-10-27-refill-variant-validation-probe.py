"""Check rejected refill sources in isolated workspaces; never alter production."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess

repo = Path.cwd()
decode = repo / 'crates/deadlib-audio/decode'
root = repo / 'target/deadlib-audio-reference/refill-variants-check-final-20261027'
root.mkdir(exist_ok=False)
checks = {}
for name, folder, source in [
    ('masked', 'word-refill-pipeline-20261027', 'masked-bitpack.rs'),
    ('window', 'bit-window-pipeline-20261027', 'bit-window-bitpack.rs'),
]:
    run = root / name
    run.mkdir()
    for package in ('ogg', 'vorbis'):
        destination = run / package
        destination.mkdir()
        shutil.copytree(decode / package / 'src', destination / 'src')
        shutil.copytree(decode / package / 'benches', destination / 'benches')
        shutil.copyfile(decode / package / 'Cargo.toml', destination / 'Cargo.toml')
    (run / 'vorbis/tests').mkdir()
    shutil.copyfile(decode / 'vorbis/tests/primitives.rs', run / 'vorbis/tests/primitives.rs')
    snapshot = repo / 'target/deadlib-audio-reference' / folder / source
    shutil.copyfile(snapshot, run / 'vorbis/src/bitpack.rs')
    (run / 'Cargo.toml').write_text('[workspace]\nmembers = ["ogg", "vorbis"]\nresolver = "3"\n', encoding='utf8', newline='\n')
    shutil.copyfile(repo / 'Cargo.lock', run / 'Cargo.lock')
    args = ['cargo', 'test', '--offline', '--manifest-path', str(run / 'Cargo.toml'),
        '-p', 'deadlib-audio-decode-vorbis', '--lib', '--test', 'primitives']
    result = subprocess.run(args, cwd=repo, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    text = '\n'.join(line.rstrip() for line in result.stdout.decode('utf8').splitlines()).rstrip('\n') + '\n'
    (root / (name + '.log')).write_text(text, encoding='utf8', newline='\n')
    checks[name] = {'command': args, 'exit_code': result.returncode,
        'source_sha256': {p.relative_to(run).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
            for package in ('ogg', 'vorbis') for p in (run / package).rglob('*') if p.is_file()},
        'tested_bitpack_snapshot_sha256': hashlib.sha256(snapshot.read_bytes()).hexdigest(),
        'scope': 'Default debug profile; native unit and primitive semantic checks, not performance measurements.'}
    (root / 'checks.json').write_text(json.dumps(checks, indent=2) + '\n', encoding='utf8', newline='\n')
    print(name, result.returncode, flush=True)
    if result.returncode:
        print(text, flush=True)
        raise SystemExit(result.returncode)
