import base64, hashlib, json
from pathlib import Path
d = Path(__file__).resolve().parent
r = d.parents[2]
name = 'crates/deadsync-song-lua/src/runtime_fallback.rs'
data = (r / name).read_bytes()
before = json.loads((d / 'production-song-routing-contract-v1-song-windows-debug/sources.json').read_text())
assert hashlib.sha256(data).hexdigest() == before[name]
out = d / 'production-fallback-edit-baseline.json'
assert not out.exists()
out.write_text(json.dumps({name: dict(sha256=before[name], base64=base64.b64encode(data).decode())}, indent=2) + '\n')
