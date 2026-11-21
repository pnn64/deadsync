"""Freeze a dimension-stratified JPEG benchmark candidate set before timing.

This script reads inventory JSON only. It does not decode or time files.
Structural selection is not proof that image or native will accept an input.
"""
import collections
import json
import pathlib

OUT = pathlib.Path(__file__).resolve().parent
FIXTURES = 'crates/deadlib-image/jpeg/tests/fixtures/'
shipped = json.loads((OUT / 'jpeg-corpus-inventory.json').read_text(encoding='utf-8'))['inputs']
runtime = json.loads((OUT / 'jpeg-corpus-runtime-inventory.json').read_text(encoding='utf-8'))['inputs']
assets = [item for item in shipped if item['path'].startswith('assets/')]
asset_hashes = {item['sha256'] for item in assets}
unique = {item['sha256']: item for item in reversed(runtime)}
groups = collections.defaultdict(list)
omitted = []
for item in unique.values():
    if item['sha256'] in asset_hashes:
        continue
    if item.get('header_error') or item.get('precision') != 8 or item.get('process') not in ('baseline', 'progressive') or item.get('component_count') not in (1, 3):
        omitted.append(dict(path=item['path'], sha256=item['sha256'], reason=item.get('header_error', 'not an ordinary 8-bit gray/RGB JPEG')))
        continue
    groups[(item['process'], item['color_hint'], item['sampling'])].append(item)

selected = []
for key, values in sorted(groups.items()):
    values.sort(key=lambda item: (item['width'] * item['height'], item['bytes'], item['sha256'], item['path']))
    indexes = [0, (len(values) - 1) // 2, len(values) - 1] if len(values) >= 10 else [0, len(values) - 1]
    for rank in sorted(set(indexes)):
        item = dict(values[rank])
        item['group'] = 'runtime-stratified'
        item['selection'] = dict(stratum=list(key), rank=rank, population=len(values),
                                 order='width*height, encoded bytes, SHA256, path')
        selected.append(item)

for item in assets:
    selected.append(dict(item, group='shipped-assets', selection='All five shipped JPEG assets'))

diagnostic_names = {'8-410.jpg', '8-411.jpg', '8-multiscan.jpg'}
diagnostic = []
native_only = []
for item in shipped:
    path = pathlib.PurePosixPath(item['path'])
    if item['path'].startswith(FIXTURES + 'sequential/') or item['path'].startswith(FIXTURES + 'progressive/'):
        if item.get('precision') == 8:
            if path.name in diagnostic_names:
                reason = ('Known image/zune sequential multi-scan sample mismatch' if path.parent.name == 'sequential' and path.name == '8-multiscan.jpg'
                          else 'Legacy interpolation/refinement diagnostic kept separate from main speed aggregate')
                diagnostic.append(dict(item, group='legacy-diagnostic', reason=reason))
            else:
                selected.append(dict(item, group='small-generated-' + path.parent.name,
                                     selection='All original 8-bit fixtures except predeclared diagnostic names'))
            continue
    if item['path'] == FIXTURES + 'metadata/ycck-8.jpg':
        selected.append(dict(item, group='small-generated-metadata', selection='Adobe transform 2 / YCCK coverage'))
        continue
    if not item['path'].startswith('assets/'):
        native_only.append(dict(item, group='native-correctness', reason='Broader fixture/acceptance oracle; not a reference speed claim'))

selected.sort(key=lambda item: item['path'])
diagnostic.sort(key=lambda item: item['path'])
native_only.sort(key=lambda item: item['path'])
payload = dict(schema=1, baseline_commit='9a6f4ebabe71934202b55df110dae03883ca512c',
               status='Fixed before timing; reference/native acceptance still requires untimed preflight.',
               policy=dict(runtime='Content-deduplicate; exclude shipped copies; stratify by process/color/sampling; take minimum, median and maximum pixel count when population >=10, otherwise extrema.',
                           fixtures='All 8-bit sequential/progressive fixtures except 410/411/multiscan diagnostic names, plus 8-bit YCCK metadata.',
                           exclusions='No timing-based filtering. Non-JPEG signatures and structurally nonordinary files are listed separately.',
                           comparison='Cross-decoder pixels are diagnostic. Native before/after exact output is the regression oracle.'),
               speed_candidates=selected, diagnostic=diagnostic, native_correctness=native_only,
               runtime_structural_exclusions=omitted)
(OUT / 'jpeg-corpus-selection.json').write_text(json.dumps(payload, indent=2) + '\n', encoding='utf-8')
for name, items in [('speed', selected), ('diagnostic', diagnostic), ('native-correctness', native_only)]:
    (OUT / ('jpeg-corpus-' + name + '.txt')).write_text(''.join(item['path'] + '\n' for item in items), encoding='utf-8')

print(json.dumps(dict(speed=len(selected), diagnostics=len(diagnostic), native_correctness=len(native_only),
                      groups=dict(collections.Counter(item['group'] for item in selected)),
                      runtime_exclusions=omitted), indent=2))
print('group\twidth\theight\tprocess\tcolor\tsampling\tmetadata_bytes\tpath')
for item in selected:
    print('\t'.join(str(item.get(k, '')) for k in ('group', 'width', 'height', 'process', 'color_hint', 'sampling', 'metadata_bytes', 'path')))
