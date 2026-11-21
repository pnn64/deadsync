"""Inventory JPEG inputs without decoding them or altering source assets.

Header inspection is structural evidence, not an acceptance/accuracy oracle.
Run before performance measurements. Only writes jpeg-corpus-* files beside this script.
"""
from __future__ import annotations

import argparse
import collections
import hashlib
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
OUT = pathlib.Path(__file__).resolve().parent
SOF = {0xC0: 'baseline', 0xC1: 'extended-sequential', 0xC2: 'progressive',
       0xC3: 'lossless', 0xC5: 'differential-sequential', 0xC6: 'differential-progressive',
       0xC7: 'differential-lossless', 0xC9: 'arithmetic-sequential',
       0xCA: 'arithmetic-progressive', 0xCB: 'arithmetic-lossless',
       0xCD: 'differential-arithmetic-sequential', 0xCE: 'differential-arithmetic-progressive',
       0xCF: 'differential-arithmetic-lossless'}

def relative(path):
    return path.relative_to(ROOT).as_posix()

def file_list(directory):
    result = subprocess.run(['rg', '--files', '--hidden', '--no-ignore',
                             '--iglob', '*.jpg', '--iglob', '*.jpeg', str(directory)],
                            capture_output=True, text=True)
    if result.returncode not in (0, 1):
        raise RuntimeError(result.stderr)
    return sorted((pathlib.Path(p) for p in result.stdout.splitlines()), key=relative)

def inspect(path):
    raw = path.read_bytes()
    result = dict(path=relative(path), bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest(),
                  frames=[], scans=0, restarts=0, restart_intervals=[], metadata=[],
                  jfif=False, exif=False, icc_chunks=0, adobe_transform=None,
                  hierarchy=False, eoi=False)
    result['signature'] = raw[:8].hex()
    if not raw.startswith(b'\xff\xd8'):
        result['header_error'] = 'missing SOI; filename extension is not sufficient to identify JPEG'
        result['metadata_bytes'] = 0
        return result
    pos = 0
    while pos < len(raw):
        mark = raw.find(b'\xff', pos)
        if mark < 0:
            break
        pos = mark + 1
        while pos < len(raw) and raw[pos] == 0xFF:
            pos += 1
        if pos >= len(raw):
            break
        code = raw[pos]
        pos += 1
        if code == 0:
            continue
        if code == 0xD9:
            result['eoi'] = True
            result['trailing_bytes'] = len(raw) - pos
            break
        if code == 0xD8 or code == 0x01:
            continue
        if 0xD0 <= code <= 0xD7:
            result['restarts'] += 1
            continue
        if pos + 2 > len(raw):
            result['header_error'] = 'truncated marker length'
            break
        length = int.from_bytes(raw[pos:pos + 2], 'big')
        if length < 2 or pos + length > len(raw):
            result['header_error'] = 'invalid or truncated marker payload'
            break
        data = raw[pos + 2:pos + length]
        pos += length
        if code in SOF and len(data) >= 6:
            components = [dict(id=data[i], h=data[i + 1] >> 4, v=data[i + 1] & 15,
                               quantizer=data[i + 2])
                          for i in range(6, min(len(data), 6 + data[5] * 3) - 2, 3)]
            result['frames'].append(dict(marker=f'{code:02x}', process=SOF[code], precision=data[0],
                                         width=int.from_bytes(data[3:5], 'big'),
                                         height=int.from_bytes(data[1:3], 'big'),
                                         component_count=data[5], components=components))
        elif code == 0xDA:
            result['scans'] += 1
        elif code == 0xDC and len(data) >= 2:
            result['dnl_height'] = int.from_bytes(data[:2], 'big')
        elif code == 0xDD and len(data) >= 2:
            result['restart_intervals'].append(int.from_bytes(data[:2], 'big'))
        elif code == 0xDE:
            result['hierarchy'] = True
        if code == 0xFE or 0xE0 <= code <= 0xEF:
            result['metadata'].append(dict(marker=f'{code:02x}', bytes=len(data),
                                            sha256=hashlib.sha256(data).hexdigest()))
            result['jfif'] |= code == 0xE0 and data.startswith(b'JFIF\0')
            result['exif'] |= code == 0xE1 and data.startswith(b'Exif\0\0')
            result['icc_chunks'] += int(code == 0xE2 and data.startswith(b'ICC_PROFILE\0'))
            if code == 0xEE and data.startswith(b'Adobe') and len(data) >= 12:
                result['adobe_transform'] = data[11]
    result['metadata_bytes'] = sum(m['bytes'] for m in result['metadata'])
    if result['frames']:
        first = result['frames'][0]
        for key in ('width', 'height', 'precision', 'process', 'component_count'):
            result[key] = first[key]
        ids = [c['id'] for c in first['components']]
        count = first['component_count']
        if count == 1:
            color = 'gray'
        elif count == 3:
            color = 'RGB' if result['adobe_transform'] == 0 or ids == [82, 71, 66] else 'YCbCr'
        elif count == 4:
            color = 'YCCK' if result['adobe_transform'] == 2 else 'CMYK'
        else:
            color = f'generic-{count}'
        result['color_hint'] = color
        result['sampling'] = ','.join(f"{c['h']}x{c['v']}" for c in first['components'])
    return result

def summary(items):
    counts = collections.Counter((i.get('process', 'table-only'), i.get('precision', 0),
                                  i.get('color_hint', 'none'), i.get('sampling', 'none')) for i in items)
    return [dict(process=k[0], precision=k[1], color_hint=k[2], sampling=k[3], count=v)
            for k, v in sorted(counts.items())]

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--include-target', action='store_true', help='Count target JPEGs; do not read/hash them.')
    parser.add_argument('--include-runtime', action='store_true', help='Read/hash debug and release runtime JPEGs.')
    args = parser.parse_args()
    directories = [ROOT / 'assets', ROOT / 'crates/deadlib-image/jpeg/tests/fixtures']
    items = [inspect(path) for directory in directories for path in file_list(directory)]
    items.sort(key=lambda item: item['path'])
    manifest = dict(schema=1, baseline_commit='9a6f4ebabe71934202b55df110dae03883ca512c',
                    scope='All shipped assets and all JPEG fixture files, case insensitive extension.',
                    inspection='Structural header inspection only; no decoder executed.',
                    inputs=items, categories=summary(items))
    (OUT / 'jpeg-corpus-inventory.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
    report = dict(files=len(items), bytes=sum(i['bytes'] for i in items),
                  categories=manifest['categories'], assets=[i for i in items if i['path'].startswith('assets/')])
    if args.include_target:
        target = file_list(ROOT / 'target')
        groups = collections.defaultdict(lambda: dict(files=0, bytes=0, examples=[]))
        for path in target:
            group = groups[path.relative_to(ROOT / 'target').parts[0]]
            group['files'] += 1
            group['bytes'] += path.stat().st_size
            if len(group['examples']) < 4:
                group['examples'].append(relative(path))
        report['target_groups'] = dict(sorted(groups.items()))
        (OUT / 'jpeg-corpus-target-groups.json').write_text(json.dumps(report['target_groups'], indent=2) + '\n', encoding='utf-8')
    if args.include_runtime:
        runtime = [inspect(path) for directory in (ROOT / 'target/debug', ROOT / 'target/release')
                   for path in file_list(directory)]
        runtime.sort(key=lambda item: item['path'])
        unique = {item['sha256']: item for item in reversed(runtime)}
        runtime_report = dict(schema=1, scope='Existing debug/release runtime JPEG files; no fixture generation.',
                              inputs=runtime, categories=summary(list(unique.values())),
                              files=len(runtime), unique_files=len(unique), bytes=sum(i['bytes'] for i in runtime))
        (OUT / 'jpeg-corpus-runtime-inventory.json').write_text(json.dumps(runtime_report, indent=2) + '\n', encoding='utf-8')
        report['runtime_summary'] = {k: runtime_report[k] for k in ('files', 'unique_files', 'bytes', 'categories')}
    print(json.dumps(dict(files=report['files'], bytes=report['bytes'], categories=len(report['categories']),
                          assets=[{k: i.get(k) for k in ('path', 'bytes', 'sha256', 'width', 'height', 'process',
                                                       'precision', 'color_hint', 'sampling', 'metadata_bytes', 'exif', 'icc_chunks')}
                                  for i in report['assets']], target_groups=report.get('target_groups', {}),
                          runtime=report.get('runtime_summary', {})), indent=2))

if __name__ == '__main__':
    main()
