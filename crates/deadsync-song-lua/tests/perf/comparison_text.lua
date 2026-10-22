-- Native texture tags and player function-ease probes share transient names.
local texture, target
local linear = function(t, b, c, d) return b + c * t / d end
prefix_globals = {}
return Def.ActorFrame{
    Name='ComparisonText',
    InitCommand=function(self)
        prefix_globals.ease = {}
        for i = 1, 32 do
            prefix_globals.ease[i] = {i / 32, 0.125, 0, i,
                function(value) if target then target:x(value) end end, 'len', linear}
        end
    end,
    OnCommand=function(self) target = SCREENMAN:GetTopScreen():GetChild('PlayerP1') end,
    Def.Sprite{
        Name='TextureSource', Texture='finalization-a 2x6.png',
        InitCommand=function(self) texture = self:GetTexture() end,
    },
    Def.ActorMultiVertex{
        Name='Mesh',
        InitCommand=function(self)
            self:SetDrawState{Mode='DrawMode_Triangles'}:SetVertices{
                {{0,0,0},{1,1,1,1},{0,0}},
                {{1,0,0},{1,1,1,1},{1,0}},
                {{0,1,0},{1,1,1,1},{0,1}},
            }
            assert(texture)
            for i = 1, 128 do self:SetTexture(texture) end
        end,
    },
}
