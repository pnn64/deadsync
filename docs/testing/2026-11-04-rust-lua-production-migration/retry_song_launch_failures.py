"""Run byte-identical neutral copies only for recorded Windows 740 launch failures."""
import argparse
import hashlib
import json
import re
import shutil
import subprocess
import time
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('stage')
p.add_argument('--source', required=True, help='Completed regression stage directory beside this script.')
a = p.parse_args()
assert re.fullmatch(r'[a-z0-9-]+', a.stage)
assert re.fullmatch(r'[a-z0-9-]+', a.source)
d = Path(__file__).resolve().parent
r = d.parents[2]
source_dir = d / a.source
result = json.loads((source_dir / 'result.json').read_text(encoding='utf-8'))
assert result['sources_unchanged']
manifest = json.loads((source_dir / 'sources.json').read_text(encoding='utf-8'))
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
unchanged = lambda: all((r / name).is_file() and sha(r / name) == digest for name, digest in manifest.items())
assert unchanged()
log = (source_dir / 'run.log').read_text(encoding='utf-8', errors='replace')
pattern = r'could not execute process `([^`]+?\.exe)( --nocapture --test-threads=1)` \(never executed\)\s+Caused by:\s+[^\r\n]*\(os error 740\)'
launches = re.findall(pattern, log)
assert launches, 'No matching recorded launch-only failures; do not rerun completed tests.'
out = d / a.stage
out.mkdir(exist_ok=False)
base = (r / 'target/rust-adapters/debug/deps').resolve()
reports = []
for index, (original, _) in enumerate(launches):
    original = Path(original).resolve()
    assert original.parent == base and original.is_file()
    binary = base / f'migration-control-{a.stage}-{index}.exe'
    assert not binary.exists()
    shutil.copyfile(original, binary)
    assert sha(binary) == sha(original)
    command = [str(binary), '--nocapture', '--test-threads=1']
    started = time.monotonic()
    with (out / f'control-{index}.log').open('wb') as stream:
        completed = subprocess.run(command, cwd=r, stdout=stream, stderr=subprocess.STDOUT)
    output = (out / f'control-{index}.log').read_text(encoding='utf-8', errors='replace')
    row = dict(original=str(original), binary=str(binary), original_sha256=sha(original),
        binary_sha256=sha(binary), command=command, exit_code=completed.returncode,
        seconds=time.monotonic()-started,
        tests_passed=sum(map(int, re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed;', output))),
        tests_failed=sum(map(int, re.findall(r'test result: (?:ok|FAILED)\. \d+ passed; (\d+) failed;', output))),
        sources_unchanged=unchanged(), elevation_used=False)
    reports.append(row)
    (out / 'result.json').write_text(json.dumps(reports, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(row), flush=True)
assert all(row['exit_code'] == 0 and row['sources_unchanged'] for row in reports)
