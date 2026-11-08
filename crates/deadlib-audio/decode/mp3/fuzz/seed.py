"""Seed raw frames and streaming inputs from the recorded validation corpora."""
from pathlib import Path
import argparse, hashlib, json

p = argparse.ArgumentParser()
p.add_argument('--reference-root', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
args = p.parse_args()
crate = Path(__file__).resolve().parents[1]
scratch = args.reference_root
out = args.output
out.mkdir(parents=True, exist_ok=False)
records = []


def seed(target, data):
    digest = hashlib.sha256(data).hexdigest()
    path = out / 'corpus' / target / digest
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    records.append({'target': target, 'sha256': digest, 'bytes': len(data)})


framing = json.loads((crate / 'tests/validation/2026-10-27-foundation/reference-framing.json').read_text())
for r in framing:
    data = (scratch / 'framing-final' / Path(r['command'][-1]).name).read_bytes()
    assert hashlib.sha256(data).hexdigest() == r['sha256']
    for control in [0, 3, 7]:
        seed('streams', bytes([control]) + data[:32768])
    rows = [list(map(int, line.split('\t')[1:])) for line in r['native']['stdout'].splitlines() if line.startswith('frame\t')]
    packets = []
    for row in rows[:8]:
        packet = data[row[0]:row[0] + row[-1]]
        packets.append(packet)
        for control in [0, 1]:
            seed('frames', bytes([control]) + packet)
    for control in [0, 1]:
        seed('frames', bytes([control]) + b''.join(packets))
for path in (scratch / 'encoder-corpus-verified').glob('*.mp3'):
    for control in [0, 3, 7]:
        seed('streams', bytes([control]) + path.read_bytes()[:32768])
for target in ['frames', 'streams']:
    for data in [b'', b'ID3', b'ID3\x04\x00\x00\x7f\x7f\x7f\x7f', b'\xff\xfb\x90\xc0', bytes(1024)]:
        seed(target, b'\x03' + data)
(out / 'seeds.json').write_text(json.dumps({'files': len(records), 'seeds': records}, indent=2) + '\n')
print('seeds', len(records))
