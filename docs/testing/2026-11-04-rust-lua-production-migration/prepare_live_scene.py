"""Preserve the verified live-layer state before coordinator assembly."""
import base64,hashlib,json
from pathlib import Path
r=Path(__file__).resolve().parents[3];d=Path(__file__).parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=json.loads((d/'song-live-layers-reviewed-sources.json').read_text())
for p,h in before.items():assert sha(r/p)==h,p
files=['Cargo.toml','src/rust_engine.rs','src/rust_engine/actors.rs','src/rust_engine/live_frames.rs','src/rust_engine/statistics.rs','src/rust_engine/actors/initial_state.rs','src/rust_engine/actors/live_frames.rs','src/rust_engine/actors/generated_players.rs','src/rust_engine/actors/diagnostics.rs','src/rust_engine/actors/messages.rs','src/rust_engine/actors/song_capture.rs']
saved={}
for p in files:
 p=r/'crates/deadsync-song-lua'/p;saved[p.relative_to(r).as_posix()]={'sha256':sha(p),'base64':base64.b64encode(p.read_bytes()).decode()}
p=d/'song-live-scene-edit-baseline.json';assert not p.exists();p.write_text(json.dumps(saved,indent=2)+'\n')
(d/'song-live-scene-before-sources.json').write_text(json.dumps(before,indent=2)+'\n')
print(json.dumps({'previous_goal_turn':'progress: live layer scheduling; 483 tests verified','sources':len(before),'saved':len(saved)}))
