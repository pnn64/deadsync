return Def.ActorFrame {
    Name = "Root",
    Def.ActorFrame {
        Name = "CurtainParent",
        InitCommand = function(self) self:xy(100,80):zoomx(1.334375) end,
        Def.Sprite {
            Name = "Curtain", Texture = "fit-rect.png",
            InitCommand = function(self) self:xy(30,-20):valign(0):skewx(-0.2) end,
        },
    },
    Def.ActorFrame {
        Name = "SkewedParent",
        InitCommand = function(self)
            self:xy(350,180):zoomx(1.3):zoomy(0.7):rotationz(20):skewx(0.15):skewy(-0.2)
        end,
        Def.Sprite {
            Name = "Skewed", Texture = "fit-rect.png",
            InitCommand = function(self)
                self:xy(30,-20):zoomx(0.8):zoomy(1.2):rotationz(30):skewx(-0.1):skewy(0.04)
            end,
        },
    },
    Def.ActorFrame {
        Name = "ReflectedParent",
        InitCommand = function(self) self:xy(600,280):zoomx(-1.5):zoomy(0.7):rotationz(45) end,
        Def.Sprite {
            Name = "Reflected", Texture = "fit-rect.png",
            InitCommand = function(self)
                self:xy(30,-20):zoomx(0.8):zoomy(1.2):rotationz(-20):skewx(0.12):skewy(0.03)
            end,
        },
    },
}
