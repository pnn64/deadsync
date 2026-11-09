"""Run the existing gameplay case and reject warnings or errors in the app log."""
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import time
from pathlib import Path

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('stage')
p.add_argument('--build', required=True)
a = p.parse_args()
d = Path(__file__).resolve().parent
r = d.parents[2]
baseline = d / 'baseline-rendered-dreaming-windows'
build = d / a.build
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
build_result = json.loads((build / 'result.json').read_text())
assert build_result['exit_code'] == 0
sources = json.loads((build / 'sources.json').read_text())
assert all(sha(r / name) == digest for name, digest in sources.items())
out = d / a.stage
out.mkdir(exist_ok=False)
data = out / 'data'
data.mkdir()
config = baseline / 'config.ini'
case = json.loads((baseline / 'manifest.json').read_text())
assert sha(config) == case['config_sha256']
assert sha(Path(case['simfile'])) == case['simfile_sha256']
shutil.copyfile(config, data / 'deadsync.ini')
case.update(name=a.stage, data_dir=str(data), artifact_dir=str(out / 'run'))
manifest = out / 'manifest.json'
manifest.write_text(json.dumps(case, indent=2) + '\n')
artifact = build_result.get('artifact')
binary = Path(artifact['path']) if artifact else r / 'target/rust-adapters/debug/deadsync.exe'
if artifact:
    assert sha(binary) == artifact['sha256']
preserved = r / 'target/rust-lua-production-current' / a.stage / 'deadsync.exe'
assert binary.is_file() and not preserved.exists()
preserved.parent.mkdir(parents=True, exist_ok=False)
shutil.copyfile(binary, preserved)
assert sha(preserved) == sha(binary)
env = os.environ.copy()
env['DEADSYNC_CASE_SCREENSHOT'] = '1'
startup = subprocess.STARTUPINFO()
startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
startup.wShowWindow = 0
cmd = [str(binary), '--perf-case', str(manifest)]
started = time.monotonic()
with (out / 'run.log').open('wb') as stream:
    process = subprocess.Popen(cmd, cwd=r, env=env, stdout=stream, stderr=subprocess.STDOUT,
                               startupinfo=startup, creationflags=subprocess.CREATE_NO_WINDOW)
    (out / 'started.json').write_text(json.dumps(dict(pid=process.pid, command=cmd,
        binary_sha256=sha(binary), preserved_binary=str(preserved),
        manifest_sha256=sha(manifest), baseline_manifest_sha256=sha(baseline / 'manifest.json')),
        indent=2) + '\n')
    print(f'Owned application PID {process.pid}; isolated gameplay smoke started.', flush=True)
    status = process.wait()
log = (out / 'run.log').read_text(errors='replace')
problems = [line for line in log.splitlines() if re.search(r'\b(?:WARN|ERROR)\b', line)]
report = dict(exit_code=status, seconds=time.monotonic()-started, binary_sha256=sha(binary),
    result_exists=(out / 'run/result.json').is_file(),
    screenshots=[str(path.relative_to(out)) for path in sorted(data.rglob('*.png'))],
    warnings_or_errors=problems, log_sha256=sha(out / 'run.log'),
    sources_unchanged=all(sha(r / name) == digest for name, digest in sources.items()))
report['validation_passed'] = bool(status == 0 and report['result_exists'] and
    report['screenshots'] and report['sources_unchanged'] and not problems)
(out / 'process.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report), flush=True)
assert report['validation_passed']
