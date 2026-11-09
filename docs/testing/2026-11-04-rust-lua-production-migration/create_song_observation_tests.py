from pathlib import Path
r=Path('/mnt/c/GitHub/deadsync/crates/deadsync-song-lua')
s=(r/'tests/rust_song_compiler.rs').read_text().split('#[test]')[0]
s=s.replace('        info\n','        info,\n        boolean_writes,\n        speed_writes,\n        noteskin_writes,\n        state_calls,\n        overlay_writes\n')
s=s.replace('    // The production payload is compared in full above. Reference-only setter\n    // observations remain tested by their dedicated adapter/oracle controls.\n','')
s += r'''
#[test]
fn startup_option_setters_and_partial_speed_failures_match() {
    let out = compare(&[r#"
        return Def.Actor{InitCommand=function(self)
            local ps=GAMESTATE:GetPlayerState(PLAYER_1)
            for _,level in ipairs{'ModsLevel_Preferred','ModsLevel_Stage','ModsLevel_Song','ModsLevel_Current'} do
                local po=ps:GetPlayerOptions(level)
                po:Mirror(true,false);po:Mirror(false,true)
                po:XMod(2,.4,true);po:TimeSpacing(0,.2)
                pcall(function()po:CMod(-1)end)
                pcall(function()po:MMod(333,-1)end)
                pcall(function()po:ScrollSpeed(3,-1)end)
                po:ScrollBPM(444,.3);po:MaxScrollBPM(555,.6)
            end
        end}
    "#], |_|{});
    assert!(!out[0].boolean_writes.is_empty());
    assert!(out[0].speed_writes.iter().any(|w|w.failed));
}

#[test]
fn accepted_rejected_and_parsed_noteskin_writes_match() {
    let out=compare(&[r#"
        NOTESKIN.DoesNoteSkinExist=function(self,name)return name=='accepted'end
        return Def.Actor{InitCommand=function(self)
            local ps=GAMESTATE:GetPlayerState(PLAYER_1)
            for _,level in ipairs{'ModsLevel_Preferred','ModsLevel_Stage','ModsLevel_Song','ModsLevel_Current'} do
                local po=ps:GetPlayerOptions(level)
                po:NoteSkin('accepted');po:NoteSkin('rejected',true)
                po:FromString('50% Mini, accepted')
                ps:SetPlayerOptions(level,'25% Mini, accepted')
            end
        end}
    "#], |_|{});
    assert!(!out[0].noteskin_writes.is_empty());
}

#[test]
fn nested_manager_calls_and_reload_history_match() {
    let out=compare(&[r#"
        visits=(visits or 0)+1
        return Def.ActorFrame{
            InitCommand=function(self)
                MESSAGEMAN:Broadcast('Outer')
                if visits==1 then SCREENMAN:SetNewScreen('ScreenGameplay')end
            end,
            OuterMessageCommand=function(self)MESSAGEMAN:Broadcast('Inner')end,
            InnerMessageCommand=function(self)GAMESTATE:SetCurrentSteps(PLAYER_1,nil)end,
        }
    "#], |_|{});
    assert_eq!(out[0].startup_reloads,1);
    assert!(out[0].state_calls.len()>=4);
}

#[test]
fn chronological_direct_queued_update_and_message_writes_match() {
    let out=compare(&[r#"
        local started=false
        return Def.ActorFrame{Name='observed',
            OnCommand=function(self)self:SetUpdateFunction(function(self)
                local beat=GAMESTATE:GetSongBeat()
                self:x(beat*12):x(beat*14):diffusealpha(.8):visible(true)
                if not started then
                    started=true
                    self:linear(.03):y(7):queuecommand('Later')
                    MESSAGEMAN:Broadcast('Touch',{value=8})
                end
                self:playcommand('Update')
            end)end,
            UpdateCommand=function(self)self:z(9):draworder(2)end,
            LaterCommand=function(self)self:z(11):playcommand('Update')end,
            TouchMessageCommand=function(self,p)self:z(p.value):basezoomx(.5)end,
        }
    "#], |_|{});
    assert!(!out[0].overlay_writes.is_empty());
    assert!(!out[0].state_calls.is_empty());
}

#[test]
fn chronological_option_setters_and_gc_match() {
    let out=compare(&[r#"
        return Def.Actor{OnCommand=function(self)self:SetUpdateFunction(function(self)
            local po=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Stage')
            po:Mirror(true,true);po:XMod(2,.7);po:FromString('25% Mini')
            collectgarbage()
        end)end}
    "#], |_|{});
    assert!(out[0].boolean_writes.len()>1);
    assert!(out[0].noteskin_writes.len()>1);
}

#[test]
fn startup_queue_capture_excludes_noteskin_discovery_writes() {
    let out=compare(&[r#"
        NOTESKIN.DoesNoteSkinExist=function()return true end
        return Def.Actor{OnCommand=function(self)self:sleep(.03):queuecommand('Later')end,
            LaterCommand=function(self)
                local po=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Stage')
                po:Mirror(true);po:XMod(3);po:NoteSkin('queued')
                MESSAGEMAN:Broadcast('Queued')
            end}
    "#], |_|{});
    assert!(!out[0].boolean_writes.is_empty());
}

#[test]
fn multilayer_screen_and_player_write_indices_match() {
    let out=compare(&[r#"
        return Def.Actor{OnCommand=function(self)self:SetUpdateFunction(function(self)
            local s=SCREENMAN:GetTopScreen(); s:x(3)
            s:GetChild('PlayerP1'):y(4)
            self:rotationz(5)
        end)end}
    "#,r#"
        return Def.Quad{OnCommand=function(self)self:SetUpdateFunction(function(self)
            self:zoom(.7):basealpha(.4)
        end)end}
    "#], |c|c.background_layer_count=1);
    assert_eq!(out.len(),2);
    assert!(out.iter().any(|o|!o.overlay_writes.is_empty()));
}
'''
p=r/'tests/rust_song_observations.rs'; assert not p.exists();p.write_text(s)
p=r/'Cargo.toml';p.write_bytes(p.read_bytes()+b'\n[[test]]\nname = "rust_song_observations"\nrequired-features = ["rust-engine", "test-support"]\n')
print('Created seven full-payload oracle cases including all five observation fields')
