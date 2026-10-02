local flash
local fired=false
return Def.ActorFrame{
    Def.Quad{Name="Flash",InitCommand=function(self) flash=self;self:visible(false) end,OnCommand=cmd(zoomto,64,32;xy,100,100),HideCommand=cmd(visible,false)},
    Def.ActorFrame{InitCommand=function(self) self:SetUpdateFunction(function()
        if not fired and GAMESTATE:GetSongBeat()>=1 then
            fired=true;flash:visible(true):diffusealpha(1):sleep(0.25):linear(0.25):diffusealpha(0):queuecommand("Hide")
        end
    end) end},
}
