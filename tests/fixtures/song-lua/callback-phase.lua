local fired=false
return Def.ActorFrame{
    Def.Quad{Name="Before",OnCommand=cmd(xy,100,100;zoomto,64,32),KickMessageCommand=cmd(linear,0.2;x,200)},
    Def.ActorFrame{Name="Driver",InitCommand=function(self) self:SetUpdateFunction(function()
        if not fired and GAMESTATE:GetSongBeat()>=0.2 then fired=true;MESSAGEMAN:Broadcast("Kick") end
    end) end},
    Def.Quad{Name="After",OnCommand=cmd(xy,100,200;zoomto,64,32),KickMessageCommand=cmd(linear,0.2;x,200)},
}
