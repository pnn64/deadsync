local target,waiter
local fired,off=false,false
return Def.ActorFrame{
    Def.Quad{Name="Driver",InitCommand=cmd(queuecommand,"Update"),OnCommand=cmd(visible,false),UpdateCommand=function(self)
        local b=GAMESTATE:GetSongBeat()
        if not fired and b>=0.25 then fired=true;target:playcommand("Hit");waiter:sleep(0.4):queuecommand("Respawn") end
        if not off and b>=0.7 then off=true;MESSAGEMAN:Broadcast("Off") end
        self:sleep(0.02):queuecommand("Update")
    end},
    Def.Quad{Name="Waiter",InitCommand=function(self) waiter=self end,OnCommand=cmd(visible,false),RespawnCommand=function() target:queuecommand("Respawn") end},
    Def.ActorFrame{InitCommand=function(self) target=self end,
        Def.Quad{Name="Target",OnCommand=cmd(xy,100,100;zoomto,64,32;zoom,0.75),OffMessageCommand=cmd(bouncebegin,0.2;zoom,0),HitCommand=cmd(visible,false),RespawnCommand=cmd(sleep,0.14;queuecommand,"Show"),ShowCommand=cmd(visible,true)},
        Def.Quad{Name="Flash",OnCommand=cmd(xy,100,200;zoomto,64,32;zoom,0.75;visible,false),HitCommand=cmd(visible,true;sleep,0.14;queuecommand,"Hide"),RespawnCommand=cmd(visible,true;sleep,0.14;queuecommand,"Hide"),HideCommand=cmd(visible,false)},
    },
    Def.Quad{Name="Slap",OnCommand=cmd(xy,200,200;zoomto,64,32;visible,false),OffMessageCommand=cmd(visible,true;linear,0.1;x,250;queuecommand,"Middle"),MiddleCommand=cmd(zoomy,1.25;linear,0.1;zoomy,1;queuecommand,"Hide"),HideCommand=cmd(visible,false)},
}
