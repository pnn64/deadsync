from pathlib import Path
import base64,hashlib,json
r=Path('/mnt/c/GitHub/deadsync');d=r/'docs/testing/2026-11-04-rust-lua-production-migration';name='crates/deadsync-song-lua/src/rust_engine/compiler.rs';p=r/name;raw=p.read_bytes();saved=d/'song-compiler-facade-before-type-alias.json';assert not saved.exists()
saved.write_text(json.dumps({name:{'sha256':hashlib.sha256(raw).hexdigest(),'base64':base64.b64encode(raw).decode()}},indent=2)+'\n')
nl=b'\r\n' if b'\r\n' in raw else b'\n';marker=b'type Output<S, V> = CompiledSongLua<SongLuaOverlayActor<Kind<S, V>>>;';argument=b'    slots: fn(&Path, &Path, &Path) -> Result<Arc<[S]>, String>,';assert raw.count(marker)==1 and raw.count(argument)==1
raw=raw.replace(marker,marker+nl+b'type ModelSlots<S> = fn(&Path, &Path, &Path) -> Result<Arc<[S]>, String>;').replace(argument,b'    slots: ModelSlots<S>,')
p.write_bytes(raw);print('Introduced transparent model-slot reader type alias; no body changes')