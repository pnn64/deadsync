"""Run the recorded normal-application gameplay smoke with isolated user state."""
import hashlib
import json
import os
import shutil
import subprocess
import time
from pathlib import Path
d = Path(__file__).resolve().parent
r = d.parents[2]
baseline = d / 'baseline-rendered-dreaming-windows'
build = d / 'production-routing-build-v1-application-windows-debug'
assert json.loads((build / 'result.json').read_text())['exit_code'] == 0
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
sources = json.loads((build / 'sources.json').read_text())
assert all(sha(r / name) == digest for name, digest in sources.items())
out = d / 'production-rendered-dreaming-windows-v1'
out.mkdir(exist_ok=False)
data = out / 'data'
data.mkdir()
config = baseline / 'config.ini'
case = json.loads((baseline / 'manifest.json').read_text())
assert sha(config) == case['config_sha256']
assert sha(Path(case['simfile'])) == case['simfile_sha256']
shutil.copyfile(config, data / 'deadsync.ini')
case.update(name='lua-migration-rust-dreaming-v1', data_dir=str(data), artifact_dir=str(out / 'run'))
manifest = out / 'manifest.json'
manifest.write_text(json.dumps(case, indent=2) + '\n')
binary = r / 'target/rust-adapters/debug/deadsync.exe'
preserved = r / 'target/rust-lua-production-current/artifacts/windows/debug/deadsync.exe'
assert binary.is_file() and not preserved.exists()
preserved.parent.mkdir(parents=True, exist_ok=True)
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
report = dict(exit_code=status, seconds=time.monotonic()-started, binary_sha256=sha(binary),
    result_exists=(out / 'run/result.json').is_file(),
    screenshots=[str(p.relative_to(out)) for p in sorted(data.rglob('*.png'))],
    log_sha256=sha(out / 'run.log'), sources_unchanged=all(sha(r / name) == digest for name, digest in sources.items()))
(out / 'process.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report), flush=True)
assert status == 0 and report['result_exists'] and report['screenshots'] and report['sources_unchanged']
