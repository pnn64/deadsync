-- Repeated publication of native uploads, with authored range and vertex edits.
local meshes, uploads, tick = {}, {}, 0
local root = Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            tick = tick + 1
            self:x(tick % 2)
            if tick % 60 == 0 then
                for _, mesh in ipairs(meshes) do
                    mesh:SetDrawState{Num = tick % 120 == 0 and 256 or 248}
                end
            end
            if tick % 120 == 0 then
                for i, mesh in ipairs(meshes) do
                    uploads[i][1][1][1] = tick % 240 == 0 and 0 or 1
                    mesh:SetVertices(uploads[i])
                end
            end
        end)
    end,
}
for i = 1, 32 do
    local vertices = {}
    for quad = 0, 63 do
        local x, y = quad % 8 * 4, math.floor(quad / 8) * 4
        for _, offset in ipairs{{0,0},{0,3},{3,3},{3,0}} do
            vertices[#vertices + 1] = {
                {x + offset[1], y + offset[2], 0},
                {i / 32, quad / 64, 0.5, 1},
                {offset[1] / 3, offset[2] / 3},
            }
        end
    end
    uploads[i] = vertices
    root[#root + 1] = Def.ActorMultiVertex {
        Name = 'Mesh' .. i,
        InitCommand = function(self)
            meshes[i] = self
            self:SetVertices(vertices):SetDrawState{Mode='DrawMode_Quads'}
        end,
        OnCommand = function(self)
            self:xy((i - 1) % 8 * 40, math.floor((i - 1) / 8) * 40)
        end,
    }
end
return root
