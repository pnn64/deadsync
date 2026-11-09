import base64,hashlib,json
from pathlib import Path
r=Path('/mnt/c/GitHub/deadsync'); d=Path(__file__).parent
p=r/'crates/deadsync-song-lua/src/rust_engine/test_support/update_driver.rs'
b=p.read_bytes(); saved=json.loads((d/'song-observations-edit-baseline.json').read_text(encoding='utf-8-sig'))
name=p.relative_to(r).as_posix(); assert name not in saved
saved[name]={'sha256':hashlib.sha256(b).hexdigest(),'base64':base64.b64encode(b).decode()}
(d/'song-observations-edit-baseline.json').write_text(json.dumps(saved,indent=2)+'\n')
old=b'        column_splines: splines,'; assert b.count(old)==1
p.write_bytes(b.replace(old,old+b'\n        overlay_writes: lua.remove_app_data::<crate::lua_util::SongLuaOverlayWrites>().unwrap_or_default().0,'))
