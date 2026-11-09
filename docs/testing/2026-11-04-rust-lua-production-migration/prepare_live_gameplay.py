"""Save the verified compiler source before gameplay callback integration."""
import base64, hashlib, json
from pathlib import Path
d = Path(__file__).resolve().parent
r = d.parents[2]
before = json.loads((d/'song-live-compiler-reviewed-sources.json').read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
for p, h in before.items():
    assert sha(r/p) == h, p
files = ['Cargo.toml', 'src/live.rs', 'src/rust_engine.rs',
         'src/rust_engine/live_scene.rs', 'src/rust_engine/live_frames.rs',
         'src/rust_engine/compiler/live.rs', 'src/rust_engine/actors.rs',
         'src/rust_engine/actors/diagnostics.rs',
         'src/rust_engine/actors/messages.rs',
         'src/rust_engine/actors/messages/judgment.rs',
         'tests/rust_live_scene.rs', 'tests/rust_live_compiler.rs']
saved = {}
for name in files:
    p = r/'crates/deadsync-song-lua'/name
    saved[p.relative_to(r).as_posix()] = {'sha256': sha(p), 'base64': base64.b64encode(p.read_bytes()).decode()}
p = d/'song-live-gameplay-edit-baseline.json'
assert not p.exists()
p.write_text(json.dumps(saved, indent=2)+'\n')
(d/'song-live-gameplay-before-sources.json').write_text(json.dumps(before, indent=2)+'\n')
print(json.dumps({'previous_goal_turn': 'no progress; scope response only; verified active migration objective', 'sources': len(before), 'saved': len(saved)}))
