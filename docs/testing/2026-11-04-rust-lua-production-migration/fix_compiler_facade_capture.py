from pathlib import Path
import json,base64,hashlib
root=Path('/mnt/c/GitHub/deadsync');d=root/'docs/testing/2026-11-04-rust-lua-production-migration';bp=d/'song-compiler-facade-edit-baseline-v2.json'
b=json.loads(bp.read_text(encoding='utf-8-sig'))
def edit(name,old,new):
 p=root/name;data=p.read_bytes()
 if name not in b:b[name]={'sha256':hashlib.sha256(data).hexdigest(),'base64':base64.b64encode(data).decode()}
 nl='\r\n' if b'\r\n' in data else '\n';old=old.replace('\n',nl).encode();new=new.replace('\n',nl).encode()
 assert data.count(old)==1,(name,data.count(old))
 p.write_bytes(data.replace(old,new))
edit('crates/deadsync-song-lua/src/rust_engine/song_updates.rs',
 '            self.actors.replace_song_capture_indices(screen, &values);\n            self.actors.begin_song_wrapper_capture(screen, values.len());',
 '            self.actors.replace_song_capture_indices(screen, &values);\n            self.begin_actor_capture(false);\n            self.actors.begin_song_wrapper_capture(screen, values.len());')
edit('crates/deadsync-song-lua/src/rust_engine/song_updates.rs',
 '            self.actors.end_song_update_capture(screen);\n            Ok(SongUpdateOutput {',
 '            self.end_actor_capture();\n            self.actors.end_song_update_capture(screen);\n            Ok(SongUpdateOutput {')
edit('crates/deadsync-song-lua/src/rust_engine.rs',
 '        let mut vm = Vm::new();\n        let source_loader = vm.get_global(b"loadstring")?;',
 '        let mut vm = Vm::new();\n        #[cfg(feature = "test-support")]\n        if context.initial_lua_gc_stopped {\n            vm.with_native_stack(|stack| stack.gc(deadlib_lua::GcOperation::Stop).map(|_| ()))?;\n        }\n        let source_loader = vm.get_global(b"loadstring")?;')
edit('crates/deadsync-song-lua/tests/rust_song_compiler.rs',
 "                else MESSAGEMAN:Broadcast('FinalOn')end",
 "                else\n                    MESSAGEMAN:Broadcast('FinalOn')\n                    self:SetUpdateFunction(function(self,dt)\n                        if dt==0 then MESSAGEMAN:Broadcast('FinalZero')end\n                    end)\n                end")
edit('crates/deadsync-song-lua/tests/rust_song_compiler.rs',
 '.filter(|m| m.message == "FinalOn")', '.filter(|m| m.message == "FinalZero")')
edit('crates/deadsync-song-lua/tests/rust_song_compiler.rs',
 '        "ReloadConstruction" | "AuthoredInit" | "OutgoingEnd"',
 '        "ReloadConstruction" | "AuthoredInit" | "OutgoingEnd" | "FinalOn"')
p=root/'crates/deadsync-song-lua/src/frame_capture.rs';raw=p.read_bytes();nl=b'\r\n' if b'\r\n' in raw else b'\n';mark=b'/// Owned startup queue output shared by preparation adapters and the oracle.';start=raw.index(mark);block=raw[start:];assert block.count(b'pub struct')==1
raw=raw[:start].rstrip(b'\r\n')+nl
where=raw.index(b'pub(crate) type PlayerProbe')
p.write_bytes(raw[:where]+block.rstrip(b'\r\n')+nl+nl+raw[where:])
bp.write_text(json.dumps(b,indent=2)+'\n')
print('edited five files; baseline entries',len(b))