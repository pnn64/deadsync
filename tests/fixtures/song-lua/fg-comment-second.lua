return Def.ActorFrame {
    Name="Second",
    Def.Quad {
        Name="SecondQuad",
        InitCommand=function(self) self:xy(480,240):zoomto(40,30):diffuse({0.2,0.8,1,1}) end,
    },
}
