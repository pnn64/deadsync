"""Summarize complete decoder TSVs, keeping corpus and API comparisons separate."""
import argparse
import csv
import hashlib
import io
import json
from pathlib import Path
import re
import statistics


def summarize(path):
    data = path.read_bytes()
    text = data.decode('utf-16' if data[:2] in (b'\xff\xfe', b'\xfe\xff') else 'utf-8-sig')
    cases = {}
    for row in csv.DictReader(io.StringIO(text), delimiter='\t'):
        case = cases.setdefault(row['file'], {})
        assert row['codec'] not in case, ('duplicate row', row)
        case[row['codec']] = {key: int(row[key]) for key in ('ns', 'thread_cycles', 'alloc_calls', 'requested_bytes')}
    assert cases, 'empty measurement'
    assert all(set(case) == {'native', 'png', 'image', 'png-packed'} for case in cases.values()), 'incomplete measurement'
    groups = {'all': list(cases.values()), 'formats': [], 'photos': [], 'assets': []}
    for name, case in cases.items():
        filename = name.replace('\\', '/').rsplit('/', 1)[-1]
        group = 'photos' if re.fullmatch(r'kodim\d+\.png', filename) else 'formats' if filename.startswith('color-') else 'assets'
        groups[group].append(case)
    result = {'input': str(path), 'sha256': hashlib.sha256(data).hexdigest(), 'groups': {}}
    for name, group in groups.items():
        if not group:
            continue
        summary = {'cases': len(group), 'totals': {}, 'native_vs': {}}
        for codec in ('native', 'png', 'image', 'png-packed'):
            summary['totals'][codec] = {field: sum(case[codec][field] for case in group) for field in group[0][codec]}
        for codec in ('png', 'image', 'png-packed'):
            comparison = {}
            for field in ('thread_cycles', 'alloc_calls', 'requested_bytes'):
                if all(case[codec][field] > 0 and case['native'][field] > 0 for case in group):
                    ratios = [case['native'][field] / case[codec][field] for case in group]
                    comparison[field] = {'native_over_reference_geomean': statistics.geometric_mean(ratios), 'native_wins': sum(r < 1 for r in ratios)}
            summary['native_vs'][codec] = comparison
        result['groups'][name] = summary
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('files', type=Path, nargs='+')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    result = json.dumps([summarize(path) for path in args.files], indent=2) + '\n'
    if args.output:
        args.output.write_text(result, encoding='utf-8', newline='\n')
    else:
        print(result, end='')
