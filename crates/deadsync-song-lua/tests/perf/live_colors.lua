-- Authored color writes, destination reads, and recurring tween queues.
local actors = {}
local tick = 0
local root = Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function(frame)
            tick = tick + 1
            for i, actor in ipairs(actors) do
                local phase = ((tick + i) % 16) / 16
                actor:diffuse(phase, 0.5, 1, 0.75):diffusealpha(0.5)
                    :diffusecolor(0.25, phase, 0.75, 1)
                    :diffuseupperleft(1, phase, 0.5, 0.75)
                    :diffuselowerright(phase, 0.5, 1, 0.5)
                    :glow(0.5, phase, 1, 0.25)
                local diffuse, glow = actor:GetDiffuse(), actor:GetGlow()
                assert(diffuse[1] == 0.25 and diffuse[4] == 0.5)
                assert(glow[2] == phase and glow[4] == 0.25)
                diffuse[1], glow[2] = 99, 99
                assert(actor:GetDiffuseAlpha() == 0.5)
            end
            -- The runner's startup smoke snapshot tracks positions. Tie its
            -- motion probe to an authored color getter; full fingerprints also
            -- compare every actor's colors outside the timed update.
            if actors[1] then frame:x(actors[1]:GetGlow()[2]) end
        end)
    end,
}
for i = 1, 128 do
    root[#root + 1] = Def.Quad {
        Name = 'Color' .. i,
        OnCommand = function(self)
            actors[#actors + 1] = self
            self:xy(i % 16 * 32, math.floor(i / 16) * 32):zoomto(16, 16)
                :queuecommand('Cycle')
        end,
        CycleCommand = function(self)
            self:linear(0.25 + (i % 4) / 16):queuecommand('Cycle')
        end,
    }
end
return root
