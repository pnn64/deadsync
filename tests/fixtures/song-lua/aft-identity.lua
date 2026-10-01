local named, anonymous
return Def.ActorFrame{
    Def.ActorFrameTexture{
        Name="ActorLabel",
        InitCommand=function(self)
            named = self
            self:SetTextureName("TextureLabel"):SetWidth(64):SetHeight(64):Create()
        end,
        Def.Quad{ Name="NamedChild" },
    },
    Def.ActorFrameTexture{
        InitCommand=function(self)
            anonymous = self
            self:SetWidth(64):SetHeight(64):Create()
        end,
        Def.Quad{ Name="AnonymousChild" },
    },
    Def.Quad{
        Name="NamedLength",
        OnCommand=function(self) self:x(#named:GetName()) end,
    },
    Def.Quad{
        Name="AnonymousLength",
        OnCommand=function(self) self:x(#anonymous:GetName()) end,
    },
    Def.Sprite{
        Name="NamedTexture",
        OnCommand=function(self) self:SetTexture(named:GetTexture()) end,
    },
    Def.Sprite{
        Name="AnonymousTexture",
        OnCommand=function(self) self:SetTexture(anonymous:GetTexture()) end,
    },
}
