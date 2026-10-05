local function sprite(name, z, rx, ry)
    return Def.Sprite{
        Name = name, Texture = "fit-rect.png",
        InitCommand = function(self)
            self:xy(427, 240):z(z):rotationx(rx or 0):rotationy(ry or 0)
        end,
    }
end
return Def.ActorFrame{
    Name = "Camera",
    InitCommand = function(self) self:fov(120):vanishpoint(427, 240) end,
    sprite("Front", 246.52),
    sprite("Behind", 246.54),
    sprite("Tilted", 224.245, 55, 30),
    Def.Quad{
        Name = "Circle",
        InitCommand = function(self)
            self:setsize(2646, 2595):xy(427, 240)
            self:rotationx(48.8098327414963):rotationy(30.03682014861311):rotationz(83.7)
            self:zoom(0.4377301259288319)
        end,
    },
    Def.ActorFrame{
        Name = "Scaled",
        InitCommand = function(self) self:xy(427, 240):zoom(1.3754602518576638) end,
        Def.Quad{
            Name = "InheritedCircle",
            InitCommand = function(self)
                self:setsize(2646, 2595)
                self:rotationx(48.8098327414963):rotationy(30.03682014861311):rotationz(83.7)
                self:zoom(0.4377301259288319)
            end,
        },
    },
    Def.Quad{
        Name = "Spun",
        InitCommand = function(self)
            self:setsize(64, 32):xy(427, 240):z(224.245)
            self:rotationx(55):rotationy(30):rotationz(1.234567)
            self:spin():effectmagnitude(0, 0, 5.1)
        end,
    },
    Def.Sprite{
        Name = "Mirrored", Texture = "fit-rect.png",
        InitCommand = function(self)
            self:xy(320, 150):z(40):rotationx(15):rotationy(20):rotationz(30)
            self:zoomx(-0.8):zoomy(1.2):halign(0.25):valign(0.75)
        end,
    },
}
