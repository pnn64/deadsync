local before,after
local fired=false
return Def.ActorFrame{
    Def.Quad{Name="Before",InitCommand=function(self) before=self end,OnCommand=cmd(zoomto,64,32;xy,100,100),KickMessageCommand=cmd(linear,0.4;x,200)},
    Def.Quad{Name="Driver",InitCommand=cmd(queuecommand,"Update"),OnCommand=cmd(visible,false),UpdateCommand=function(self)
        local beat=GAMESTATE:GetSongBeat()
        if beat>=0.3 then
            if not fired then fired=true;MESSAGEMAN:Broadcast("Kick") end
            before:y(100+beat*30):zoomy(1+beat/10)
            after:y(200+beat*30):zoomy(1+beat/10)
        end
        self:sleep(0.02):queuecommand("Update")
    end},
    Def.Quad{Name="After",InitCommand=function(self) after=self end,OnCommand=cmd(zoomto,64,32;xy,100,200),KickMessageCommand=cmd(linear,0.4;x,200)},
}
