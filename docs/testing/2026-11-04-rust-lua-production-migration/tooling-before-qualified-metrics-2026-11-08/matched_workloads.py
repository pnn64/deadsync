"""Run the frozen/current release tools in ABBA order on the original workload."""
from pathlib import Path
import argparse
import hashlib
import json
import platform
import statistics
import subprocess
import sys
import time

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('stage')
p.add_argument('--build-stage', default='production-final-workloads-windows-release')
a = p.parse_args()
d = Path(__file__).resolve().parent
root = d.parents[2]
out = d / a.stage
out.mkdir(exist_ok=False)
sha = lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
before = {row['name']: row for row in json.loads((d / 'baseline-workloads-windows-release/results.json').read_text())}
after = {row['name']: row for row in json.loads((d / a.build_stage / 'results.json').read_text())}
baseline_live = json.loads((d / 'baseline-live-dreaming-windows-release/result.json').read_text())
simfile = Path(baseline_live['command'][1])
assert sha(simfile) == baseline_live['simfile_sha256']
for row in [*before.values(), *after.values()]:
    assert row['exit_code'] == 0 and sha(row['artifact']) == row['artifact_sha256']

def compilers():
    command = [
        'powershell', '-NoProfile', '-Command',
        "@(Get-CimInstance Win32_Process -Filter \"name = 'rustc.exe' OR name = 'cargo.exe' OR name = 'clippy-driver.exe' OR name = 'link.exe' OR name = 'lld-link.exe'\" | Select-Object ProcessId,Name) | ConvertTo-Json -Compress",
    ]
    text = subprocess.check_output(command, cwd=root).decode('utf-8').strip()
    value = json.loads(text) if text else []
    return value if isinstance(value, list) else [value]

environment = dict(platform=platform.platform(), machine=platform.machine(),
    rustc=subprocess.check_output(['rustc', '-Vv'], cwd=root).decode('utf-8'),
    order='native, Rust, Rust, native for each workload; preparation uses 3 iterations per process; live uses 481 frames/60 warmup',
    timing_scope='Original complete preparation and gameplay-update tools; no GPU/audio device. Compilation checked before and after each run. Ordinary system load remains uncontrolled.')
(out / 'environment.json').write_text(json.dumps(environment, indent=2) + '\n')
rows = []
for kind, tool in [('prepare', 'song_lua_performance'), ('live', 'song_lua_execution')]:
    for index, backend in enumerate(['native', 'rust', 'rust', 'native']):
        active = compilers()
        assert not active, f'Compilation is active; do not label this a controlled pair: {active}'
        build = before if backend == 'native' else after
        stage = f'{a.stage}-{kind}-{index}-{backend}'
        command = [sys.executable, str(d / 'run_workload.py'), stage, kind,
                   build[tool]['artifact'], str(simfile), '--frames', '481', '--iterations', '3']
        print(f'measuring {kind} {index} {backend}', flush=True)
        started = time.monotonic()
        result = subprocess.run(command, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        (out / f'{kind}-{index}-{backend}.log').write_bytes(result.stdout)
        assert result.returncode == 0, result.stdout.decode(errors='replace')[-4000:]
        record = json.loads((d / stage / 'result.json').read_text())
        output = json.loads((d / stage / 'output.json').read_text())
        assert record['inputs_unchanged'] and record['inputs_sha256'] == baseline_live['inputs_sha256']
        if kind == 'live':
            assert output['startup_smoke_passed'] and not output['errors']
            assert output['frames'] == 481 and output['benchmark']['warmup_frames'] == 60
            assert output['benchmark']['debug_assertions'] is False
        row = dict(stage=stage, kind=kind, backend=backend, seconds=time.monotonic()-started,
                   record=record, output_sha256=sha(d / stage / 'output.json'),
                   compilation_after=compilers())
        rows.append(row)
        (out / 'runs.json').write_text(json.dumps(rows, indent=2) + '\n')
        assert not row['compilation_after'], 'Compilation appeared during a measurement; keep this run as qualified evidence.'

summary = {}
for kind in ['prepare', 'live']:
    summary[kind] = {}
    for backend in ['native', 'rust']:
        selected = [row for row in rows if row['kind'] == kind and row['backend'] == backend]
        outputs = [json.loads((d / row['stage'] / 'output.json').read_text()) for row in selected]
        samples = [s for output in outputs for s in (output['samples'] if kind == 'prepare' else output['benchmark']['samples'])]
        total_seconds = sum(s['wall_seconds'] for s in samples)
        total_cycles = sum(s['thread_cpu_cycles'] for s in samples)
        summary[kind][backend] = dict(processes=len(selected), samples=len(samples),
            median_sample_seconds=statistics.median(s['wall_seconds'] for s in samples),
            mean_sample_seconds=total_seconds / len(samples), mean_thread_cycles=total_cycles / len(samples),
            median_process_peak_rss_bytes=statistics.median(row['record']['peak_rss_bytes'] for row in selected),
            median_process_peak_commit_bytes=statistics.median(row['record']['peak_commit_bytes'] for row in selected))
    old, new = summary[kind]['native'], summary[kind]['rust']
    summary[kind]['rust_over_native'] = {key: new[key] / old[key] for key in [
        'median_sample_seconds', 'mean_sample_seconds', 'mean_thread_cycles',
        'median_process_peak_rss_bytes', 'median_process_peak_commit_bytes']}
report = dict(environment=environment, summary=summary,
    limitations=[
        'Application snapshots include preserved unrelated work; these measurements do not isolate every change to the VM.',
        'Process memory includes the complete tool and assets. Native malloc is excluded from legacy Rust GlobalAlloc counters, so allocation-count ratios are not reported.',
        'The unchanged native visual hash varies across processes through allocation-identity message ordering; raw hashes are retained, not forced equal.',
        'This reports representative before/after costs without a LuaJIT or optimization target.'])
(out / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(summary), flush=True)
