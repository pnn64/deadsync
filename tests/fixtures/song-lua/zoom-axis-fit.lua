return Def.ActorFrame{
    Name="Root",
    Def.Quad{
        Name="Sized",
        InitCommand=function(self) self:setsize(10,20):xy(100,100):basezoomx(2):basezoomy(.5) end,
        OnCommand=function(self)
            self:zoomtowidth(30):zoomtoheight(40):linear(.5):zoomtowidth(50):zoomtoheight(10)
                :linear(.5):zoomx(1):zoomy(1)
        end,
    },
    Def.Sprite{
        Name="Image", Texture="fit-rect.png",
        InitCommand=function(self) self:xy(300,200) end,
        OnCommand=function(self)
            self:zoomtowidth(120):zoomtoheight(30):linear(.5):zoomtowidth(-60):zoomtoheight(90)
                :linear(.5):zoomx(1):zoomy(1)
        end,
    },
    Def.Quad{
        Name="Unsized",
        OnCommand=function(self) self:xy(427,240):zoomtowidth(854):zoomtoheight(480):linear(1):zoomx(1):zoomy(1) end,
    },
}
