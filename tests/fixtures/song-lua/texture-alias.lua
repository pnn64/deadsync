local source, original, first, second
return Def.ActorFrame{
    LoadActor("fit-rect.png")..{
        Name="Source",
        InitCommand=function(self) source=self; self:visible(false) end,
        OnCommand=function(self)
            original=self:GetTexture()
            assert(original:GetSourceWidth()==64 and original:GetSourceHeight()==32)
            assert(original:GetTextureWidth()==64 and original:GetTextureHeight()==32)
            self:Load("Normal 2x6.png")
            local sheet=self:GetTexture()
            assert(sheet:GetSourceWidth()==128 and sheet:GetSourceHeight()==384)
            assert(sheet:GetTextureWidth()==128 and sheet:GetTextureHeight()==512)
            assert(sheet:GetNumFrames()==12)
            assert(self:GetWidth()==64 and self:GetHeight()==64)
            assert(original:GetSourceWidth()==64 and original:GetSourceHeight()==32)
            assert(original:GetPath():match("fit%-rect%.png$"))
        end,
    },
    Def.Sprite{
        Name="First",
        OnCommand=function(self)
            first=self
            assert(self:GetTexture()==nil)
            self:SetTexture(original):xy(100,100)
            assert(self:GetWidth()==64 and self:GetHeight()==32)
            self:SetTexture(self:GetTexture())
            assert(self:GetWidth()==64 and self:GetHeight()==32)
        end,
    },
    Def.Sprite{
        Name="Second",
        OnCommand=function(self)
            second=self
            self:SetTexture(first:GetTexture()):xy(200,100)
            assert(self:GetWidth()==64 and self:GetHeight()==32)
        end,
    },
    Def.ActorFrame{
        Name="TextureDriver",
        OnCommand=function(self)
            local fired=false
            self:SetUpdateFunction(function()
                if not fired and GAMESTATE:GetCurMusicSeconds()>=0.1 then
                    fired=true
                    first:SetTexture(source:GetTexture())
                    assert(first:GetWidth()==64 and first:GetHeight()==64)
                    assert(second:GetTexture():GetPath():match("fit%-rect%.png$"))
                    assert(second:GetWidth()==64 and second:GetHeight()==32)
                end
            end)
        end,
    },
}
