"""Preserve native cursor reads and the distinct frame/field motion order."""
from pathlib import Path
import sys
assert sys.argv[1:]==['--apply']
root=Path(__file__).resolve().parents[3]
p=root/'crates/deadsync-song-lua/src/rust_engine/actors/chronological.rs';s=p.read_text()
s=s.replace('Motion([f32; 2]),','Motion([f32; 2], bool),')
start=s.index('        Query::Motion(clock) =>');end=s.index('        Query::Mask',start)
s=s[:start]+'''        Query::Motion(clock, include_spin) => {
            let time=effect_time(call,&data.store,actor,clock)?;
            let spin=if include_spin {render_spin(call,&data.store,actor)?}else{None};
            Answer::Motion(time,spin)
        }
'''+s[end:]
start=s.index('fn with_wrappers');end=s.index('\nfn active',start)
s=s[:start]+'''fn visit_wrappers(
    call:&mut HostCall<'_>,store:&Shared,actor:TableId,ordinary:bool,reverse:bool,
    mut visit:impl FnMut(&mut HostCall<'_>,TableId)->Result<bool,Error>,
)->Result<bool,Error>{
    let value=if ordinary {sprites::field(call,actor,"__songlua_wrappers")?}else{get(call,actor,b"__songlua_wrappers")?};
    let array=optional_table(call,value)?;
    call.with_roots(&[value],|call|{
        let len=if let Some(array)=array {call.raw_table_len(array)?}else{store.lock().unwrap().actors.get(&actor).map_or(0,|a|a.wrappers.len())};
        let mut index=if reverse{len}else{1};
        loop{
            if reverse && index==0 {return Ok(false)}
            let value=if let Some(array)=array {call.raw_table_get(array,Value::Number(index as f64))?}else{
                store.lock().unwrap().actors.get(&actor).and_then(|a|a.wrappers.get(index-1)).copied().map_or(Value::Nil,Value::Table)
            };
            if value==Value::Nil && !reverse{return Ok(false)}
            let wrapper=table(call,value)?;
            if call.with_roots(&[value],|call|visit(call,wrapper))? {return Ok(true)}
            if reverse{index-=1}else{index+=1}
        }
    })
}
'''+s[end:]
start=s.index('    if with_wrappers(',s.index('fn active'));end=s.index('    let mut index = 1;',start)
s=s[:start]+'''    if visit_wrappers(call,store,actor,true,false,|call,row|active(call,store,row))? {return Ok(true)}
'''+s[end:]
start=s.index('fn motion(');end=s.index('\nfn field_wrappers',start)
s=s[:start]+'''fn render_spin(call:&mut HostCall<'_>,store:&Shared,actor:TableId)->Result<Option<[f32;3]>,Error>{
    let fallback={let guard=store.lock().unwrap();guard.actors.get(&actor).filter(|a|a.effects.spin_baked).map(|a|[a.current.rot_x_deg,a.current.rot_y_deg,a.current.rot_z_deg])};
    let value=get(call,actor,b"__songlua_state_spin")?;
    let Some(pose)=optional_table(call,value)?else{return Ok(fallback)};
    call.with_roots(&[value],|call|{
        let value=sprites::field(call,pose,"baked")?;
        if !boolean(call,value)?.unwrap_or(false){return Ok(None)}
        let value=sprites::field(call,pose,"rotation")?;let rotation=table(call,value)?;
        call.with_roots(&[value],|call|{
            let mut result=[0.;3];
            for(i,out)in result.iter_mut().enumerate(){let value=call.raw_table_get(rotation,Value::Number((i+1)as f64))?;*out=sprites::number(call,value)? as f32;}
            Ok(Some(result))
        })
    })
}
fn effect_time(call:&mut HostCall<'_>,store:&Shared,actor:TableId,clock:[f32;2])->Result<Option<[f32;2]>,Error>{
    let fallback={let guard=store.lock().unwrap();guard.actors.get(&actor).map(|a|{let mut state=a.state.render;effects::render_time(a,clock,&mut state);state})};
    let value=get(call,actor,b"__songlua_state_rainbow")?;
    let rainbow=boolean(call,value)?.unwrap_or_else(||fallback.is_some_and(|s|s.rainbow));
    if !rainbow {
        let value=get(call,actor,b"__songlua_state_effect_mode")?;
        let selected=if value==Value::Nil {
            fallback.is_some_and(|s|!matches!(s.effect_mode,deadlib_present::anim::EffectMode::None|deadlib_present::anim::EffectMode::Spin))
        }else{
            initial_state::value_text(call,value)?.is_some_and(|s|matches!(s.as_str(),"bob"|"bounce"|"wag"|"pulse"|"diffuseblink"|"diffuseshift"|"diffuseramp"|"glowblink"|"glowshift"|"glowramp"))
        };
        if !selected{return Ok(None)}
    }
    let value=get(call,actor,b"__songlua_state_motion_clock")?;
    let Some(motion)=optional_table(call,value)?else{return Ok(fallback.and_then(|s|s.effect_time))};
    call.with_roots(&[value],|call|{
        let value=get(call,actor,b"__songlua_state_effect_clock")?;
        let beat=initial_state::value_text(call,value)?.is_some_and(|s|matches!(s.as_str(),"beat"|"beatnooffset"));
        let value=sprites::field(call,motion,"units")?;
        let units=if value==Value::Nil{0.}else{sprites::number(call,value)? as f32};
        Ok(Some([clock[usize::from(beat)],units]))
    })
}
'''+s[end:]
start=s.index('        call.with_roots(&[value],',s.index('fn field_wrappers'));s=s[:start]+'''        call.with_roots(&[value],|call|{
            let mut states=Vec::new();
            visit_wrappers(call,store,field,false,true,|call,id|{
                ensure(call,store,id)?;
                let mut state=initial_state::read(call,store,id)?;
                replay_pose(store,id,&mut state);
                if let Some(spin)=render_spin(call,store,id)?{[state.rot_x_deg,state.rot_y_deg,state.rot_z_deg]=spin;state.spin_baked=true;}
                if let Some(time)=effect_time(call,store,id,clock)?{state.effect_time=Some(time);}
                states.push(state);Ok(false)
            })?;
            Ok(Some(states))
        })
    })
}
'''
p.write_text(s)
p=root/'crates/deadsync-song-lua/src/rust_engine/song_updates.rs';s=p.read_text()
s=s.replace('self.update_motion(value, [music, start])?', 'self.update_motion(value, [music, start], false)?')
s=s.replace('self.update_motion(value, [music, next_beat])?', 'self.update_motion(value, [music, next_beat], true)?')
start=s.index('    fn update_motion(');end=s.index('    fn update_masks(',start)
chunk=s[start:end].replace('        clock: [f32; 2],','        clock: [f32; 2],\n        include_spin:bool,').replace('Query::Motion(clock)','Query::Motion(clock,include_spin)')
s=s[:start]+chunk+s[end:]
p.write_text(s)
