return Def.ActorFrame{
    Def.Actor{
        OnCommand=function(self) self:sleep(1):queuecommand("Camera") end,
        CameraCommand=function() SCREENMAN:GetTopScreen():fov(60) end,
    },
    Def.Sprite{
        Name="Extensionless",
        Texture="Normal",
        OnCommand=function(self)
            self:xy(SCREEN_CENTER_X,SCREEN_CENTER_Y):z(-100)
                :zoomtoheight(120):zoomtowidth(180):diffusealpha(0)
                :sleep(2):diffusealpha(1)
        end,
    },
    Def.Quad{
        Name="BehindCamera",
        OnCommand=function(self)
            self:xy(10105,400):z(20000):zoomto(40,30):zoom(0)
                :diffusealpha(0):sleep(2):diffusealpha(1)
        end,
    },
}
