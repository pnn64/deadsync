import csv
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess
import sys

repo = Path.cwd()
decode = repo / 'crates/deadlib-audio/decode'
root = repo / 'target/deadlib-audio-reference'
prior = root / 'encoding-entropy-20261027'
run = root / 'unit-twiddle-encoding-20261027'
run.mkdir(exist_ok=True)
sys.path.insert(0, str(decode / 'scripts'))
from benchmark import rust_worker, registry_packages
from benchmark_encoding import source_snapshot

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def invoke(command):
    result = subprocess.run([str(x) for x in command], capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f'{command}: {result.stderr}')
    return result.stdout

sources = source_snapshot()
build = subprocess.run(['cargo', 'bench', '--locked', '-p', 'deadlib-audio-decode-vorbis',
                        '--bench', 'encoding', '--no-run', '--message-format=json'], capture_output=True, text=True)
(run / 'build.log').write_bytes((build.stdout + build.stderr).encode())
assert build.returncode == 0, build.stderr
worker, profile = rust_worker(build.stdout, 'encoding')
worker_hash = sha(worker)
manifest = json.loads((prior / 'manifest.json').read_text())
assert profile == manifest['cargo_artifact_profile']
assert registry_packages(build.stdout) == manifest['registry_packages']
qualities = list(csv.DictReader((prior / 'quality.tsv').open(), delimiter='\t'))
records = []
for row in qualities:
    if row['engine'] != 'native':
        continue
    number = list(manifest['prepared_inputs']).index(row['path'])
    geometry = manifest['prepared_inputs'][row['path']]
    pcm = prior / f'input-{number}.pcm'
    assert sha(pcm) == geometry['pcm_sha256']
    old_pcm = prior / f'output-{number}-native-{row["setting"]}.pcm'
    assert sha(old_pcm) == row['decoded_reference_sha256']
    output = run / f'output-{number}-native-{row["setting"]}.ogg'
    values = invoke([worker, 'encode', row['setting'], 1, pcm, output]).strip().split('\t')
    assert len(values) == 12 and values[:3] == ['native', row['setting'], '1']
    assert sha(output) == row['encoded_sha256'], (row['path'], row['setting'])
    result = json.loads(invoke([worker, 'verify', pcm, output, old_pcm]))
    assert (result['frames'], result['channels'], result['rate']) == (geometry['frames'], geometry['channels'], geometry['rate'])
    records.append({'path': row['path'], 'setting': row['setting'], 'encoded_sha256': sha(output),
                    'prior_libvorbis_pcm_sha256': sha(old_pcm), 'encoded_bytes_identical': True, **result})
    print(row['path'], row['setting'], 'byte-identical; native/C/Symphonia PCM verified', flush=True)
assert len(records) == 24
assert source_snapshot() == sources
assert sha(worker) == worker_hash
result = {'completed_utc': datetime.now(timezone.utc).isoformat(), 'source_sha256': sources,
          'prior_benchmark_manifest_sha256': sha(prior / 'manifest.json'), 'cargo_artifact_profile': profile,
          'registry_packages': registry_packages(build.stdout), 'worker_path': str(worker), 'worker_sha256': worker_hash,
          'cases': records, 'all_native_encoded_bytes_identical': True,
          'channel_samples_per_decoder': sum(r['frames'] * r['channels'] for r in records)}
(run / 'checks.json').write_bytes((json.dumps(result, indent=2) + '\n').encode())
print('Verified all 24 native outputs and', result['channel_samples_per_decoder'], 'channel samples per decoder.')
