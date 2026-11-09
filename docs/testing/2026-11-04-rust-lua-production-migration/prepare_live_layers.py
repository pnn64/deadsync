"""Save the live-layer pre-edit sources and verify the previous goal progress."""
import base64,hashlib,json,subprocess
from pathlib import Path
r=Path(__file__).resolve().parents[3];d=Path(__file__).parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
files=['Cargo.toml','src/rust_engine.rs','src/rust_engine/actors.rs','src/rust_engine/actors/initial_state.rs','src/rust_engine/actors/song_activation.rs']
saved={}
for n in files:
 p=r/'crates/deadsync-song-lua'/n;saved[p.relative_to(r).as_posix()]={'sha256':sha(p),'base64':base64.b64encode(p.read_bytes()).decode()}
p=d/'song-live-layers-edit-baseline.json';assert not p.exists();p.write_text(json.dumps(saved,indent=2)+'\n')
previous=json.loads((d/'song-live-actors-reviewed-sources.json').read_text())
for p,h in previous.items():assert sha(r/p)==h,p
(d/'song-live-layers-before-sources.json').write_text(json.dumps(previous,indent=2)+'\n')
assert json.loads((d/'song-live-actors-review.json').read_text())['tests_passed']==204
print(json.dumps({'previous_goal_turn':'progress: live traversal and native player feedback; 204 tests verified','sources':len(previous),'saved':list(saved)}))
