"""Apply reviewed driver lifetime and observation-order corrections once."""
from pathlib import Path
import sys
assert sys.argv[1:]==['--apply']
root=Path(__file__).resolve().parents[3]
p=root/'crates/deadsync-song-lua/src/rust_engine/song_updates.rs';s=p.read_text()
a=s.index('    fn update_pose(');b=s.index('    fn update_motion(',a)
s=s[:a]+'''    fn update_pose(&mut self, value: Value, mut state: State) -> Result<State,String> {
        self.actors.apply_song_replay_pose(value,&mut state);Ok(state)
    }
'''+s[b:]
old='''                let state = self.update_pose(*v, State::default())?;
                crate::perframe::sample_player_pose(&mut out[i], &state, None);'''
assert old in s
s=s.replace(old,'''                let mut state = State::default();
                if self.actors.apply_song_replay_pose(*v,&mut state) {
                    crate::perframe::sample_player_pose(&mut out[i], &state, None);
                }''')
a=s.index('    fn call_song_update_frame(');b=s.index('\n}\n',a)
s=s[:a]+'''    fn call_song_update_frame(
        &mut self, screen:ActorScene, frame:usize, beat:f64, seconds:f64,
        delta_beats:f32, delta:f64,
    )->Result<(),String>{
        let prior=self.values().map_err(|e|e.to_string())?;
        let prior_delta=self.delta_values().map_err(|e|e.to_string())?;
        self.set_beat(beat as f32).map_err(|e|e.to_string())?;
        self.set_delta_values(delta_beats,delta as f32).map_err(|e|e.to_string())?;
        let globals=self.compiler_globals()?;
        let runtime=self.compiler_table_field(globals,SONG_LUA_RUNTIME_KEY)?;
        if !matches!(runtime,Value::Table(_)){return Err("song runtime: table expected".into())}
        let pin=self.vm.pin(runtime).map_err(|e|e.to_string())?;
        let result=(||{
            let function=self.vm.new_host_function_with_data(|call|{
                let(beat,seconds,clock)=call.function_data::<(f64,f64,Arc<Clock>)>().unwrap().clone();
                let runtime=call.captures()[0];
                for(key,value)in[(BEAT,Value::Number(beat)),(SECONDS,Value::Number(seconds))]{
                    let key=call.try_intern(key.as_bytes())?;call.table_set(runtime,key,value)?;
                }
                if let Some(clock)=&clock.timing {
                    let value=field(call,runtime,RATE)?;
                    let rate={let mut stack=call.native_stack()?;stack.push(value)?;
                        stack.to_number(-1).ok_or_else(||stack.error(b"music rate: number expected"))? as f32};
                    let position=clock.get_song_position((seconds*f64::from(rate))as f32+clock.get_time_for_beat_exact(0.));
                    for(key,value)in[(BPS,Value::Number(f64::from(position.bpm/60.))),(FREEZE,Value::Boolean(position.is_in_freeze)),(DELAY,Value::Boolean(position.is_in_delay))]{
                        let key=call.try_intern(key.as_bytes())?;call.table_set(runtime,key,value)?;
                    }
                }Ok(())
            },&[runtime],(beat,seconds,self.clock.clone())).map_err(|e|e.to_string())?;
            self.vm.call(function,&[]).map_err(|e|e.to_string())?;
            self.advance_options(seconds,None).map_err(|e|e.to_string())?;
            let result=self.advance_song_replay_at_frame(screen,frame,delta)
                .and_then(|_|self.draw_actor_scene(screen)).map(|_|()).map_err(|e|e.to_string());
            self.set_values(prior.0,prior.1).map_err(|e|e.to_string())?;
            self.set_delta_values(prior_delta.0,prior_delta.1).map_err(|e|e.to_string())?;
            result
        })();
        self.vm.unpin(pin).map_err(|e|e.to_string())?;result
    }
'''+s[b:]
p.write_text(s)

# Restrict native visibility changes to the four owned startup fields.
p=root/'crates/deadsync-song-lua/src/lua_util.rs';s=p.read_bytes()
a=s.index(b'pub struct SongLuaStartupState {');b=s.index(b'\n}',a)
chunk=s[a:b]
for name in [b'steps',b'commands',b'block_steps',b'clock_path']:
    before=b'    '+name+b':'
    assert chunk.count(before)==1
    chunk=chunk.replace(before,b'    pub(crate) '+name+b':')
p.write_bytes(s[:a]+chunk+s[b:])
