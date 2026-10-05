return Def.ActorFrame{
    Name="Root", OnCommand=function(self) self:queuemessage("Paint") end,
    Def.ActorFrame{
        Name="Family",
        PaintMessageCommand=function(self) self:GetChild(""):diffuse(0,0,0,1) end,
        Def.Quad{Name="", InitCommand=cmd(xy,100,100;setsize,16,16)},
        Def.Quad{Name="Named", InitCommand=cmd(xy,200,100;setsize,16,16)},
    },
}
