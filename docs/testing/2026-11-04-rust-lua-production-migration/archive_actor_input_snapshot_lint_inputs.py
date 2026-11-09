"""Preserve the three files before doc/attribute-only Clippy cleanup."""
import hashlib
import json
from pathlib import Path
d = Path(__file__).resolve().parent
r = d.parents[2]
names = ['src/rust_engine/actors/input_snapshot.rs',
         'src/rust_engine/actors/input_snapshot/tables.rs',
         'src/rust_engine/actors/lifecycle/snapshot.rs']
out = d / 'actor-input-snapshots-lint-inputs.json'
assert not out.exists()
rows = []
for name in names:
    source = r / 'crates/deadsync-song-lua' / name
    archive = d / ('actor-input-snapshots-pre-lint-' + name.replace('/', '_') + '.txt')
    assert not archive.exists()
    data = source.read_bytes()
    archive.write_bytes(data)
    rows.append(dict(source=source.relative_to(r).as_posix(), archive=archive.name,
                     sha256=hashlib.sha256(data).hexdigest()))
out.write_text(json.dumps(rows, indent=2) + '\n')
print('Archived the three lint-cleanup inputs.', flush=True)
