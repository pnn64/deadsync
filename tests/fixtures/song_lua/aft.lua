local capture = nil

return Def.ActorFrame{
    OnCommand=function(self)
        local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        if player then
            player:visible(false)
        end
    end,
    Def.ActorFrameTexture{
        Name="CaptureAFT",
        InitCommand=function(self)
            capture = self
            self:SetTextureName("FixtureAFT")
            self:SetWidth(640)
            self:SetHeight(480)
            self:Create()
        end,
        Def.Quad{
            Name="BGQuad",
            InitCommand=function(self)
                self:FullScreen():diffuse(0, 0, 0, 1)
            end,
        },
        Def.ActorProxy{
            Name="ProxyOverlay",
            OnCommand=function(self)
                local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
                local field = player and player:GetChild("NoteField")
                self:SetTarget(field):visible(true)
            end,
        },
    },
    Def.Sprite{
        Name="AFTSpriteR",
        OnCommand=function(self)
            self:SetTexture(capture:GetTexture()):diffuse(1, 0, 0, 1):blend("add")
        end,
    },
    Def.Sprite{
        Name="AFTSpriteG",
        OnCommand=function(self)
            self:SetTexture(capture:GetTexture()):diffuse(0, 1, 0, 1):blend("add")
        end,
    },
    Def.Sprite{
        Name="AFTSpriteB",
        OnCommand=function(self)
            self:SetTexture(capture:GetTexture()):diffuse(0, 0, 1, 1):blend("add")
        end,
    },
}
