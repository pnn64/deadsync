return Def.ActorFrame{
    OnCommand=function(self) self:fov(35) end,
    Def.Quad{
        Name="BackgroundDepth",
        OnCommand=function(self)
            self:xy(SCREEN_CENTER_X,SCREEN_CENTER_Y):z(-200):zoomto(64,64)
        end,
    },
}
