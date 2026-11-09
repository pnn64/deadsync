"""Preserve the verified coordinator snapshot before live compiler integration."""
import base64,hashlib,json
from pathlib import Path
d=Path(__file__).parent;r=d.resolve().parents[2]
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=json.loads((d/'song-live-scene-reviewed-sources.json').read_text())
for p,h in before.items():assert sha(r/p)==h,p
files=['Cargo.toml','src/lib.rs','src/compile.rs','src/rust_engine.rs','src/rust_engine/compiler.rs','src/rust_engine/live_scene.rs','src/rust_engine/actors/live_scene.rs','src/rust_engine/actors/compiler.rs','src/rust_engine/observations.rs']
saved={}
for p in files:
 p=r/'crates/deadsync-song-lua'/p;saved[p.relative_to(r).as_posix()]={'sha256':sha(p),'base64':base64.b64encode(p.read_bytes()).decode()}
p=d/'song-live-compiler-edit-baseline.json';assert not p.exists();p.write_text(json.dumps(saved,indent=2)+'\n')
(d/'song-live-compiler-before-sources.json').write_text(json.dumps(before,indent=2)+'\n')
print(json.dumps({'previous_goal_turn':'progress: retained Rust live-scene coordinator; 561 tests verified','sources':len(before),'saved':len(saved)}))
