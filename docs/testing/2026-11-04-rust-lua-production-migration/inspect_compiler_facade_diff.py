from pathlib import Path
import base64,difflib,json,hashlib
root=Path('/mnt/c/GitHub/deadsync');d=root/'docs/testing/2026-11-04-rust-lua-production-migration';before=json.loads((d/'song-compiler-facade-before-sources-v2.json').read_text(encoding='utf-8-sig'));b=json.loads((d/'song-compiler-facade-edit-baseline-v2.json').read_text(encoding='utf-8-sig'))
for name in sorted(b):
 if name not in before or not name.endswith(('.rs','.toml')):continue
 p=root/name;old=base64.b64decode(b[name]['base64']);assert hashlib.sha256(old).hexdigest()==before[name],name
 if old==p.read_bytes():continue
 print(name)
 if name.endswith(('compile.rs','perframe.rs','lua_util.rs','song_activation.rs','Cargo.toml')):
  diff=list(difflib.unified_diff(old.decode().splitlines(),p.read_text().splitlines(),n=1))
  print('\n'.join(diff[:52]))
print('preserved baseline hashes verified')