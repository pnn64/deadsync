return Def.ActorFrame {
    Name="First",
    Def.Quad {
        Name="FirstQuad",
        InitCommand=function(self) self:xy(160,240):zoomto(40,30):diffuse({1,0.3,0.2,1}) end,
    },
}
