"""Verify the completed gameplay snapshot and archive the arrow-phase edit boundary."""
import base64,hashlib,json
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2]
before=json.loads((d/'song-live-gameplay-reviewed-sources.json').read_text())
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
for p,h in before.items():assert sha(r/p)==h,p
names=['Cargo.toml','src/rust_engine.rs','src/rust_engine/host_clocks.rs','src/rust_engine/live_scene.rs','src/rust_engine/test_support.rs']
saved={}
for n in names:
 p=r/'crates/deadsync-song-lua'/n;saved[p.relative_to(r).as_posix()]={'sha256':sha(p),'base64':base64.b64encode(p.read_bytes()).decode()}
p=d/'song-arrows-edit-baseline.json';assert not p.exists();p.write_text(json.dumps(saved,indent=2)+'\n')
(d/'song-arrows-before-sources.json').write_text(json.dumps(before,indent=2)+'\n')
print(json.dumps({'previous_goal_turn':'progress: same-VM gameplay callbacks; 586 regression and 150 live library tests','sources':len(before),'saved':len(saved)}))
