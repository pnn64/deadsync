"""Verify the completed arrows boundary and archive the live-visual edit inputs."""
import base64, hashlib, json
from pathlib import Path

d=Path(__file__).resolve().parent; r=d.parents[2]
before=json.loads((d/'song-arrows-reviewed-sources.json').read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
for p,h in before.items(): assert sha(r/p)==h,p
names=['Cargo.toml','src/rust_engine/actors.rs','src/rust_engine/actors/text.rs',
       'src/rust_engine/actors/sprites.rs','src/rust_engine/live_scene.rs']
saved={}
for name in names:
 p=r/'crates/deadsync-song-lua'/name
 saved[p.relative_to(r).as_posix()]={'sha256':sha(p),'base64':base64.b64encode(p.read_bytes()).decode()}
for name in ['song-live-visuals-edit-baseline.json','song-live-visuals-before-sources.json']:
 assert not (d/name).exists(),name
(d/'song-live-visuals-edit-baseline.json').write_text(json.dumps(saved,indent=2)+'\n')
(d/'song-live-visuals-before-sources.json').write_text(json.dumps(before,indent=2)+'\n')
print(json.dumps({'previous_goal_turn':'progress: ArrowEffects, post-health cache and ownership; 658 tests across 38 targets','sources_verified':len(before),'edit_boundaries_archived':len(saved)}))
