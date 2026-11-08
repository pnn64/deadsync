import csv
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
import random
import statistics
import subprocess

repo = Path.cwd()
run = repo / 'target/deadlib-audio-reference/single-pass-resources-20261027'
out = run / 'batch-recheck'
out.mkdir(exist_ok=False)
manifest = json.loads((run / 'manifest.json').read_text())
saved = json.loads((run / 'saved-binaries.json').read_text())
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
workers = {e: repo / saved[k]['path'] for e, k in [('native', 'rust_timing'), ('native-baseline', 'rust_baseline')]}
for e, k in [('native', 'rust_timing'), ('native-baseline', 'rust_baseline')]:
    assert sha(workers[e]) == saved[k]['sha256'] == manifest['workers'][k]['sha256']
original = list(csv.DictReader((run / 'seek-summary.tsv').open(), delimiter='\t'))
rng = random.Random(1338)
rows = []
for number, (name, geometry) in enumerate(manifest['workloads'].items()):
    source, workload = repo / name, run / f'workloads/{number}-seek.bin'
    assert sha(source) == manifest['input_sha256'][name] and sha(workload) == geometry['sha256']
    def worker(engine, iterations):
        args = [str(workers[engine]), 'native', 'seek_batch', str(iterations), str(source), str(workload)]
        result = subprocess.run(args, capture_output=True, text=True)
        assert result.returncode == 0, result.stderr
        fields = result.stdout.strip().split('\t')
        assert len(fields) == 10 and fields[:2] == ['native', 'seek_batch']
        n, elapsed, units, reads, consumed, seeks, tells, exposed = map(int, fields[2:])
        assert n == iterations and elapsed > 0 and units == geometry['expected_channel_samples'] * n
        previous = next(r for r in original if (r['path'],r['engine'],r['phase']) == (name,engine,'seek_batch'))
        for value, key in [(reads, 'reads_per_run'), (consumed, 'bytes_read_per_run'), (seeks, 'seeks_per_run'), (tells, 'tells_per_run'), (exposed, 'api_exposed_samples_per_run')]:
            assert value / n == float(previous[key]), (name, engine, key)
        return {'path': name, 'engine': engine, 'phase': 'seek_batch', 'iterations': n,
                'elapsed_ns': elapsed, 'ns_per_run': elapsed / n, 'units': units}
    counts = {e: min(10000, max(1, math.ceil(0.15e9 / worker(e, 1)['elapsed_ns']))) for e in workers}
    for trial in range(15):
        order = list(workers); rng.shuffle(order)
        for engine in order:
            row = worker(engine, counts[engine]); row['trial'] = trial; rows.append(row)
    print(name, '15 paired trials completed', flush=True)
def table(path, values):
    with path.open('w', newline='', encoding='utf-8') as f:
        w = csv.DictWriter(f, fieldnames=list(values[0]), delimiter='\t', lineterminator='\n')
        w.writeheader(); w.writerows(values)
summary = []
for name in manifest['workloads']:
    for engine in workers:
        times = [r['ns_per_run'] for r in rows if (r['path'],r['engine']) == (name,engine)]
        median = statistics.median(times)
        summary.append({'path': name,'engine': engine,'median_ns': median,'min_ns': min(times),'max_ns': max(times),
                        'mad_ns': statistics.median(abs(t-median) for t in times)})
for name, digest in manifest['source_sha256'].items(): assert sha(repo/name) == digest, name
table(out/'trials.tsv', rows); table(out/'summary.tsv', summary)
(out/'manifest.json').write_bytes((json.dumps({'completed_utc': datetime.now(timezone.utc).isoformat(),
    'parent_manifest_sha256': sha(run/'manifest.json'), 'repeats': 15, 'seed': 1338, 'min_seconds': 0.15,
    'workers': {k:saved[k] for k in ['rust_timing','rust_baseline']}, 'timing_rows':len(rows),
    'same_pcm_count_io_and_exposed_work_verified_every_iteration': True,
    'source_input_workload_binary_hashes_rechecked': True}, indent=2)+'\n').encode())
print('Retained 150 recheck timing rows.')
