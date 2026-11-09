from pathlib import Path
import json,re
r=Path('/mnt/c/GitHub/deadsync/crates/deadsync-song-lua');d=Path(__file__).parent
p=r/'tests/rust_song_observations.rs';s=p.read_text();old=s
def cases(s):
 ms=list(re.finditer(r'(?m)^#\[test\]\nfn (\w+)\(\) \{',s))
 return {m[1]:s[m.start():ms[i+1].start() if i+1<len(ms) else len(s)].strip() for i,m in enumerate(ms)}
cs=cases(s)
def source(name):return re.findall(r'r#"([\s\S]*?)"#',cs[name])
archived={k:source(k) for k in cs};(d/'song-observations-original-fixtures.json').write_text(json.dumps(archived,indent=2)+'\n')
def change(name,a,b):
 global s
 assert cs[name].count(a)>0
 new=cs[name].replace(a,b);assert s.count(cs[name])==1;s=s.replace(cs[name],new);cs[name]=new
change('chronological_option_setters_and_gc_match','return Def.Actor{OnCommand','return Def.ActorFrame{OnCommand')
change('chronological_option_setters_and_gc_match',"'ModsLevel_Stage'","'ModsLevel_Song'")
change('multilayer_screen_and_player_write_indices_match','return Def.Actor{OnCommand','return Def.ActorFrame{OnCommand')
change('multilayer_screen_and_player_write_indices_match','return Def.Quad{OnCommand','return Def.ActorFrame{OnCommand')
change('nested_manager_calls_and_reload_history_match','InitCommand=function(self)','OnCommand=function(self)')
change('startup_queue_capture_excludes_noteskin_discovery_writes',"'ModsLevel_Stage'","'ModsLevel_Song'")
change('chronological_direct_queued_update_and_message_writes_match','self:z(p.value):basezoomx(.5)','if p then self:z(p.value):basezoomx(.5)end')
# Refactor only the new helper so rejected fixtures have direct controls too.
start=s.index('fn compare(');end=s.index('fn compare_output',start);block=s[start:end]
prefix=block[:block.index('    assert_eq!(')]
prefix=prefix.replace('fn compare(', 'fn compile_pair(').replace(') -> Vec<Output> {', ') -> (Result<Vec<Output>, String>, Result<Vec<Output>, String>) {')
assert prefix!=block
prefix+='    (native, rust)\n}\n'
new='fn compare(sources: &[&str], configure: impl FnOnce(&mut SongLuaCompileContext)) -> Vec<Output> {\n    let (native, rust) = compile_pair(sources, configure);\n'+block[block.index('    assert_eq!('):]
s=s[:start]+prefix+new+s[end:]
def literal(text):return 'r#"'+text+'"#'
invalid=archived['chronological_option_setters_and_gc_match']+archived['multilayer_screen_and_player_write_indices_match']
s+='\n#[test]\nfn original_non_frame_update_fixtures_remain_rejected() {\n'
s+='    for source in ['+','.join(literal(x) for x in invalid)+'] {\n'
s+='        let (native, rust) = compile_pair(&[source], |_| {});\n        for result in [native,rust] { assert!(result.unwrap_err().contains("SetUpdateFunction")); }\n    }\n}\n'
s+='\n#[test]\nfn original_init_screen_request_remains_discarded() {\n    let out=compare(&['+literal(archived['nested_manager_calls_and_reload_history_match'][0])+'], |_|{});\n    assert_eq!(out[0].startup_reloads,0);\n}\n'
s+='\n#[test]\nfn original_stage_queue_is_not_the_published_song_option_owner() {\n    let out=compare(&['+literal(archived['startup_queue_capture_excludes_noteskin_discovery_writes'][0])+'], |_|{});\n    assert!(out[0].boolean_writes.is_empty());\n}\n'
s+='\n#[test]\nfn original_nil_message_probe_preserves_error_cause_and_observations() {\n    let (native,rust)=compile_pair(&['+literal(archived['chronological_direct_queued_update_and_message_writes_match'][0])+'], |_|{});\n'
s+='''    let (mut a,mut b)=(native.unwrap(),rust.unwrap());
    assert_eq!(a.len(),b.len());
    for (index,(a,b)) in a.iter_mut().zip(&mut b).enumerate() {
        for output in [&mut *a,&mut *b] {
            let diagnostics=&mut output.info.skipped_message_command_captures;
            assert_eq!(diagnostics.len(),1);
            let cause="attempt to index local 'p' (a nil value)";
            assert!(diagnostics[0].contains("TouchMessageCommand") && diagnostics[0].contains(cause));
            // Only the engine-specific rendering is normalized in this control.
            diagnostics[0]=format!("TouchMessageCommand: {cause}");
        }
        compare_output(a,b,index);
    }
}
'''
s+='''
#[test]
fn queued_update_command_keeps_last_same_frame_immediate_setter() {
    let out=compare(&[r#"
        local queued=false
        return Def.ActorFrame{OnCommand=function(self)self:SetUpdateFunction(function(self)
            self:visible(false):basezoomx(.2)
            if not queued then queued=true;self:sleep(.02):queuecommand('Later')end
        end)end,LaterCommand=function(self)self:playcommand('Update')end,
        UpdateCommand=function(self)self:visible(false):visible(true):basezoomx(.3):basezoomx(.6)end}
    "#], |_|{});
    assert!(!out[0].overlay_writes.is_empty());
}
'''
assert s!=old;p.write_text(s)
print('Repaired fixture assumptions; retained every original source as a control')
