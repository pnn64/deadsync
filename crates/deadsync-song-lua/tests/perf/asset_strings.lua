-- Repeated native asset setters exercise strings during actor startup.
local root = Def.ActorFrame{Name='AssetStrings'}
for i = 1, 16 do
    root[#root + 1] = Def.Sprite{
        Name='Sprite' .. i,
        Texture='finalization-a 2x6.png',
        InitCommand=function(self)
            for j = 1, 64 do self:SetTexture('finalization-a 2x6.png') end
            self:xy(i, -i)
        end,
    }
end
for i = 1, 8 do
    root[#root + 1] = Def.Sound{
        Name='Sound' .. i,
        File='asset-string.wav',
        InitCommand=function(self)
            for j = 1, 64 do self:Load('asset-string.wav') end
        end,
    }
end
return root
