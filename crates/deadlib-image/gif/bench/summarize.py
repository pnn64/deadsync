"""Validate complete GIF benchmark tables and summarize every matched input."""
import argparse
import csv
import io
import json
import math
from pathlib import Path

MODES = {
    'native-retained', 'gif-retained', 'native-stream', 'gif-stream',
    'native-rgba', 'gif-rgba', 'image-rgba',
}
PAIRS = [
    ('native-retained', 'gif-retained'), ('native-stream', 'gif-stream'),
    ('native-rgba', 'gif-rgba'), ('native-rgba', 'image-rgba'),
]


def read(path):
    raw = path.read_bytes()
    text = raw.decode('utf-16' if raw[:2] in (b'\xff\xfe', b'\xfe\xff') else 'utf-8-sig')
    files = {}
    for row in csv.DictReader(io.StringIO(text), delimiter='\t'):
        file = row['file'].replace('\\', '/')
        mode = row['codec']
        assert mode in MODES and mode not in files.get(file, {}), (file, mode)
        files.setdefault(file, {})[mode] = {
            key: int(row[key]) for key in ('ns', 'thread_cycles', 'alloc_calls', 'requested_bytes')
        }
    assert files
    for file, modes in files.items():
        assert modes.keys() == MODES, (file, modes.keys())
    return files


def corpus(file):
    if file.startswith('assets/'):
        return 'assets'
    if '/fixtures/' in file:
        return 'upstream'
    return 'generated'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('timings', type=Path)
    parser.add_argument('allocations', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    times, allocations = read(args.timings), read(args.allocations)
    assert times.keys() == allocations.keys(), 'Mismatched measured input sets'
    groups = {'all': list(times)}
    for file in times:
        groups.setdefault(corpus(file), []).append(file)
    summaries = {}
    for group, files in groups.items():
        summary = {'files': len(files), 'comparisons': {}}
        for native, reference in PAIRS:
            metrics = {}
            for metric, table in [('thread_cycles', times), ('alloc_calls', allocations),
                                  ('requested_bytes', allocations)]:
                actual = [table[file][native][metric] for file in files]
                expected = [table[file][reference][metric] for file in files]
                assert all(n > 0 for n in actual + expected), (group, metric)
                ratios = [a / b for a, b in zip(actual, expected)]
                metrics[metric] = {
                    'geomean_ratio': math.exp(sum(map(math.log, ratios)) / len(files)),
                    'native_total': sum(actual), 'reference_total': sum(expected),
                    'native_wins': sum(a < b for a, b in zip(actual, expected)),
                    'ties': sum(a == b for a, b in zip(actual, expected)),
                    'losses': [{'file': file, 'native': a, 'reference': b, 'ratio': a / b}
                               for file, a, b in zip(files, actual, expected) if a > b],
                }
            summary['comparisons'][f'{native}/{reference}'] = metrics
        summaries[group] = summary
    args.output.write_text(json.dumps({'groups': summaries, 'timings': times,
                                     'allocations': allocations}, indent=2) + '\n')
    for group, summary in summaries.items():
        print(group, summary['files'])
        for pair, metrics in summary['comparisons'].items():
            print(' ', pair, ', '.join(f'{metric}={data["geomean_ratio"]:.4f} '
                                      f'({data["native_wins"]} wins, {len(data["losses"])} losses)'
                                      for metric, data in metrics.items()))


if __name__ == '__main__':
    main()
