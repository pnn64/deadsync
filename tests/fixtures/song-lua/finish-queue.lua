local target
local started,finished=false,false
return Def.ActorFrame{
    Def.Quad{Name="Driver",InitCommand=cmd(queuecommand,"Update"),OnCommand=cmd(visible,false),UpdateCommand=function(self)
        local b=GAMESTATE:GetSongBeat()
        if not started and b>=0.2 then started=true;target:linear(1):x(200):linear(1):x(100) end
        if not finished and b>=0.4 then finished=true;target:finishtweening():linear(0.2):x(150) end
        self:sleep(0.02):queuecommand("Update")
    end},
    Def.Quad{Name="Target",InitCommand=function(self) target=self end,OnCommand=cmd(xy,100,100;zoomto,64,32)},
}
