"""Fetch pinned Layer III fixtures and PCM; verify length and SHA-256 before use."""
import argparse, hashlib, json, urllib.request
from pathlib import Path

p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True)
args=p.parse_args();args.output.mkdir(parents=True,exist_ok=False)
manifest=Path(__file__).with_name('data')/'iso-vectors.json'
records=json.loads(manifest.read_text())
for record in records['files']:
    data=urllib.request.urlopen(record['url'],timeout=60).read()
    assert len(data)==record['bytes'] and hashlib.sha256(data).hexdigest()==record['sha256'],record['name']
    (args.output/record['name']).write_bytes(data)
(args.output/'provenance.json').write_bytes(manifest.read_bytes())
print('Verified',len(records['files']),'pinned files at',records['revision'])
