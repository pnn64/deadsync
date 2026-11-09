"""Archive the verified setter boundary before wiring the existing snapshots."""
import base64,hashlib,json
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2]
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=json.loads((d/'song-actor-setters-reviewed-sources.json').read_text())
for name,digest in before.items():assert sha(r/name)==digest,name
audit=json.loads((d/'song-actor-setters-final-state.json').read_text())
for name,digest in audit['documents'].items():assert sha(r/name)==digest,name
names=['Cargo.toml','src/rust_engine.rs','src/rust_engine/actors.rs',
       'src/rust_engine/input.rs','src/rust_engine/live_scene.rs',
       'src/rust_engine/actors/effects.rs','src/rust_engine/actors/lifecycle.rs']
saved={}
for name in names:
    p=r/'crates/deadsync-song-lua'/name
    saved[p.relative_to(r).as_posix()]={'sha256':sha(p),'base64':base64.b64encode(p.read_bytes()).decode()}
for name,data in [('actor-input-snapshots-before-sources.json',before),
                  ('actor-input-snapshots-edit-baseline.json',saved)]:
    p=d/name;assert not p.exists(),name;p.write_text(json.dumps(data,indent=2)+'\n')
print(json.dumps({'previous_goal_turn':'progress: setter bindings and focused checks',
                  'sources_verified':len(before),'archived_boundaries':len(saved)}))
