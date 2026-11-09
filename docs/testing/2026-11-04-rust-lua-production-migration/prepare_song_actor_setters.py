"""Verify the live-visual boundary and preserve the actor-setter edit inputs."""
import base64
import hashlib
import json
from pathlib import Path

d = Path(__file__).resolve().parent
r = d.parents[2]
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
before = json.loads((d / 'song-live-visuals-reviewed-sources.json').read_text())
for name, digest in before.items():
    assert sha(r / name) == digest, name
audit = json.loads((d / 'song-live-visuals-final-state.json').read_text())
for name, digest in audit['documents'].items():
    assert sha(r / name) == digest, name
saved = {}
for name in ['Cargo.toml', 'src/rust_engine/actors.rs',
             'src/rust_engine/actors.lua', 'src/rust_engine/actors/retirement.rs']:
    p = r / 'crates/deadsync-song-lua' / name
    saved[p.relative_to(r).as_posix()] = {
        'sha256': sha(p), 'base64': base64.b64encode(p.read_bytes()).decode()}
for name, data in [('song-actor-setters-edit-baseline.json', saved),
                   ('song-actor-setters-before-sources.json', before)]:
    p = d / name
    assert not p.exists(), name
    p.write_text(json.dumps(data, indent=2) + '\n')
print(json.dumps({'previous_goal_turn': 'no progress: scope clarification; no implementation changes',
                  'sources_verified': len(before), 'archived_edit_inputs': len(saved)}))
