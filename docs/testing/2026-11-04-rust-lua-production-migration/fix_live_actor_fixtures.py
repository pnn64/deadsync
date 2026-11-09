"""Keep the original new fixtures, then use actual initialized actor handles."""
from pathlib import Path
r=Path(__file__).resolve().parents[3];d=Path(__file__).parent
p=r/'crates/deadsync-song-lua/tests/rust_live_actors.rs';s=p.read_text()
backup=d/'song-live-actors-tests-before-v3.rs';assert not backup.exists();backup.write_bytes(p.read_bytes())
s=s.replace("ROOT=Def.ActorFrame{Name='Song',Def.ActorFrame{Name='Child'}}; return ROOT", "return Def.ActorFrame{Name='Song',InitCommand=function(self)ROOT=self end,Def.ActorFrame{Name='Child'}}")
s=s.replace("ROOT=Def.ActorFrame{Name='Song',Def.ActorFrame{Name='Child'}};return ROOT", "return Def.ActorFrame{Name='Song',InitCommand=function(self)ROOT=self end,Def.ActorFrame{Name='Child'}}")
s=s.replace('ROOT=Def.ActorFrame{};return ROOT','return Def.ActorFrame{InitCommand=function(self)ROOT=self end}')
s=s.replace("if a then a:SetUpdateFunction(function(_,d)note(name,d)end)end", "if a then a.__songlua_update_function=function(_,d)note(name,d)end end")
s=s.replace("nf:GetColumnActors()[1]:SetUpdateFunction(function(_,d)note('Column',d)end)","nf:GetColumnActors()[1].__songlua_update_function=function(_,d)note('Column',d)end")
s=s.replace("nf:GetColumnActors()[1]:SetUpdateFunction(function(_,d)note('column',d)end)","nf:GetColumnActors()[1].__songlua_update_function=function(_,d)note('column',d)end")
s=s.replace("GetPlayerOptions('ModsLevel_Current').__songlua_player_option_state.mini=1\"", "GetPlayerOptions('ModsLevel_Song'):Mini(1)\"")
p.write_text(s)
