from pathlib import Path
root=Path('/mnt/c/GitHub/deadsync')
p=root/'crates/deadsync-song-lua/src/rust_engine/song_updates.rs'
s=p.read_bytes();nl=b'\r\n' if b'\r\n' in s else b'\n'
def replace(old,new):
 global s
 old=old.replace('\n',nl.decode()).encode();new=new.replace('\n',nl.decode()).encode()
 assert s.count(old)==1,(old[:100],s.count(old));s=s.replace(old,new)
replace('use std::collections::{BTreeMap, HashMap};','use std::{collections::{BTreeMap, HashMap}, time::Instant};')
replace('            let mut broadcasts = Vec::new();\n            for (frame, (exact_beat, delta))', '''            let mut broadcasts = Vec::new();
            let profile = std::env::var_os("DEADSYNC_SONG_LUA_TIMING_STDERR").is_some();
            let mut reset_ms = 0.;
            let mut message_ms = 0.;
            let mut update_ms = 0.;
            let mut player_ms = 0.;
            let mut mod_ms = 0.;
            let mut column_ms = 0.;
            let mut overlay_ms = 0.;
            let mut frame_count = 0;
            for (frame, (exact_beat, delta))''')
replace('                let next_time = frame_time(next_beat, seconds);\n                for &value in tracked_values {','                let next_time = frame_time(next_beat, seconds);\n                frame_count = frame;\n                let stage = profile.then(Instant::now);\n                for &value in tracked_values {')
elapsed=lambda name: f'                {name}_ms += stage.map_or(0., |started| started.elapsed().as_secs_f64() * 1000.);\n                let stage = profile.then(Instant::now);\n'
replace('                replay_states.copy_from_slice(&current);',elapsed('reset')+'                replay_states.copy_from_slice(&current);')
replace('                    next_beat,\n                )?;\n                self.sample_song_overlay_schedule(', '                    next_beat,\n                )?;\n'+elapsed('message')+'                self.sample_song_overlay_schedule(')
replace('                self.restore_update_messages(&access, &values, &replay_states, started)?;',elapsed('update')+'                self.restore_update_messages(&access, &values, &replay_states, started)?;')
replace('                let music = self.update_music_seconds(seconds, context)?;',elapsed('overlay')+'                let music = self.update_music_seconds(seconds, context)?;')
replace('                player_samples.push(next_players);\n                mod_samples.push(', '                player_samples.push(next_players);\n'+elapsed('player')+'                mod_samples.push(')
replace('                column_samples.push(self.read_song_note_column_transform_samples()?);',elapsed('mod')+'                column_samples.push(self.read_song_note_column_transform_samples()?);')
replace('                self.capture_song_column_splines(&mut splines, (seconds * rate) as f32)?;\n                self.capture_update_fields(', '                self.capture_song_column_splines(&mut splines, (seconds * rate) as f32)?;\n                column_ms += stage.map_or(0., |started| started.elapsed().as_secs_f64() * 1000.);\n                self.capture_update_fields(')
replace('            let fallback_bpms = [(0., song_display_bps(context) * 60.)];','''            if profile {
                eprintln!("Song lua update timing: frames={frame_count} reset_ms={reset_ms:.3} message_ms={message_ms:.3} update_ms={update_ms:.3} player_ms={player_ms:.3} mod_ms={mod_ms:.3} column_ms={column_ms:.3} overlay_ms={overlay_ms:.3}");
            }
            let fallback_bpms = [(0., song_display_bps(context) * 60.)];''')
p.write_bytes(s)
p=root/'crates/deadsync-song-lua/tests/rust_song_compiler.rs';s=p.read_bytes();nl=b'\r\n' if b'\r\n' in s else b'\n'
new=r'''
#[test]
fn reload_media_boundary_preserves_only_final_on_and_zero_update_sounds() {
    let out = compare(&[r#"
        n=(n or 0)+1
        SOUND:PlayOnce('sound.ogg')
        return Def.ActorFrame{Name='root',
            InitCommand=function(self)SOUND:PlayOnce('sound.ogg')end,
            OnCommand=function(self)
                if n==1 then SCREENMAN:SetNewScreen('ScreenGameplay')
                else
                    SOUND:PlayOnce('sound.ogg')
                    self:SetUpdateFunction(function(self,dt)
                        if dt==0 then SOUND:PlayOnce('sound.ogg')end
                    end)
                end
            end,
            EndCommand=function(self)SOUND:PlayOnce('sound.ogg')end,
        }
    "#], |context| context.initial_lua_gc_stopped=true);
    assert_eq!(out[0].startup_reloads,1);
    assert_eq!(out[0].messages.iter().filter(|m| m.message.starts_with("__songlua_sound_call_")).count(),2);
}

#[test]
fn framebuffer_and_deferred_sprite_kinds_match_complete_compiler() {
    let out=compare(&[r#"
        local capture=Def.ActorFrameTexture{Name='capture',
            InitCommand=function(self)self:SetWidth(16):SetHeight(8):SetTextureName('compiled-aft'):EnableAlphaBuffer(true):Create()end,
            Def.Quad{Name='inside'},
        }
        return Def.ActorFrame{capture,
            Def.Sprite{Name='captured',InitCommand=function(self)self:SetTexture(capture:GetTexture())end},
            Def.Sprite{Name='late',OnCommand=function(self)self:Load('art.png')end},
        }
    "#], |_|{});
    assert!(out[0].overlays.iter().any(|r| matches!(r.kind,Kind::ActorFrameTexture{..})));
    assert!(out[0].overlays.iter().any(|r| matches!(r.kind,Kind::AftSprite{..})));
    assert!(out[0].overlays.iter().any(|r| r.name.as_deref()==Some("late") && matches!(r.kind,Kind::Sprite{..})));
}
'''
p.write_bytes(s+new.replace('\n',nl.decode()).encode())
print('Added existing timing diagnostics and two full-output comparisons')