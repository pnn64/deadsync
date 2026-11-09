"""Freeze the verified source and preserve every prospective routing edit."""
import base64
import hashlib
import json
from pathlib import Path
d = Path(__file__).resolve().parent
r = d.parents[2]
out = d / 'production-song-cutover-edit-baseline.json'
manifest = d / 'production-song-cutover-before-sources.json'
assert not out.exists() and not manifest.exists()
before = json.loads((d / 'actor-input-snapshots-reviewed-sources.json').read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
assert all(sha(r / name) == digest for name, digest in before.items())
assert sha(d / 'objective.txt') == '86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
song = r / 'crates/deadsync-song-lua'
paths = [song / name for name in ['Cargo.toml', 'src/lib.rs', 'src/compile.rs',
    'src/live.rs', 'src/rust_engine.rs', 'src/rust_engine/test_support.rs']]
paths.extend(sorted((song / 'tests').glob('rust_*.rs')))
saved = {p.relative_to(r).as_posix(): dict(sha256=sha(p),
         base64=base64.b64encode(p.read_bytes()).decode()) for p in paths}
out.write_text(json.dumps(saved, indent=2) + '\n')
manifest.write_text(json.dumps(before, indent=2) + '\n')
print(f'Verified {len(before)} current sources; archived {len(saved)} prospective edit inputs.', flush=True)
