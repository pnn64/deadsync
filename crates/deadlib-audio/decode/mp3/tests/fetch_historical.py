"""Fetch hash-pinned historical music and rejection fixtures into scratch storage."""
import argparse, hashlib, json, urllib.request
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument('--output', type=Path, required=True)
p.add_argument('--music-only', action='store_true')
args = p.parse_args(); args.output.mkdir(parents=True, exist_ok=False)
manifest = json.loads((Path(__file__).parent/'data/historical.json').read_text())
for entry in manifest['files']:
    if args.music_only and entry['role'] != 'valid_music':
        continue
    request = urllib.request.Request(entry['url'], headers={'User-Agent': 'deadlib MP3 compatibility validation'})
    with urllib.request.urlopen(request, timeout=90) as response:
        data = response.read()
    assert len(data) == entry['bytes'] and hashlib.sha256(data).hexdigest() == entry['sha256'], entry['file']
    (args.output/entry['file']).write_bytes(data)
    print(entry['file'], len(data), entry['role'], flush=True)
(args.output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
