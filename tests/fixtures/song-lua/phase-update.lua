local before, after
local fired=false
return Def.ActorFrame{
    Def.Quad{
        Name="Before", InitCommand=function(self) before=self end,
        OnCommand=cmd(zoomto,64,32;xy,100,100),
        KickMessageCommand=cmd(stoptweening;linear,0.2;x,200),
    },
    Def.Quad{
        Name="Driver", InitCommand=cmd(xy,100,100;queuecommand,"Update"), OnCommand=cmd(zoomto,16,16),
        UpdateCommand=function(self)
            if GAMESTATE:GetSongBeat() >= 0.19 then
            if GAMESTATE:GetSongBeat() >= 0.3 and not fired then fired=true; MESSAGEMAN:Broadcast("Kick") end
            self:x(before:GetX()):y(after:GetX())
            end
            self:sleep(0.02):queuecommand("Update")
        end,
    },
    Def.Quad{
        Name="After", InitCommand=function(self) after=self end,
        OnCommand=cmd(zoomto,64,32;xy,100,200),
        KickMessageCommand=cmd(stoptweening;linear,0.2;x,200),
    },
}
