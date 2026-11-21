"""Validate complete qualification TSVs and print reproducible report numbers."""
import json
import re
from pathlib import Path
import analyze

HERE = Path(__file__).resolve().parent
read = lambda p: analyze.decode_text(p.read_bytes())
items = json.loads(read(HERE / 'jpeg-corpus-selection.json'))['speed_candidates']
items += json.loads(read(HERE / 'jpeg-corpus-kodak.json'))['inputs']
expected = {i['path'].replace('\\', '/'): dict(group=i['group'], pixels=i['width'] * i['height']) for i in items}
assert len(items) == len(expected) == 105
codecs = ('native', 'zune', 'jpeg-decoder', 'image')


def load(name, required=codecs, timing=True):
    rows, _, issues = analyze.parse_tsv((HERE / (name + '.tsv')).read_bytes(), expected)
    assert not issues, (name, issues)
    assert set(rows) == {(p, c) for p in expected for c in required}, (name, 'missing or extra rows')
    if timing:
        assert all(row['ns'] > 0 and row['process_cycles'] > 0 for row in rows.values()), (name, 'zero timing')
    else:
        assert all(row['ns'] == row['process_cycles'] == 0 and row['alloc_calls'] > 0 for row in rows.values()), (name, 'invalid allocation row')
    return {c: {p: rows[p, c] for p in expected} for c in required}


def total(rows, paths=None):
    paths = list(expected) if paths is None else paths
    return analyze.metrics([rows[p] for p in paths], len(paths))


def ratio(a, b, paths=None):
    return analyze.compare(a, b, list(expected) if paths is None else paths)


def subset(name, paths):
    selected = {p: expected[p] for p in paths}
    rows, _, issues = analyze.parse_tsv((HERE / (name + '.tsv')).read_bytes(), selected)
    assert not issues and set(rows) == {(p, 'native') for p in paths}, (name, 'subset coverage')
    assert all(row['ns'] > 0 and row['process_cycles'] > 0 for row in rows.values()), (name, 'zero timing')
    return {p: rows[p, 'native'] for p in paths}


groups = {g: [p for p in expected if expected[p]['group'] == g] for g in sorted({i['group'] for i in expected.values()})}
baseline = load('baseline-rgba8')
final = [load('final-rgba8-run' + str(n)) for n in (1, 2)]
stages = ('baseline', 'cmyk', 'vertical', 'entropy', 'traversal')
paired = [{s: load(f'paired-stages-{s}-round{n}', ('native',))['native'] for s in stages} for n in (1, 2)]
focus = [
    ('cmyk', 'baseline', 'cmyk', groups['kodak-baseline-cmyk-444-q85']),
    ('vertical', 'cmyk', 'vertical', [p for p in expected if p.endswith('/Starfield.jpg') or p.endswith('/Goodbye_bn.jpg')]),
    ('prefix', 'vertical', 'entropy', groups['kodak-progressive-420-q85']),
    ('traversal', 'entropy', 'traversal', groups['kodak-progressive-420-q85']),
]
assert [len(ps) for _, _, _, ps in focus] == [4, 2, 24, 24]
alloc0, alloc1 = load('baseline-alloc', timing=False), load('final-alloc', timing=False)
for stage in stages[1:]:
    alloc = load(stage + '-alloc', ('native',), timing=False)
    assert alloc['native'] == alloc0['native'], (stage, 'allocation behavior changed')
assert alloc1['native'] == alloc0['native']


def diagnostics(name):
    rows = [line.split('\t') for line in read(HERE / (name + '.log')).splitlines()]
    assert not any(row[0] in ('EXCLUDED', 'UNAVAILABLE') for row in rows)
    records = [row for row in rows if row[0] == 'PIXELS']
    assert len(records) == 315 and all(len(row) == 5 for row in records), (name, 'diagnostic count/shape')
    pixels = {(analyze.normalized(row[1]), row[2]): row[3:] for row in records}
    assert set(pixels) == {(analyze.normalized(p), c) for p in expected for c in codecs[1:]}, (name, 'diagnostic coverage')
    return pixels


assert diagnostics('baseline-rgba8') == diagnostics('final-rgba8-run1') == diagnostics('final-rgba8-run2')
losses = []
for p in expected:
    ratios = [run['native'][p]['process_cycles'] / run['image'][p]['process_cycles'] for run in final]
    if min(ratios) > 1:
        losses.append(dict(file=p, group=expected[p]['group'], native_over_image=ratios))
losses.sort(key=lambda item: -min(item['native_over_image']))
tests = {}
for name, count, suites in (('final-tests', 190, 26), ('final-portable-tests', 190, 26), ('final-i686-tests', 55, 5)):
    results = re.findall(r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored', read(HERE / (name + '.log')), re.MULTILINE)
    assert len(results) == suites and all(status == 'ok' and failed == ignored == '0' for status, _, failed, ignored in results), (name, 'suite failure/count')
    passed = sum(int(passed) for _, passed, _, _ in results)
    assert passed == count, (name, 'test count')
    tests[name] = dict(passed=passed, suites=suites)
report = dict(
    expected_files=105, pixels=sum(i['pixels'] for i in expected.values()),
    baseline={c: total(baseline[c]) for c in codecs},
    final_runs=[dict(codecs={c: total(run[c]) for c in codecs},
                     native_over_reference={c: ratio(run['native'], run[c]) for c in codecs[1:]},
                     groups={g: ratio(run['native'], run['image'], ps) for g, ps in groups.items()},
                     native_faster_files=sum(run['native'][p]['process_cycles'] < run['image'][p]['process_cycles'] for p in expected)) for run in final],
    paired_overall=[ratio(run['traversal'], run['baseline']) for run in paired],
    changes={name: [dict(before=total(run[old], ps), after=total(run[new], ps), comparison=ratio(run[new], run[old], ps)) for run in paired] for name, old, new, ps in focus},
    allocations={c: total(alloc1[c]) for c in codecs},
    native_allocation_per_input_unchanged=True, reference_diagnostics_unchanged=True,
    image_losses_both_runs=losses,
    paired_regressions_both_rounds=[dict(file=p, ratios=[run['traversal'][p]['process_cycles'] / run['baseline'][p]['process_cycles'] for run in paired]) for p in expected if all(run['traversal'][p]['process_cycles'] > 1.05 * run['baseline'][p]['process_cycles'] for run in paired)],
    tests=tests,
)
progressive = groups['kodak-progressive-420-q85']
report['rejected_trials'] = {
    'mcu': [ratio(subset(f'mcu-paired-mcu-round{n}', progressive), subset(f'mcu-paired-entropy-round{n}', progressive), progressive) for n in (1, 2)],
    'left': ratio(load('left-rgba8', ('native',))['native'], load('entropy-rgba8', ('native',))['native'], progressive),
    'packed': [ratio(subset(f'packed-screen-packed-round{n}', progressive), subset(f'packed-screen-traversal-round{n}', progressive), progressive) for n in (1, 2)],
}
print(json.dumps(report, indent=2))
