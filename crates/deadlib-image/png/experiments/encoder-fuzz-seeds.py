"""Deterministic, narrow encoder seeds; no production encoder builds them."""
from pathlib import Path
import hashlib
import json
import zipfile

root = next(p for p in Path(__file__).resolve().parents if (p / 'crates/deadlib-image/png/Cargo.toml').is_file())
package = root / 'crates/deadlib-image/png'
archive = root / 'target/png-encoder-20261028-seeds.zip'
assert not archive.exists()
seeds = {}
flags = [0, 1, 2, 3, 4, 5, 6, 7, 12, 13, 14, 15, 16, 17, 18, 19, 32, 33, 34, 35, 64, 65, 96, 97, 127]
for color in range(15):
    for flag in flags:
        for filtering in range(6):
            seeds[f'encode_stream/format-{color:02d}-flags-{flag:03d}-filter-{filtering}'] = bytes([color, 17, 13, flag, filtering, 0, 7, 127, 255, 1, 42, 128])
for size in [0, 1, 2, 7, 31, 257, 32767, 32768, 32769, 65535, 65536, 131072]:
    for pattern in range(4):
        state = 0x71394a55
        data = bytearray()
        for n in range(size):
            state = (1664525 * state + 1013904223) & 0xffffffff
            data.append([0, state >> 24, (n % 7 if n // 32768 % 2 == 0 else (n * 71 + n // 13) & 255), ((n % 32768) * 71 + (n % 32768) // 31) & 255][pattern])
        for mode in range(2):
            for fragment in [0, 1, 4]:
                for output in [0, 3]:
                    for separate in range(2):
                        raw = data.copy()
                        raw[:min(6, size)] = bytes([mode, fragment, output, separate, separate, 30][:min(6, size)])
                        seeds[f'deflate_stream/size-{size:06d}-pattern-{pattern}-mode-{mode}-in-{fragment}-out-{output}-finish-{separate}'] = bytes(raw)
with zipfile.ZipFile(archive, 'x') as out:
    for name, data in sorted(seeds.items()):
        entry = zipfile.ZipInfo(name, (2026, 10, 28, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_DEFLATED
        out.writestr(entry, data)
report = {'archive': str(archive), 'archive_sha256': hashlib.sha256(archive.read_bytes()).hexdigest(),
          'producer_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), 'targets': {}}
for target in ['encode_stream', 'deflate_stream']:
    subset = {n.split('/')[1]: d for n, d in seeds.items() if n.startswith(target + '/')}
    digest = hashlib.sha256()
    for name, data in sorted(subset.items()):
        digest.update(name.encode() + b'\0' + data)
    report['targets'][target] = {'files': len(subset), 'bytes': sum(map(len, subset.values())),
                                 'max_input': max(map(len, subset.values())), 'sha256': digest.hexdigest()}
(package / 'fuzz/reports/encoder-seeds.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8', newline='\n')
print(json.dumps(report, indent=2))
