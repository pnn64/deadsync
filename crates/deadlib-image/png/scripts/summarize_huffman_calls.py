"""Reproduce the symbol-decoder experiment's aggregate rates and invariants."""
from collections import defaultdict
import csv
import hashlib
import json
from pathlib import Path
from statistics import median

PACKAGE = Path(__file__).resolve().parent.parent
STEM = 'benchmark-huffman-call-'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def summarize():
    groups, invariants, artifacts, measurements = {}, {}, [], {}
    for manifest_path in sorted(PACKAGE.glob(STEM + '*-manifest.json')):
        manifest = json.loads(manifest_path.read_text(encoding='utf-8-sig'))
        group = manifest_path.name[len(STEM):-len('-manifest.json')]
        cases = defaultdict(list)
        paths = [manifest_path]
        for run in range(1, manifest['runs'] + 1):
            path = PACKAGE / f'{STEM}{group}-run{run}.tsv'
            paths.append(path)
            with path.open(encoding='utf-8-sig', newline='') as stream:
                for row in csv.DictReader(stream, delimiter='\t'):
                    assert int(row['iterations']) == manifest['iterations']
                    key = (row['codec'], row['operation'], row['case'])
                    cases[key].append(row)
        current = {}
        operations = defaultdict(list)
        for (codec, operation, name), values in cases.items():
            assert len(values) == manifest['runs'], (group, codec, operation, name)
            sizes = {(v['encoded_bytes'], v['sample_bytes'], v['image_pipeline_bytes'])
                     for v in values}
            assert len(sizes) == 1
            current[(codec, operation, name)] = sizes.pop()
            operations[(codec, operation)].append((int(values[0]['sample_bytes']),
                                                   median(int(v['ns_per_image']) for v in values)))
        assert current
        measurements[group] = cases
        target = manifest['target']
        if target in invariants:
            assert current == invariants[target], ('size/case mismatch', group)
        else:
            invariants[target] = current
        assert manifest['cpu'] == 0 and manifest['affinity'] == 1
        assert manifest['warmups_per_operation'] == 3
        groups[group] = [dict(codec=codec, operation=operation,
                              mib_per_second=round(sum(v[0] for v in values) * 1e9 /
                                                   sum(v[1] for v in values) / 1048576, 6),
                              cases=len(values), runs=manifest['runs'],
                              iterations=manifest['iterations'])
                         for (codec, operation), values in operations.items()]
        artifacts.extend(dict(path=p.relative_to(PACKAGE).as_posix(), sha256=digest(p))
                         for p in paths)

    def rate(group, codec, operation):
        return next(v['mib_per_second'] for v in groups[group]
                    if v['codec'] == codec and v['operation'] == operation)

    interleaved = {}
    for target in ['inflate', 'corpus', 'pipeline']:
        for variant, phases in [('before', ['before1', 'before2']),
                                ('split', ['split1', 'split2'])]:
            names = [f'interleave-{phase}-{target}' for phase in phases]
            if not all(name in measurements for name in names):
                continue
            combined = defaultdict(list)
            for name in names:
                for key, rows in measurements[name].items():
                    combined[key].extend(rows)
            operations = defaultdict(list)
            for (codec, operation, _), rows in combined.items():
                operations[(codec, operation)].append((int(rows[0]['sample_bytes']),
                                                       median(int(v['ns_per_image']) for v in rows)))
            interleaved[f'{variant}-{target}'] = [
                dict(codec=codec, operation=operation,
                     mib_per_second=round(sum(v[0] for v in values) * 1e9 /
                                          sum(v[1] for v in values) / 1048576, 6),
                     cases=len(values), source_groups=names)
                for (codec, operation), values in operations.items()]

    comparisons = []
    for trial in ['split-trial', 'narrow-trial', 'combined-trial']:
        for target, reference, reference_op, operations in [
            ('inflate', 'flate2-1.1.10', 'inflate', ['inflate', 'inflate-stream']),
            ('corpus', 'png-0.18.1', 'decode', ['decode', 'decode-reader']),
            ('pipeline', 'png-0.18.1', 'decode', ['decode', 'decode-reader', 'decode-rows'])]:
            selected = f'{trial}-{target}'
            if selected not in groups:
                continue
            for baseline in ['before', 'control']:
                base = f'{baseline}-{target}'
                for operation in operations:
                    observed = rate(selected, 'deadlib-image-png', operation) / rate(base, 'deadlib-image-png', operation)
                    relative = observed * rate(base, reference, reference_op) / rate(selected, reference, reference_op)
                    comparisons.append(dict(trial=trial, target=target, baseline=baseline,
                                            operation=operation, observed_ratio=round(observed, 6),
                                            reference_relative_ratio=round(relative, 6)))
    interleaved_comparisons = []
    for target, reference, reference_op, operations in [
        ('inflate', 'flate2-1.1.10', 'inflate', ['inflate', 'inflate-stream']),
        ('corpus', 'png-0.18.1', 'decode', ['decode', 'decode-reader']),
        ('pipeline', 'png-0.18.1', 'decode', ['decode', 'decode-reader', 'decode-rows'])]:
        before, selected = f'before-{target}', f'split-{target}'
        if not all(name in interleaved for name in [before, selected]):
            continue

        def interleaved_rate(group, codec, operation):
            return next(v['mib_per_second'] for v in interleaved[group]
                        if v['codec'] == codec and v['operation'] == operation)

        for operation in operations:
            observed = interleaved_rate(selected, 'deadlib-image-png', operation) / interleaved_rate(before, 'deadlib-image-png', operation)
            relative = observed * interleaved_rate(before, reference, reference_op) / interleaved_rate(selected, reference, reference_op)
            interleaved_comparisons.append(dict(target=target, operation=operation,
                                               observed_ratio=round(observed, 6),
                                               reference_relative_ratio=round(relative, 6)))
    output = dict(
        method='Sum sample bytes divided by sum of per-case medians of run-average nanoseconds. CPU 0, three warmups per operation. Reference-relative ratios are an inference on a shared host, not a controlled causal estimate.',
        groups=groups, interleaved=interleaved, comparisons=comparisons,
        interleaved_comparisons=interleaved_comparisons, artifact_sha256=artifacts,
        analysis_sha256=digest(Path(__file__)))
    (PACKAGE / 'benchmark-huffman-call-summary.json').write_bytes(
        (json.dumps(output, indent=2) + '\n').encode('utf-8'))
    for group, values in groups.items():
        print(group, {(v['codec'], v['operation']): round(v['mib_per_second'], 2) for v in values})
    for group, values in interleaved.items():
        print('Interleaved', group,
              {(v['codec'], v['operation']): round(v['mib_per_second'], 2) for v in values})


if __name__ == '__main__':
    summarize()
