"""Extract the saved production track algorithms without reformatting native files."""
from pathlib import Path
import base64, hashlib, json, re, subprocess, sys
base=Path(__file__).resolve().parent
root=base.parents[2]
saved=json.loads((base/'song-frame-capture-edit-baseline.json').read_text(encoding='utf-8-sig'))
def original(path):return base64.b64decode(saved[path]['base64']).decode()
checked={}
check_only='--apply' not in sys.argv
def write(path,text):
    if check_only:
        if path.endswith('/frame_capture.rs') or path.endswith('/song_discovery.rs'):
            canonical=lambda s:subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=s,text=True,capture_output=True,check=True).stdout
            assert canonical(text)==canonical((root/path).read_text()),path
        else:
            assert text.encode()==(root/path).read_bytes(),path
        checked[path]=hashlib.sha256((root/path).read_bytes()).hexdigest()
    else:
        if (root/path).exists():
            current=(root/path).read_bytes()
            generated=text.encode()
            if current!=generated and (path not in saved or current!=original(path).encode()):
                canonical=lambda s:subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=s,text=True,capture_output=True,check=True).stdout
                assert canonical(current.decode())==canonical(text),f'Refusing to overwrite other changes: {path}'
        (root/path).write_bytes(text.encode())
native='crates/deadsync-song-lua/src/perframe.rs'
old=original(native)
text=old.replace('\r\n','\n')
between=lambda a,b:text[text.index(a):text.index(b)]
# Use the next public type as the stable end, preserving its attributes in native.
start=text.index('const PLAYER_TRANSFORM_TARGETS:')
end=text.index('\n};',start)+4
constant=text[start:end]
a=between('// Close an idle interval', 'fn capture_update_overlay_samples<')
c=between('#[cfg_attr(test, derive(Clone))]\nstruct SongLuaScheduledOverlaySample', 'fn apply_scheduled_overlay_states_uncached<')
f=between('#[cfg(test)]\nfn merge_completed_scheduled_overlay_samples(', 'pub fn call_update_functions_at(')
capture=between('fn capture_update_overlay_samples<', '#[cfg_attr(test, derive(Clone))]\nstruct SongLuaScheduledOverlaySample')
apply=between('fn apply_scheduled_overlay_states_uncached<', '#[cfg(test)]\nfn merge_completed_scheduled_overlay_samples(')
signature=capture[:capture.index(') -> Result<(), String> {')+len(') -> Result<(), String> {')]
native_wrapper=signature+'''
    let mut source = NativeFrameCapture { lua, overlays };
    crate::frame_capture::capture_update_overlay_samples(
        &mut source, crate::frame_capture::CaptureFrameArgs {
            context, baseline, from_states, update_states, to_states,
            restored_indices, tracks, track_indices, time, next_time, next_seconds,
            scheduled_samples, scratch,
        },
    )
}

pub(crate) struct NativeFrameCapture<'a, Actor> {
    pub(crate) lua: &'a Lua,
    pub(crate) overlays: &'a [Actor],
}
impl<Actor: std::borrow::Borrow<Table>> crate::frame_capture::CaptureSource for NativeFrameCapture<'_, Actor> {
    fn actor_count(&self) -> usize { self.overlays.len() }
    fn chronological(&self) -> bool {
        self.lua.app_data_ref::<crate::lua_util::SongLuaCompileFrames>().is_some()
    }
    fn drain(&mut self, visit: &mut crate::frame_capture::CaptureVisitor<'_>) -> Result<(), String> {
        let overlays = self.overlays;
        crate::lua_util::drain_overlay_update_capture(self.lua,
            |index, values, scheduled, final_values, reset| {
                let mut player = || {
                    let actor = overlays[index].borrow();
                    Ok(actor.raw_get::<Option<i64>>("__songlua_player_index")
                        .map_err(|e| e.to_string())?.is_some()
                        && actor.raw_get::<Option<String>>("__songlua_player_child_name")
                        .map_err(|e| e.to_string())?.is_none())
                };
                visit(index, values, scheduled, final_values, reset, &mut player)
            })
    }
    fn reset(&mut self, index: usize) -> Result<(), String> {
        reset_actor_capture(self.lua, self.overlays[index].borrow()).map_err(|e| e.to_string())
    }
}

'''
apply_start=apply.index('fn apply_scheduled_overlay_states<')
native_apply_signature=apply[apply_start:apply.index(') -> Result<(), String> {',apply_start)+len(') -> Result<(), String> {')]
native_apply=native_apply_signature+'''
    crate::frame_capture::apply_scheduled_overlay_states(states, scheduled, seconds,
        |index, target, value| {
            if let Some(actor) = overlays.get(index) {
                crate::lua_util::set_actor_overlay_update_getter_value(lua, actor.borrow(), target, value)?;
            }
            Ok(())
        })
}

'''
capture=capture.replace('fn capture_update_overlay_samples<Actor: std::borrow::Borrow<Table>>(\n    lua: &Lua,',
    'fn capture_update_overlay_samples(\n    source: &mut impl CaptureSource,')
capture=capture.replace('    overlays: &[Actor],\n','')
capture=capture.replace('''    if lua
        .app_data_ref::<crate::lua_util::SongLuaCompileFrames>()
        .is_some()
    {''','''    let chronological = source.chronological();
    if chronological {''')
capture=capture.replace('overlays.len()', 'source_len')
capture=capture.replace('    let chronological = source.chronological();','    let source_len = source.actor_count();\n    let chronological = source.chronological();')
capture=capture.replace('crate::lua_util::TWEEN_POSE_TARGETS','crate::tween_pose::TARGETS')
capture=capture.replace('''    crate::lua_util::drain_overlay_update_capture(
        lua,
        |overlay_index, values, scheduled, final_values, tween_reset| {''','''    source.drain(
        &mut |overlay_index, values, scheduled, final_values, tween_reset, player| {''')
ps=capture.index('            let actor = overlays[overlay_index].borrow();')
pe=capture.index('                // Player Lua getters retain destinations.',ps)
capture=capture[:ps]+'''            let player = player()?;
            if player || chronological {
'''+capture[pe:]
capture=capture.replace('''        reset_actor_capture(lua, overlays[overlay_index].borrow())
            .map_err(|err| err.to_string())?;''','''        source.reset(overlay_index)?;''')
assert 'lua_util' not in capture and '&Lua' not in capture and 'overlays[' not in capture, 'Unexpected Lua dependency'
cs=capture.index(') -> Result<(), String> {')+len(') -> Result<(), String> {')
capture='''fn capture_update_overlay_samples(
    source: &mut impl CaptureSource,
    args: CaptureFrameArgs<'_, impl std::hash::BuildHasher>,
) -> Result<(), String> {
    let CaptureFrameArgs { context, baseline, from_states, update_states, to_states,
        restored_indices, tracks, track_indices, time, next_time, next_seconds,
        scheduled_samples, scratch } = args;
'''+capture[cs:]
for name in ['apply_scheduled_overlay_states_uncached','apply_scheduled_overlay_states']:
    apply=apply.replace('fn '+name+'<Actor: std::borrow::Borrow<Table>>(\n    lua: &Lua,\n    overlays: &[Actor],', 'fn '+name+'(')
apply=apply.replace('    seconds: f64,\n) -> Result<(), String> {','    seconds: f64,\n    mut write: impl FnMut(usize, SongLuaOverlayUpdateTarget, &SongLuaOverlayUpdateValue) -> Result<(), String>,\n) -> Result<(), String> {')
apply=apply.replace('apply_scheduled_overlay_states_uncached(lua, overlays, states, scheduled, seconds)',
                    'apply_scheduled_overlay_states_uncached(states, scheduled, seconds, write)')
getters='''        if let Some(overlay) = overlays.get(sample.overlay_index) {
            crate::lua_util::set_actor_overlay_update_getter_value(
                lua,
                overlay.borrow(),
                sample.target,
                &value,
            )?;
        }'''
assert apply.count(getters)==2
apply=apply.replace(getters,'        write(sample.overlay_index, sample.target, &value)?;')
assert 'lua_util' not in apply and '&Lua' not in apply
types='crates/deadsync-song-lua/src/lua_util.rs'
lt=original(types)
ln=lt.replace('\r\n','\n')
begin=ln.index('#[derive(Clone)]\npub struct SongLuaScheduledOverlayUpdate {')
finish=ln.index('\n}\n',begin)+3
scheduled=ln[begin:finish].replace('#[derive(Clone)]','#[derive(Clone, Debug, PartialEq)]').replace('SongLuaScheduledOverlayUpdate','ScheduledUpdate')
write(types, lt.replace(ln[begin:finish].replace('\n','\r\n'), 'pub use crate::frame_capture::ScheduledUpdate as SongLuaScheduledOverlayUpdate;\r\n'))
header='''//! Shared owned-data track compilation for chronological song preparation.
use crate::{SongLuaCompileContext, SongLuaOverlayState, SongLuaOverlayUpdateSample,
    SongLuaOverlayUpdateTrack, SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue,
    song_beat_at_elapsed_seconds};

pub(crate) type PlayerProbe<'a> = &'a mut dyn FnMut() -> Result<bool, String>;
pub(crate) type CaptureVisitor<'a> = dyn FnMut(usize,
    &[(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)], &[ScheduledUpdate],
    &[(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)], bool, PlayerProbe<'_>)
    -> Result<(), String> + 'a;
pub(crate) trait CaptureSource {
    fn actor_count(&self) -> usize;
    fn chronological(&self) -> bool;
    fn drain(&mut self, visit: &mut CaptureVisitor<'_>) -> Result<(), String>;
    fn reset(&mut self, index: usize) -> Result<(), String>;
}

pub(crate) struct CaptureFrameArgs<'a, H: std::hash::BuildHasher> {
    pub context: &'a SongLuaCompileContext,
    pub baseline: &'a [SongLuaOverlayState],
    pub from_states: &'a [SongLuaOverlayState],
    pub update_states: &'a mut [SongLuaOverlayState],
    pub to_states: &'a mut [SongLuaOverlayState],
    pub restored_indices: &'a [usize],
    pub tracks: &'a mut Vec<SongLuaOverlayUpdateTrack>,
    pub track_indices: &'a mut std::collections::HashMap<(usize, SongLuaOverlayUpdateTarget), usize, H>,
    pub time: f32,
    pub next_time: f32,
    pub next_seconds: f64,
    pub scheduled_samples: &'a mut Vec<SongLuaScheduledOverlaySample>,
    pub scratch: &'a mut OverlaySampleScratch,
}

'''
core=header+scheduled+'\n'+constant+'\n'+a+capture+c+apply+f
core=core.replace('crate::lua_util::SongLuaScheduledOverlayUpdate','ScheduledUpdate')
core=re.sub(r'(?m)^fn ', 'pub(crate) fn ',core)
core=re.sub(r'(?m)^struct ', 'pub(crate) struct ',core)
core=core.replace('const PLAYER_TRANSFORM_TARGETS:', 'pub(crate) const PLAYER_TRANSFORM_TARGETS:')
for struct in ['OverlaySampleScratch','SongLuaScheduledOverlaySample']:
    block=re.search(r'pub\(crate\) struct '+struct+r' \{.*?\n\}',core,re.S).group()
    new=re.sub(r'(?m)^    ([a-z_][a-z_0-9]*):',r'    pub(crate) \1:',block)
    core=core.replace(block,new)
assert 'mlua' not in core and 'lua_util' not in core
write('crates/deadsync-song-lua/src/frame_capture.rs',core)
formatted=lambda code:subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=code,text=True,capture_output=True,check=True).stdout
new=text.replace(constant,'').replace(a,'').replace(c,'').replace(f,'').replace(between('fn capture_update_overlay_samples<', '#[cfg_attr(test, derive(Clone))]\nstruct SongLuaScheduledOverlaySample'),formatted(native_wrapper)+'\n').replace(between('fn apply_scheduled_overlay_states_uncached<', '#[cfg(test)]\nfn merge_completed_scheduled_overlay_samples('),formatted(native_apply)+'\n')
new='pub(crate) use crate::frame_capture::*;\n'+new
new=new.replace('SongLuaOverlayState, SongLuaOverlayUpdateSample, SongLuaOverlayUpdateTarget,',
                'SongLuaOverlayState, SongLuaOverlayUpdateTarget,')
write(native,new.replace('\n','\r\n'))
sd='crates/deadsync-song-lua/src/rust_engine/song_discovery.rs'
raw=original(sd)
sn=raw.replace('\r\n','\n')
start=sn.index('/// Existing compiler scheduling contract.')
end=sn.index('\nimpl RustSongRuntime',start)
new=sn[:start]+'pub use crate::frame_capture::ScheduledUpdate as SongReplayScheduledUpdate;\n'+sn[end:]
write(sd,new.replace('\n','\r\n') if '\r\n' in raw else new)
lib='crates/deadsync-song-lua/src/lib.rs'
write(lib,original(lib).replace('mod perframe;','mod frame_capture;\r\nmod perframe;'))
if check_only:
    (base/'song-frame-capture-extraction-proof.json').write_text(json.dumps(dict(
        saved_native_source_sha256=saved[native]['sha256'],
        extraction_matches_saved_algorithms=True,checked=checked),indent=2)+'\n')
    print(json.dumps(checked))
else:
    print('Extracted shared frame compiler and native wrappers')
