return Def.ActorFrame{
    Name="Root",
    Def.ActorFrame{Name="Driver", OnCommand=function(self) self:SetUpdateFunction(function() end) end},
    Def.Quad{Name="Monitor", InitCommand=cmd(xy,427,240;setsize,64,64),
        OnCommand=cmd(glowshift;effectcolor1,11,1,1,.3;effectcolor2,1,1,1,0;effectperiod,.2;effectclock,"timer")},
}
