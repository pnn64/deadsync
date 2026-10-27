"""Reproduce filter experiment rates, size invariants and reference-relative ratios."""
from collections import defaultdict
import csv
import hashlib
import json
from pathlib import Path
from statistics import median

PACKAGE = Path(__file__).resolve().parent.parent
STEM = 'benchmark-filter-'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def rates(cases):
    operations = defaultdict(list)
    for (codec, operation, _), values in cases.items():
        operations[(codec, operation)].append((int(values[0]['sample_bytes']),
                                               median(int(v['ns_per_image']) for v in values)))
    return [dict(codec=codec, operation=operation, cases=len(values),
                 mib_per_second=round(sum(v[0] for v in values) * 1e9 / sum(v[1] for v in values) / 1048576, 6))
            for (codec, operation), values in operations.items()]


def partitions(cases, target):
    result = {}
    if target == 'filter':
        assert len(cases) == 240
        for filter_type in range(5):
            result[f'filter{filter_type}'] = rates({k: v for k, v in cases.items() if k[2].endswith(f'filter{filter_type}')})
        for color in ['gray8', 'gray16', 'ga8', 'ga16', 'rgb8', 'rgb16', 'rgba8', 'rgba16']:
            for label, filter_type in [('paeth', 4), ('average', 3)]:
                result[f'{label}-{color}'] = rates({k: v for k, v in cases.items() if k[2].endswith(f'{color}-filter{filter_type}')})
    return result


def compare(before, selected):
    result = []
    for value in selected:
        codec, operation = value['codec'], value['operation']
        if codec != 'deadlib-image-png':
            continue
        reference = 'flate2-1.1.10' if operation.startswith('inflate') else 'png-0.18.1'
        reference_op = 'inflate' if operation.startswith('inflate') else 'encode' if operation.startswith('encode') else 'decode'

        def rate(values, c, op):
            return next(v['mib_per_second'] for v in values if v['codec'] == c and v['operation'] == op)

        observed = value['mib_per_second'] / rate(before, codec, operation)
        relative = observed * rate(before, reference, reference_op) / rate(selected, reference, reference_op)
        result.append(dict(operation=operation, observed_ratio=round(observed, 6),
                           reference_relative_ratio=round(relative, 6)))
    return result


def main():
    groups, artifacts, invariants, measurements = {}, [], {}, {}
    for path in sorted(PACKAGE.glob(STEM + '*-manifest.json')):
        manifest = json.loads(path.read_text(encoding='utf-8-sig'))
        name = path.name[len(STEM):-len('-manifest.json')]
        cases = defaultdict(list)
        paths = [path]
        for run in range(1, manifest['runs'] + 1):
            table = PACKAGE / f'{STEM}{name}-run{run}.tsv'
            paths.append(table)
            with table.open(encoding='utf-8-sig', newline='') as stream:
                for row in csv.DictReader(stream, delimiter='\t'):
                    assert int(row['iterations']) == manifest['iterations']
                    cases[(row['codec'], row['operation'], row['case'])].append(row)
        current = {}
        for key, values in cases.items():
            assert len(values) == manifest['runs']
            sizes = {(v['encoded_bytes'], v['sample_bytes'], v['image_pipeline_bytes']) for v in values}
            assert len(sizes) == 1
            current[key] = sizes.pop()
        assert current
        measurements[name] = cases
        target = manifest['target']
        if target in invariants:
            assert current == invariants[target], ('case/size/capacity mismatch', name)
        else:
            invariants[target] = current
        assert manifest['cpu'] == 0 and manifest['affinity'] == 1
        groups[name] = dict(target=target, runs=manifest['runs'], iterations=manifest['iterations'],
                            aggregate=rates(cases), partitions=partitions(cases, target))
        artifacts.extend(dict(path=p.relative_to(PACKAGE).as_posix(), sha256=digest(p)) for p in paths)
    interleaved, comparisons = {}, {}
    for target in ['filter', 'corpus', 'pipeline']:
        for variant, phases in [('before', ['before1', 'before2']), ('final', ['final1', 'final2'])]:
            names = [f'interleave-{phase}-{target}' for phase in phases]
            if not all(name in measurements for name in names):
                continue
            combined = defaultdict(list)
            for name in names:
                for key, rows in measurements[name].items():
                    combined[key].extend(rows)
            interleaved[f'{variant}-{target}'] = dict(source_groups=names, aggregate=rates(combined),
                                                      partitions=partitions(combined, target))
        before, selected = f'before-{target}', f'final-{target}'
        if before in interleaved and selected in interleaved:
            comparisons[target] = dict(aggregate=compare(interleaved[before]['aggregate'], interleaved[selected]['aggregate']),
                                       partitions={key: compare(values, interleaved[selected]['partitions'][key])
                                                   for key, values in interleaved[before]['partitions'].items()})
    selected_interleaved, selected_comparisons = {}, {}
    for target in ['filter', 'corpus', 'pipeline']:
        for variant, phases in [('before', ['before1', 'before2']), ('selected', ['selected1', 'selected2'])]:
            names = [f'selected-interleave-{phase}-{target}' for phase in phases]
            if not all(name in measurements for name in names):
                continue
            combined = defaultdict(list)
            for name in names:
                for key, rows in measurements[name].items():
                    combined[key].extend(rows)
            selected_interleaved[f'{variant}-{target}'] = dict(source_groups=names, aggregate=rates(combined),
                                                               partitions=partitions(combined, target))
        before, selected = f'before-{target}', f'selected-{target}'
        if before in selected_interleaved and selected in selected_interleaved:
            selected_comparisons[target] = dict(aggregate=compare(selected_interleaved[before]['aggregate'], selected_interleaved[selected]['aggregate']),
                                                partitions={key: compare(values, selected_interleaved[selected]['partitions'][key])
                                                            for key, values in selected_interleaved[before]['partitions'].items()})
    output = dict(method='Total sample bytes divided by sum of per-case medians of run-average nanoseconds; CPU0; three warmups per operation; shared host and frequency uncontrolled. Rust png filter benchmark retains its native wire-byte output, while native PNG retains U8/U16 samples.',
                  groups=groups, interleaved=interleaved, interleaved_comparisons=comparisons,
                  selected_variant='hybrid', selected_interleaved=interleaved,
                  selected_comparisons=comparisons,
                  split_encoder_interleaved=selected_interleaved, split_encoder_comparisons=selected_comparisons,
                  artifact_sha256=artifacts, analysis_sha256=digest(Path(__file__)))
    (PACKAGE / 'benchmark-filter-summary.json').write_bytes((json.dumps(output, indent=2) + '\n').encode())
    for name, group in groups.items():
        print(name, {(v['codec'], v['operation']): round(v['mib_per_second'], 2) for v in group['aggregate']})
        if group['target'] == 'filter':
            for part in ['filter3', 'filter4']:
                print(part, {(v['codec'], v['operation']): round(v['mib_per_second'], 2) for v in group['partitions'][part]})
    for name, group in interleaved.items():
        print('Selected interleaved', name, {(v['codec'], v['operation']): round(v['mib_per_second'], 2) for v in group['aggregate']})
    for name, group in selected_interleaved.items():
        print('Split-encoder interleaved', name, {(v['codec'], v['operation']): round(v['mib_per_second'], 2) for v in group['aggregate']})


if __name__ == '__main__':
    main()
