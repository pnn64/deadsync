return Def.ActorFrame {
    Name = "Root",
    Def.ActorFrame {
        Name = "FlatYParent",
        OnCommand = function(self) self:xy(170,39):basezoomx(0.8):basezoomy(0.8):zoomy(0) end,
        Def.Sprite {
            Name = "FlatY", Texture = "fit-rect.png",
            OnCommand = function(self) self:rotationz(-6) end,
        },
    },
    Def.ActorFrame {
        Name = "FlatXParent",
        OnCommand = function(self) self:xy(300,240):zoomx(1.334375) end,
        Def.Sprite {
            Name = "FlatX", Texture = "fit-rect.png",
            OnCommand = function(self) self:xy(45,-43):basezoomx(0):basezoomy(0.8):rotationz(85) end,
        },
    },
    Def.ActorFrame {
        Name = "ReflectedParent",
        OnCommand = function(self) self:xy(600,280):zoomx(0):zoomy(-0.7):rotationz(45):skewx(0.15) end,
        Def.Sprite {
            Name = "Reflected", Texture = "fit-rect.png",
            OnCommand = function(self) self:xy(30,-20):zoomx(0.8):zoomy(1.2):rotationz(-20):skewy(0.03) end,
        },
    },
}
