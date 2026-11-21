"""Record the exact sources and immutable executables used for qualification."""
import hashlib
import json
from pathlib import Path
import platform
import subprocess

root = Path(__file__).resolve().parents[2]
work = Path(__file__).resolve().parent
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def read_json(path):
    data = path.read_bytes()
    return json.loads(data.decode('utf-16' if data[:2] in (b'\xff\xfe', b'\xfe\xff') else 'utf-8-sig'))

baseline = read_json(work / 'baseline-hashes.json')
main = root / 'crates/deadlib-image/png/bench/src/main.rs'
baseline_main = next(item['Hash'] for item in baseline if item['Path'].replace('\\', '/').endswith('/bench/src/main.rs'))
assert digest(main) == baseline_main.lower(), 'timing harness changed since baseline'
assert digest(work / 'final-timing.exe') == digest(work / 'stored-timing.exe')

sources = sorted((root / 'crates/deadlib-image/png/src').rglob('*.rs'))
sources += sorted((root / 'crates/deadlib-image/png/bench/src').rglob('*.rs'))
sources += [root / path for path in (
    'Cargo.toml', 'Cargo.lock', '.cargo/config.toml',
    'crates/deadlib-image/Cargo.toml', 'crates/deadlib-image/src/lib.rs',
    'crates/deadlib-image/png/Cargo.toml',
    'crates/deadlib-image/png/bench/Cargo.toml',
    'crates/deadlib-image/png/bench/Cargo.lock',
    'crates/deadlib-image/png/bench/summarize.py',
)]
source_hashes = {path.relative_to(root).as_posix(): digest(path) for path in sources}
binary_hashes = {path.name: digest(path) for path in sorted(work.glob('*.exe'))}
cpu_command = 'Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors | ConvertTo-Json -Compress'
hardware = json.loads(subprocess.check_output(['powershell.exe', '-NoProfile', '-Command', cpu_command], text=True))
manifest = {
    'base_commit': 'b04e37657648a6af594c4591486faa3a301263ef',
    'compatibility_fix_commit': '3fc2deedb61f6655641f8f235c32ec5b53a0e7e9',
    'head_before_performance_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
    'platform': platform.platform(),
    'hardware': hardware,
    'rustc': subprocess.check_output(['rustc', '-vV'], text=True).strip(),
    'build': {
        'profile': 'release', 'opt_level': 3, 'lto': True, 'codegen_units': 1,
        'panic': 'abort', 'target_feature': '+crt-static',
        'target_cpu_native': False, 'timing_features': [], 'allocation_features': ['allocations'],
        'reference_versions': {'png': '0.18.1', 'image': '0.25.10'},
        'environment': read_json(work / 'build-environment.json'),
    },
    'measurement': {
        'logical_cpu': 0, 'batches': 5, 'rgba8_minimum_ms': 80, 'raw_minimum_ms': 40,
        'cycles': 'Windows QueryThreadCycleTime; median selected independently of wall time',
        'weighting': 'Totals represent one decode of each of the 116 inputs; per-file geometric means are also retained.',
    },
    'harness_unchanged_since_baseline': True,
    'timing_binary_origin': 'final-timing.exe is a byte-identical copy of stored-timing.exe, built after the stored-block compatibility fix and four retained optimizations.',
    'benchmark_manifest_note': 'The final standalone workspace adds only the flate2 1.1.11 test oracle already present transitively. No resolved package version changed; this dev dependency does not change the timing executable.',
    'sources_sha256': source_hashes,
    'executables_sha256': binary_hashes,
}
(work / 'final-hashes.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
print(json.dumps({'source_files': len(source_hashes), 'executables': len(binary_hashes), 'harness_unchanged': True, 'hardware': hardware}))
