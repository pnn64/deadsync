local modes = {"diffuseblink","diffuseshift","diffuseramp","glowblink","glowshift","glowramp"}
local root=Def.ActorFrame{Name="Root", InitCommand=cmd(glow,.1,.2,.3,.2),
    Def.ActorFrame{Name="DefaultParent", Def.Quad{Name="DefaultChild", InitCommand=cmd(xy,100,100;setsize,48,40;diffuse,.8,.5,.3,.75)}},
    Def.Quad{Name="TransparentChild", InitCommand=cmd(xy,200,100;setsize,48,40;diffusealpha,0)},
}
for index,mode in ipairs(modes) do
    root[#root+1]=Def.Quad{
        Name=mode,
        InitCommand=function(self) self:xy(100+(index-1)*100,260):setsize(48,40):diffuse(.2,.4,.6,.8) end,
        OnCommand=cmd(sleep,.4;queuecommand,"Begin";sleep,.5;queuecommand,"Begin";sleep,.5;queuecommand,"Other";sleep,.5;queuecommand,"Begin";sleep,.5;queuecommand,"Stop"),
        BeginCommand=function(self) self[mode](self); self:effectperiod(1.25):effectcolor1(.9,.2,.6,.35):effectcolor2(.1,.8,.3,.9) end,
        OtherCommand=cmd(rainbow), StopCommand=cmd(stopeffect),
    }
end
return root
