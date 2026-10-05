local cache
return Def.ActorFrame{
    Def.ActorFrame{
        Name="Cache",
        InitCommand=function(self) cache=self end,
        LoadActor("fit-rect.png")..{
            Name="HiddenA", InitCommand=function(self) self:visible(false) end,
        },
        LoadActor("Normal 2x6.png")..{
            Name="HiddenB", InitCommand=function(self) self:visible(false) end,
        },
    },
    Def.Sprite{
        Name="VisibleA",
        OnCommand=function(self)
            self:SetTexture(cache:GetChild("HiddenA"):GetTexture()):xy(100,100):diffusealpha(0.5)
        end,
    },
    Def.Sprite{
        Name="VisibleB",
        OnCommand=function(self)
            self:SetTexture(cache:GetChild("HiddenB"):GetTexture()):xy(100,100):diffusealpha(0.5)
        end,
    },
}
