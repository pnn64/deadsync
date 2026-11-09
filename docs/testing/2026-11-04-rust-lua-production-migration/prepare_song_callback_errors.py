from pathlib import Path
import base64,hashlib,json
r=Path('/mnt/c/GitHub/deadsync');d=r/'docs/testing/2026-11-04-rust-lua-production-migration';dest=d/'song-callback-errors-edit-baseline.json';assert not dest.exists()
names=['src/rust_engine.rs','src/rust_engine/actors.rs','src/rust_engine/compiler.rs','src/rust_engine/actors/compiler.rs','src/rust_engine/actors/song_activation.rs','src/rust_engine/actors/song_multitap.rs','src/rust_engine/actors/lifecycle.rs','src/rust_engine/actors/recurring.rs','src/rust_engine/actors/capture.rs','src/rust_engine/actors/draw.rs','src/rust_engine/test_support.rs','Cargo.toml']
rows={}
for name in names:
 name='crates/deadsync-song-lua/'+name;raw=(r/name).read_bytes();rows[name]={'sha256':hashlib.sha256(raw).hexdigest(),'base64':base64.b64encode(raw).decode()}
dest.write_text(json.dumps(rows,indent=2)+'\n');(d/'song-callback-errors-before-sources.json').write_bytes((d/'song-compiler-facade-final-sources.json').read_bytes());print('Preserved',len(rows),'files and the previous 1268-source snapshot')