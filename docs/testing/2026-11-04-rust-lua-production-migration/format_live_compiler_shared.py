"""Format only the new shared payload declarations and generalized sorter."""
from pathlib import Path
import subprocess,sys
r=Path(__file__).resolve().parents[3];p=r/'crates/deadsync-song-lua/src/lib.rs'
b=p.read_bytes();s=b.decode().replace('\r\n','\n');out=s
ranges=[('pub type CompiledSongLua<OverlayActor>', 'impl<OverlayActor, LiveScene> CompiledSongLuaData<OverlayActor, LiveScene> {'),('pub fn sort_compiled_song_lua<OverlayActor, LiveScene>(', '\npub fn runtime_static_overlay_index_by_path')]
for start,end in ranges:
 a=out.index(start);z=out.index(end,a);original=out[a:z]
 formatted=subprocess.check_output(['rustfmt','--edition','2024','--config','skip_children=true','--emit','stdout'],input=original.encode()).decode()+'\n'
 out=out[:a]+formatted+out[z:]
if '--check' in sys.argv:assert out==s,'new shared declarations need rustfmt'
else:p.write_bytes(out.replace('\n','\r\n' if b'\r\n' in b else '\n').encode())
print('shared payload declarations formatted' if '--check' not in sys.argv else 'shared payload formatting passed')
