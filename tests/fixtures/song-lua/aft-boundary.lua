local function quad(name, x, y, w, h)
    return Def.Quad{
        Name = name,
        InitCommand = function(self) self:xy(x, y):zoomto(w, h) end,
    }
end
return Def.ActorFrame{
    Name = "Outer",
    InitCommand = function(self)
        self:xy(427, 240):rotationz(17):zoom(0.8):fov(60):vanishpoint(427, 240)
    end,
    Def.ActorFrameTexture{
        Name = "ScreenTarget",
        InitCommand = function(self)
            self:xy(80, 40):rotationz(31):zoom(0.5)
            self:SetWidth(854):SetHeight(480):SetTextureName("ScreenTarget"):Create()
        end,
        quad("ScreenBackground", 427, 240, 854, 480),
        Def.ActorFrameTexture{
            Name = "SmallTarget",
            InitCommand = function(self)
                self:xy(90, 50):rotationz(13)
                self:SetWidth(320):SetHeight(200):SetTextureName("SmallTarget"):Create()
            end,
            quad("SmallBackground", 160, 100, 320, 200),
            Def.ActorFrame{
                Name = "InnerCamera",
                InitCommand = function(self) self:fov(45):vanishpoint(427, 240) end,
                quad("InnerPerspective", 427, 240, 100, 60),
            },
        },
        quad("AfterSmall", 120, 80, 40, 20),
    },
    quad("AfterTarget", 50, 30, 80, 40),
    Def.ActorProxy{
        Name = "ComboCopy",
        OnCommand = function(self)
            local combo = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("Combo")
            self:SetTarget(combo)
            self:visible(combo:GetVisible())
        end,
    },
    quad("Hidden", 0, 0, 10, 10)..{
        OnCommand = function(self) self:visible(false) end,
    },
    Def.Quad{
        Name = "VisibilityProbe",
        OnCommand = function(self)
            assert(self:GetVisible() == true)
            assert(self:GetParent():GetChild("Hidden"):GetVisible() == false)
            self:visible(false)
            assert(self:GetVisible() == false)
            self:visible(true)
            assert(self:GetVisible() == true)
            self:zoom(0)
            assert(self:GetVisible() == true)
        end,
    },
}
