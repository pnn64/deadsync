"""Preserve Sound's original installation before merging scalar bindings."""
import base64, hashlib, json
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2]
name='crates/deadsync-song-lua/src/rust_engine/actors/sound.rs'
data=(r/name).read_bytes();digest=hashlib.sha256(data).hexdigest()
before=json.loads((d/'song-actor-setters-before-sources.json').read_text())
assert digest==before[name]
frozen=json.loads((d/'song-actor-setters-focus-v5-song-linux-debug/sources.json').read_text())
for p,h in frozen.items():assert hashlib.sha256((r/p).read_bytes()).hexdigest()==h,p
out=d/'song-actor-setters-sound-edit-baseline.json';assert not out.exists()
out.write_text(json.dumps({name:{'sha256':digest,'base64':base64.b64encode(data).decode()}},indent=2)+'\n')
print(json.dumps({'sources_verified':len(frozen),'sound_baseline_sha256':digest}))
