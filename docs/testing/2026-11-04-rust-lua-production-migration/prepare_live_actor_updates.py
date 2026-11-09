"""Preserve the exact pre-edit live actor update sources and watched inventory."""
import base64, hashlib, json, subprocess
from pathlib import Path
r=Path(__file__).resolve().parents[3]; d=Path(__file__).parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
files=['src/rust_engine.rs','src/rust_engine/actors.rs','src/rust_engine/actors/song_multitap.rs','Cargo.toml']
saved={}
for n in files:
 p=r/'crates/deadsync-song-lua'/n
 saved[p.relative_to(r).as_posix()]={'sha256':sha(p),'base64':base64.b64encode(p.read_bytes()).decode()}
target=d/'song-live-actors-edit-baseline.json'; assert not target.exists()
target.write_text(json.dumps(saved,indent=2)+'\n')
crates=['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']
names=subprocess.check_output(['git','-c',f'safe.directory={r.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in crates]],cwd=r).decode().split('\0')
paths={r/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(r/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
inventory={p.relative_to(r).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
(d/'song-live-actors-before-sources.json').write_text(json.dumps(inventory,indent=2)+'\n')
print(json.dumps({'source_count':len(inventory),'saved':list(saved),'previous_goal_turn':'no progress: scope acknowledgment only','next_action':'port live actor traversal and native feedback from the fixed inventory'}))
