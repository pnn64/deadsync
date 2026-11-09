"""Record the existing paired bundled-noteskin preparation workload."""
from pathlib import Path
import argparse
import hashlib
import json
import platform
import statistics
import subprocess
import time

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('stage')
p.add_argument('--build-stage', default='production-final-workloads-windows-release')
a = p.parse_args()
d = Path(__file__).resolve().parent
root = d.parents[2]
out = d / a.stage
out.mkdir(exist_ok=False)
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
build = next(row for row in json.loads((d / a.build_stage / 'results.json').read_text()) if row['name'] == 'compare_lua_engines')
binary = Path(build['artifact'])
assert build['exit_code'] == 0 and sha(binary) == build['artifact_sha256']

def compilers():
    raw = subprocess.check_output(['powershell', '-NoProfile', '-Command',
        "@(Get-CimInstance Win32_Process -Filter \"name = 'cargo.exe' OR name = 'rustc.exe' OR name = 'clippy-driver.exe' OR name = 'link.exe' OR name = 'lld-link.exe'\" | Select-Object ProcessId,Name) | ConvertTo-Json -Compress"], cwd=root).decode().strip()
    value = json.loads(raw) if raw else []
    return value if isinstance(value, list) else [value]

def inputs():
    return {path.relative_to(root).as_posix(): sha(path) for path in sorted((root / 'assets/noteskins').rglob('*')) if path.is_file()}

before = inputs()
(out / 'inputs.json').write_text(json.dumps(before, indent=2) + '\n')
sources = json.loads((d / 'production-final-workloads-windows-release/sources.json').read_text())
relevant = {name: digest for name, digest in sources.items() if name.startswith(('crates/deadlib-lua/', 'crates/deadsync-noteskin/')) or name in ['Cargo.toml', 'Cargo.lock', '.cargo/config.toml']}
assert all(sha(root / name) == digest for name, digest in relevant.items())
(out / 'sources.json').write_text(json.dumps(relevant, indent=2) + '\n')
active = compilers()
assert not active, active
command = [str(binary), str(root / 'assets/noteskins'), str(out / 'output.json'), '3']
start = time.monotonic()
with (out / 'run.log').open('wb') as stream:
    result = subprocess.run(command, cwd=root, stdout=stream, stderr=subprocess.STDOUT)
after_compilers = compilers()
record = dict(command=command, exit_code=result.returncode, seconds=time.monotonic()-start,
    binary_sha256=sha(binary), platform=platform.platform(),
    compilation_before=active, compilation_after=after_compilers,
    inputs_unchanged=before == inputs(), sources_unchanged=all(sha(root / name) == digest for name, digest in relevant.items()),
    log_sha256=sha(out / 'run.log'))
(out / 'result.json').write_text(json.dumps(record, indent=2) + '\n')
assert result.returncode == 0 and record['inputs_unchanged'] and record['sources_unchanged']
assert not after_compilers, 'Concurrent compilation appeared; retain qualified evidence.'
data = json.loads((out / 'output.json').read_text())
assert not data['debug_assertions'] and all(case['complete_output_equal'] for case in data['cases'])
totals = {backend: {metric: sum(statistics.median(s[metric] for s in case['samples'] if s['backend'] == backend) for case in data['cases']) for metric in ['wall_seconds', 'thread_cpu_cycles']} for backend in ['legacy', 'rust']}
summary = dict(cases=len(data['cases']), samples_per_backend_per_case=3, all_complete_outputs_equal=True,
    sum_of_per_case_medians=totals, rust_over_native={metric: totals['rust'][metric] / totals['legacy'][metric] for metric in totals['rust']},
    limitations=data['limitations'], output_sha256=sha(out / 'output.json'))
(out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary), flush=True)
