"""Apply structural lint fixes without changing driver behavior."""
from pathlib import Path
import sys
assert sys.argv[1:]==['--apply']
root=Path(__file__).resolve().parents[3]
p=root/'crates/deadsync-song-lua/src/rust_engine/actors/song_messages.rs';s=p.read_text()
a=s.index('    pub(super) wrappers: Option<(');b=s.index('\n}',a)
s=s[:a]+'    pub(super) wrappers: Option<chronological::WrapperCapture>,'+s[b:];p.write_text(s)
p=root/'crates/deadsync-song-lua/src/rust_engine/actors/chronological.rs';s=p.read_text()
at=s.index('#[derive(Clone, Copy)]')
s=s[:at]+'''pub(super) struct WrapperCapture {
    start: usize,
    seen: usize,
    rows: Vec<(TableId, TableId, State)>,
}
'''+s[at:]
s=s.replace('capture.wrappers = Some((start, 0, Vec::new()));','capture.wrappers = Some(WrapperCapture {start, seen:0, rows:Vec::new()});')
s=s.replace('let Some((_, seen, rows)) = capture.wrappers.as_mut() else {','let Some(wrappers) = capture.wrappers.as_mut() else {')
s=s.replace('let result = rows[*seen..].to_vec();\n        *seen = rows.len();','let result = wrappers.rows[wrappers.seen..].to_vec();\n        wrappers.seen = wrappers.rows.len();')
s=s.replace('|(_, _, rows)| rows','|wrappers| wrappers.rows')
s=s.replace('let (start, _, rows) = capture.wrappers.as_mut().unwrap();','let wrappers = capture.wrappers.as_mut().unwrap();')
s=s.replace('capture.indices.insert(wrapper, *start + rows.len());\n    rows.push((owner, wrapper, initial));','capture.indices.insert(wrapper, wrappers.start + wrappers.rows.len());\n    wrappers.rows.push((owner, wrapper, initial));')
s=s.replace('.is_some_and(|a| effects::has_motion(a))','.is_some_and(effects::has_motion)')
p.write_text(s)
p=root/'crates/deadsync-song-lua/src/rust_engine/song_updates.rs';s=p.read_text()
s=s.replace('    active: Vec<Option<super::song_perframes::Active>>,','    active: Vec<Option<super::song_perframes::Active>>,\n    access: Access,')
s=s.replace("fn new(messages: &'a [SongLuaMessageEvent], count: usize) -> Self", "fn new(messages: &'a [SongLuaMessageEvent], count: usize, access: Access) -> Self")
s=s.replace('            active: vec![None; count],','            active: vec![None; count],\n            access,')
a=s.index('    fn advance<K>(');b=s.index('\n}\n',a);chunk=s[a:b]
chunk=chunk.replace('        access: &Access,\n','').replace('                    access,','                    &self.access,').replace('                access,','                &self.access,')
s=s[:a]+chunk+s[b:]
s=s.replace('Replay::new(messages, count)','Replay::new(messages, count, self.actors.perframe_access())')
s=s.replace('replay.advance(\n                self,\n                &access,','replay.advance(\n                self,').replace('replay.advance(\n                    self,\n                    &access,','replay.advance(\n                    self,')
# rustfmt may have retained a compact call.
s=s.replace('replay.advance(self, &access, context','replay.advance(self, context')
s=s.replace('use std::collections::{BTreeMap, HashMap};','use std::collections::{BTreeMap, HashMap};\ntype MotionSample = (Option<[f32;2]>,Option<[f32;3]>);')
s=s.replace('Result<(Option<[f32; 2]>, Option<[f32; 3]>), String>','Result<MotionSample, String>')
a=s.index('fn finish_samples<');head=s[:a];body=s[a:]
body=body.replace('for player in 0..LUA_PLAYERS {','for (player, mods) in from_mods.iter().enumerate() {').replace('if from_mods[player].get(key).is_none()','if mods.get(key).is_none()')
body=body.replace('&baseline_mods,','baseline_mods,').replace('&baseline_players,','baseline_players,')
p.write_text(head+body)
p=root/'docs/testing/2026-11-04-rust-lua-production-migration/prepare_update_samples.py';s=p.read_text()
needle='body=source[start:end]'
assert needle in s
s=s.replace(needle,needle+'''
# Borrowed generic inputs retain the native algorithm; enumerate the fixed
# player array and omit redundant references required only by native owned locals.
body=body.replace('for player in 0..LUA_PLAYERS {','for (player, mods) in from_mods.iter().enumerate() {').replace('if from_mods[player].get(key).is_none()','if mods.get(key).is_none()')
body=body.replace('&baseline_mods,','baseline_mods,').replace('&baseline_players,','baseline_players,')
''')
p.write_text(s)
