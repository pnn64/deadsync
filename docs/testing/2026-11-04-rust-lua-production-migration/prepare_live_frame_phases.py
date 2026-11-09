import base64,hashlib,json
from pathlib import Path
r=Path('/mnt/c/GitHub/deadsync');d=Path(__file__).parent
files=['crates/deadsync-song-lua/src/rust_engine.rs','crates/deadsync-song-lua/src/rust_engine/actors.rs','crates/deadsync-song-lua/src/rust_engine/test_support.rs','crates/deadsync-song-lua/src/live.rs','crates/deadsync-song-lua/Cargo.toml']
saved={n:{'sha256':hashlib.sha256((r/n).read_bytes()).hexdigest(),'base64':base64.b64encode((r/n).read_bytes()).decode()} for n in files}
p=d/'song-live-frames-edit-baseline.json';assert not p.exists();p.write_text(json.dumps(saved,indent=2)+'\n')
p=d/'song-live-frames-before-sources.json';assert not p.exists();p.write_bytes((d/'song-observations-reviewed-sources.json').read_bytes())
def edit(n,old,new):
 p=r/n;b=p.read_bytes();nl='\r\n' if b'\r\n' in b else '\n';old=old.replace('\n',nl).encode();new=new.replace('\n',nl).encode();assert b.count(old)==1,(n,b.count(old));p.write_bytes(b.replace(old,new))
edit(files[0],'mod loader;','mod live_frames;\npub use live_frames::SongLiveFrames;\nmod loader;')
edit(files[1],'mod lifecycle;','mod lifecycle;\nmod live_frames;')
print('Saved live-frame phase baseline and declared Rust adapter modules')
