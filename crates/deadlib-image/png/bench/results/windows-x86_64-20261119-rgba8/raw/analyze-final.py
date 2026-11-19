"""Summarize the frozen PNG runs without changing the measurement harness."""
import importlib.util
import json
from pathlib import Path
import statistics

root = Path(__file__).resolve().parents[2]
work = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('png_summary', root / 'crates/deadlib-image/png/bench/summarize.py')
summary = importlib.util.module_from_spec(spec)
spec.loader.exec_module(summary)

pairings = {
    'final-rgba8-run1.tsv': 'baseline-rgba8.tsv',
    'final-rgba8-run2.tsv': 'baseline-rgba8.tsv',
    'final-raw.tsv': 'baseline-raw.tsv',
    'final-rgba8-allocations.tsv': 'baseline-rgba8-allocations.tsv',
    'final-raw-allocations.tsv': 'baseline-raw-allocations.tsv',
}
measurements = {
    name: summary.summarize(work / name, work / baseline)
    for name, baseline in pairings.items()
}
_, before = summary.load(work / 'baseline-rgba8.tsv')
runs = [summary.load(work / f'final-rgba8-run{n}.tsv')[1] for n in (1, 2)]
native = 'native-rgba8'
references = ['png-assets-rgba8', 'image-rgba8']
per_file = []
for path in before:
    cycles = {codec: statistics.mean(run[path][codec]['thread_cycles'] for run in runs)
              for codec in [native] + references}
    item = {
        'file': path,
        'pixels': before[path][native]['pixels'],
        'baseline_native_cycles': before[path][native]['thread_cycles'],
        'mean_final_cycles': cycles,
        'native_after_over_before': cycles[native] / before[path][native]['thread_cycles'],
        'native_over_reference': {codec: cycles[native] / cycles[codec] for codec in references},
        'native_slower_in_both_runs': {codec: all(run[path][native]['thread_cycles'] > run[path][codec]['thread_cycles'] for run in runs)
                                      for codec in references},
    }
    per_file.append(item)
result = {'measurements': measurements, 'rgba8_per_file': per_file}
(work / 'final-summary.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')

for name, data in measurements.items():
    print(name)
    for group_name, group in data['groups'].items():
        print(json.dumps({'group': group_name, **group}))
print('Largest absolute baseline regressions:')
for item in sorted(per_file, key=lambda x: x['mean_final_cycles'][native] - x['baseline_native_cycles'], reverse=True)[:12]:
    print(json.dumps(item))
print('Largest remaining relative png gaps:')
for item in sorted(per_file, key=lambda x: x['native_over_reference']['png-assets-rgba8'], reverse=True)[:12]:
    print(json.dumps(item))
