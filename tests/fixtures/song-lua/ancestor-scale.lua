return Def.ActorFrame {
    Name = "Root",
    Def.ActorFrame {
        Name = "RoadAncestor",
        InitCommand = function(self) self:zoomx(1.334375):zoomz(1.334375) end,
        Def.ActorFrame {
            Name = "RoadParent",
            InitCommand = function(self) self:xy(240,480):z(-896):zoomx(3):rotationx(90) end,
            Def.ActorFrame {
                Name = "RoadOffset",
                InitCommand = function(self) self:y(-400) end,
                Def.Sprite {
                    Name = "Road", Texture = "fit-rect.png",
                    InitCommand = function(self) self:y(1024) end,
                },
            },
        },
    },
    Def.ActorFrame {
        Name = "TiltedAncestor",
        InitCommand = function(self) self:xy(-40,15):zoomx(0.8):zoomy(1.5):zoomz(0.6) end,
        Def.ActorFrame {
            Name = "TiltedParent",
            InitCommand = function(self)
                self:xy(350,180):z(-20):zoomx(1.3):zoomy(0.7):zoomz(1.2)
                    :rotationx(12):rotationy(-18):rotationz(22)
            end,
            Def.Sprite {
                Name = "Tilted", Texture = "fit-rect.png",
                InitCommand = function(self)
                    self:xy(30,-20):z(50):zoomx(0.8):zoomy(1.2):halign(0):valign(1):skewx(-0.1)
                end,
            },
        },
    },
    Def.ActorFrame {
        Name = "ReflectedAncestor",
        InitCommand = function(self) self:xy(20,-10):zoomx(0.6):zoomy(-0.8):zoomz(1.3) end,
        Def.ActorFrame {
            Name = "ReflectedParent",
            InitCommand = function(self)
                self:xy(600,280):z(40):zoomx(-1.5):zoomy(0.7):zoomz(0.6)
                    :rotationx(-20):rotationy(35):rotationz(45)
            end,
            Def.Sprite {
                Name = "Reflected", Texture = "fit-rect.png",
                InitCommand = function(self) self:xy(30,-20):z(-50):zoomx(0.8):zoomy(1.2) end,
            },
        },
    },
}
