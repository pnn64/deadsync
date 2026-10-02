return Def.ActorFrame{
    OnCommand=function(self) self:SetUpdateFunction(function() end) end,
    Def.Quad{Name="TweenSpin",OnCommand=cmd(zoomto,64,32;xy,100,200;rotationz,10;spin;effectmagnitude,0,0,60;linear,0.4;diffusealpha,0.2)},
    Def.Quad{Name="BeatSpin",OnCommand=cmd(zoomto,64,32;xy,250,100;rotationz,20;spin;effectclock,"beat";effectmagnitude,0,0,10)},
}
