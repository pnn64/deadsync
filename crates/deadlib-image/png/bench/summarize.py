"""Summarize complete decoder TSVs, keeping corpus and API comparisons separate."""
import argparse
import csv
import hashlib
import io
import json
from pathlib import Path
import re
import statistics


def load(path):
    data = path.read_bytes()
    text = data.decode('utf-16' if data[:2] in (b'\xff\xfe', b'\xfe\xff') else 'utf-8-sig')
    cases = {}
    for row in csv.DictReader(io.StringIO(text), delimiter='\t'):
        case = cases.setdefault(row['file'], {})
        assert row['codec'] not in case, ('duplicate row', row)
        case[row['codec']] = {key: int(row[key]) for key in ('ns', 'thread_cycles', 'alloc_calls', 'requested_bytes')}
        if 'pixels' in row:
            case[row['codec']]['pixels'] = int(row['pixels'])
    assert cases, 'empty measurement'
    codecs = tuple(next(iter(cases.values())))
    allowed = (
        {'native', 'png', 'image', 'png-packed'},
        {'native-rgba8', 'png-assets-rgba8', 'image-rgba8'},
    )
    assert set(codecs) in allowed, ('unexpected codecs', codecs)
    assert all(set(case) == set(codecs) for case in cases.values()), 'incomplete measurement'
    for case in cases.values():
        assert len({row.get('pixels', 0) for row in case.values()}) == 1, 'unequal pixel counts'
    return data, cases


def summarize(path, baseline=None):
    data, cases = load(path)
    codecs = ('native', 'png', 'image', 'png-packed') if 'native' in next(iter(cases.values())) else (
        'native-rgba8', 'png-assets-rgba8', 'image-rgba8')
    native = codecs[0]
    before = None
    if baseline:
        _, before = load(baseline)
        assert set(before) == set(cases), 'baseline corpus differs'
        assert all(set(before[name]) == set(case) for name, case in cases.items()), 'baseline codecs differ'
    groups = {'all': list(cases), 'formats': [], 'photos': [], 'assets': []}
    for name in cases:
        filename = name.replace('\\', '/').rsplit('/', 1)[-1]
        group = 'photos' if re.fullmatch(r'kodim\d+\.png', filename) else 'formats' if filename.startswith('color-') else 'assets'
        groups[group].append(name)
    result = {'input': str(path), 'sha256': hashlib.sha256(data).hexdigest(), 'groups': {}}
    for name, names in groups.items():
        if not names:
            continue
        group = [cases[file] for file in names]
        summary = {'cases': len(group), 'totals': {}, 'native_vs': {}}
        for codec in codecs:
            summary['totals'][codec] = {field: sum(case[codec][field] for case in group) for field in group[0][codec]}
            total = summary['totals'][codec]
            if total.get('pixels', 0) and total['thread_cycles']:
                total['cycles_per_pixel'] = total['thread_cycles'] / total['pixels']
            if total.get('pixels', 0) and total['ns']:
                total['megapixels_per_second'] = total['pixels'] * 1000 / total['ns']
        for codec in codecs[1:]:
            comparison = {}
            for field in ('ns', 'thread_cycles', 'alloc_calls', 'requested_bytes'):
                if all(case[codec][field] > 0 and case[native][field] > 0 for case in group):
                    ratios = [case[native][field] / case[codec][field] for case in group]
                    comparison[field] = {
                        'native_over_reference_geomean': statistics.geometric_mean(ratios),
                        'native_over_reference_total': summary['totals'][native][field] / summary['totals'][codec][field],
                        'native_wins': sum(r < 1 for r in ratios),
                    }
            summary['native_vs'][codec] = comparison
        if before is not None:
            change = {}
            for field in ('ns', 'thread_cycles', 'alloc_calls', 'requested_bytes'):
                if all(before[file][native][field] > 0 and cases[file][native][field] > 0 for file in names):
                    ratios = [cases[file][native][field] / before[file][native][field] for file in names]
                    previous = sum(before[file][native][field] for file in names)
                    current = summary['totals'][native][field]
                    change[field] = {
                        'before_total': previous,
                        'after_total': current,
                        'after_over_before_total': current / previous,
                        'after_over_before_geomean': statistics.geometric_mean(ratios),
                        'improved_cases': sum(r < 1 for r in ratios),
                    }
            summary['native_change'] = change
        result['groups'][name] = summary
    if baseline:
        result['baseline'] = str(baseline)
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('files', type=Path, nargs='+')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--baseline', type=Path, help='same corpus/mode before a production change')
    args = parser.parse_args()
    result = json.dumps([summarize(path, args.baseline) for path in args.files], indent=2) + '\n'
    if args.output:
        args.output.write_text(result, encoding='utf-8', newline='\n')
    else:
        print(result, end='')
