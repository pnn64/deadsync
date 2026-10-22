-- A later screen remains loaded behind its depth mask until the door opens.
return Def.ActorFrame{
    OnCommand=function(self) self:ztest(1):fov(150):sleep(1000) end,
    Def.Quad{
        OnCommand=function(self)
            self:blend('BlendMode_NoEffect'):zwrite(1)
            self:xy(SCREEN_CENTER_X, SCREEN_CENTER_Y):zoomto(SCREEN_WIDTH, SCREEN_HEIGHT)
            self:sleep(3):zoom(0)
        end,
    },
    Def.Quad{
        OnCommand=function(self)
            self:xy(SCREEN_CENTER_X, SCREEN_CENTER_Y):zoomto(SCREEN_WIDTH, SCREEN_HEIGHT)
            self:diffuse({1,0,0,1})
        end,
    },
}
