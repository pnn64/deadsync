"""Retry unchanged test executables under neutral names, without elevation."""
import hashlib
import json
import re
import shutil
import subprocess
import time
from pathlib import Path

d = Path(__file__).resolve().parent
r = d.parents[2]
out = d / 'production-song-neutral-controls-v1'
out.mkdir(exist_ok=False)
source_manifest = json.loads((d / 'production-all-song-regressions-v1-song-windows-debug/sources.json').read_text(encoding='utf-8'))
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
assert all(sha(r / name) == digest for name, digest in source_manifest.items())
base = r / 'target/rust-adapters/debug/deps'
reports = []
for original, neutral in [
    ('rust_song_update_output-63e8f1b3acf29b19.exe', 'migration-output-control-v1.exe'),
    ('rust_song_updates-a42df165a1dd4456.exe', 'migration-phase-control-v1.exe'),
]:
    source, binary = base / original, base / neutral
    assert source.is_file() and not binary.exists()
    shutil.copyfile(source, binary)
    assert sha(binary) == sha(source)
    command = [str(binary), '--nocapture', '--test-threads=1']
    started = time.monotonic()
    error = None
    with (out / (neutral + '.log')).open('wb') as stream:
        try:
            result = subprocess.run(command, cwd=r, stdout=stream, stderr=subprocess.STDOUT)
            status = result.returncode
        except OSError as failure:
            status, error = None, str(failure)
    log = (out / (neutral + '.log')).read_text(encoding='utf-8', errors='replace')
    report = dict(original=str(source), binary=str(binary), binary_sha256=sha(binary),
        original_sha256=sha(source), command=command, exit_code=status, launch_error=error,
        seconds=time.monotonic()-started,
        tests_passed=sum(map(int, re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed;', log))),
        tests_failed=sum(map(int, re.findall(r'test result: (?:ok|FAILED)\. \d+ passed; (\d+) failed;', log))),
        sources_unchanged=all(sha(r / name) == digest for name, digest in source_manifest.items()))
    reports.append(report)
    (out / 'result.json').write_text(json.dumps(reports, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(report), flush=True)
assert all(row['exit_code'] == 0 and row['sources_unchanged'] for row in reports)
