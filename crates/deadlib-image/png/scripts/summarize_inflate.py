"""Summarize checked-in inflation experiments from per-case median run averages."""
from collections import defaultdict
import csv
import hashlib
import json
from pathlib import Path
from statistics import median

package = Path(__file__).resolve().parent.parent
prefixes = ['before', 'wide-trial', 'canonical-trial', 'combined-trial', 'final',
            'before-confirm', 'before-pipeline', 'final-pipeline',
            'before-confirm-pipeline', 'final-synthetic', 'before-confirm-synthetic']
groups = {}
all_rows = {}
artifacts = []
for prefix in prefixes:
    stem = f'benchmark-inflate-{prefix}'
    manifest_path = package / f'{stem}-manifest.json'
    manifest = json.loads(manifest_path.read_text(encoding='utf-8-sig'))
    cases = defaultdict(list)
    rows = []
    paths = [manifest_path]
    for run in range(1, manifest['runs'] + 1):
        path = package / f'{stem}-run{run}.tsv'
        paths.append(path)
        with path.open(encoding='utf-8-sig', newline='') as stream:
            for row in csv.DictReader(stream, delimiter='\t'):
                assert int(row['iterations']) == manifest['iterations']
                cases[(row['codec'], row['operation'], row['case'])].append(row)
                rows.append(row)
    operations = defaultdict(list)
    for (codec, operation, name), values in cases.items():
        assert len(values) == manifest['runs']
        size = {int(v['sample_bytes']) for v in values}
        assert len(size) == 1
        operations[(codec, operation)].append((size.pop(), median(int(v['ns_per_image']) for v in values)))
    groups[prefix] = [dict(codec=codec, operation=operation,
                           mib_per_second=round(sum(v[0] for v in values) * 1e9 / sum(v[1] for v in values) / 1048576, 6),
                           cases=len(values), runs=manifest['runs'], iterations=manifest['iterations'])
                      for (codec, operation), values in operations.items()]
    all_rows[prefix] = rows
    artifacts.extend({'path': p.relative_to(package).as_posix(),
                      'sha256': hashlib.sha256(p.read_bytes()).hexdigest()} for p in paths)

for old, new in [('before', 'final'), ('before-pipeline', 'final-pipeline'),
                 ('before-confirm-synthetic', 'final-synthetic')]:
    def invariants(rows):
        result = {}
        for row in rows:
            key = (row['codec'], row['operation'], row['case'])
            value = (row['sample_bytes'], row['encoded_bytes'], row['image_pipeline_bytes'])
            assert key not in result or result[key] == value
            result[key] = value
        return result
    assert invariants(all_rows[old]) == invariants(all_rows[new]), (old, new)

def rate(group, codec, operation):
    return next(r['mib_per_second'] for r in groups[group] if r['codec'] == codec and r['operation'] == operation)

comparisons = []
for final, before, control, reference, operations in [
    ('final', 'before', 'before-confirm', 'flate2-1.1.10', ['inflate', 'inflate-stream']),
    ('final-pipeline', 'before-pipeline', 'before-confirm-pipeline', 'png-0.18.1', ['decode', 'decode-reader'])]:
    reference_op = 'inflate' if final == 'final' else 'decode'
    for operation in operations:
        selected = rate(final, 'deadlib-image-png', operation)
        for base in [before, control]:
            relative = (selected / rate(final, reference, reference_op)) / (rate(base, 'deadlib-image-png', operation) / rate(base, reference, reference_op))
            comparisons.append({'operation': operation, 'baseline': base,
                                'observed_ratio': round(selected / rate(base, 'deadlib-image-png', operation), 6),
                                'reference_relative_ratio': round(relative, 6)})
output = {'method': 'Sum native/filtered bytes divided by sum of per-case medians of run-average nanoseconds. All runs have three warmups per operation. Reference-relative ratios are an inference on a shared host, not a controlled causal estimate.',
          'groups': groups, 'comparisons': comparisons, 'artifact_sha256': artifacts,
          'analysis_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
(package / 'benchmark-inflate-summary.json').write_bytes((json.dumps(output, indent=2) + '\n').encode('utf-8'))
for group, values in groups.items():
    print(group, {(v['codec'], v['operation']): round(v['mib_per_second'], 2) for v in values})
print(json.dumps(comparisons, indent=2))
