"""Record unresolved source findings without changing the migration objective."""
import hashlib
import json
from pathlib import Path

d = Path(__file__).resolve().parent
r = d.parents[2]
out = d / 'actor-input-snapshots-review-notes-v1.json'
assert not out.exists()
names = [
    'crates/deadsync-song-lua/src/rust_engine/actors/lifecycle.rs',
    'crates/deadsync-song-lua/src/rust_engine/actors/models.rs',
    'crates/deadsync-song-lua/src/model_texture.rs',
    'crates/deadsync-song-lua/src/live.rs',
    'crates/deadsync-theme-simply-love/src/screens/gameplay.rs',
]
texts = {name: (r / name).read_text() for name in names}
assert '.is_some_and(|actor| !actor.model_materials.is_empty())' in texts[names[0]]
assert 'clocks\n                .iter()\n                .any(Option::is_some)' in texts[names[2]]
assert 'scene.next_screen_name()' in texts[names[4]]
rust_scene = [r / 'crates/deadsync-song-lua/src/rust_engine/live_scene.rs',
              *(r / 'crates/deadsync-song-lua/src/rust_engine/live_scene').glob('*.rs')]
assert all('fn next_screen_name(' not in p.read_text() for p in rust_scene)
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
out.write_text(json.dumps({
    'objective_sha256': sha(d / 'objective.txt'),
    'sources': {name: sha(r / name) for name in names},
    'findings': [
        {'status': 'Pending correction after verification source freeze',
         'location': names[0],
         'issue': 'The new frozen texture flag tests material-vector nonemptiness. Native model_texture::install retains only actors with at least one Some clock. A vector of static None clocks must remain eligible.',
         'required_change': 'Use actor.model_materials.iter().any(Option::is_some), preserving the existing texture-job gate.'},
        {'status': 'Existing migration API gap; not implemented in this phase',
         'location': names[4],
         'issue': 'The production Gameplay transition calls scene.next_screen_name(). The Rust scene currently lacks this API.',
         'required_behavior': 'Read the current top-screen raw __songlua_next_screen_name override with native conversion/error behavior, and return None for an empty override.'},
    ],
    'current_verification': {
        'linux_focus': '54 passed on the preceding 1306-source snapshot.',
        'linux_contract': '43 passed on the 1307-source snapshot; recorder interrupted in a WSL filesystem call. Independent host source proof retained.',
        'windows_contract': 'Running against the same frozen 1307-source snapshot.',
        'final_source_verification_complete': False,
    },
    'scope': 'Interpreter and C API-removal milestones remain completed. The fixed production migration goal remains active and incomplete; these findings implement existing required behavior, not new features.',
}, indent=2) + '\n')
print('Recorded the two source findings and current incomplete validation state.', flush=True)
