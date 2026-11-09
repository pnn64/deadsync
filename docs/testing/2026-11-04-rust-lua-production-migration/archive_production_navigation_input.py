import base64, hashlib, json
from pathlib import Path
d = Path(__file__).resolve().parent
r = d.parents[2]
name = 'crates/deadsync-song-lua/src/rust_engine/live_scene.rs'
before = json.loads((d / 'production-song-cutover-before-sources.json').read_text())
data = (r / name).read_bytes()
assert hashlib.sha256(data).hexdigest() == before[name]
out = d / 'production-navigation-edit-baseline.json'
assert not out.exists()
out.write_text(json.dumps({name: dict(sha256=before[name], base64=base64.b64encode(data).decode())}, indent=2) + '\n')
