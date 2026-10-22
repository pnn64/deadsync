-- Timer, music and beat clocks, including idle actors and effect changes.
local actors = {}
local tick = 0
local root = Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function(frame)
            tick = tick + 1
            for i, actor in ipairs(actors) do
                if tick % 64 == 1 then
                    actor:bob():effectperiod(0.5):effectmagnitude(0, 4, 0)
                elseif tick % 64 == 17 then
                    actor:pulse():effectmagnitude(0.8, 1.2, 0)
                elseif tick % 64 == 33 then
                    actor:diffuseblink()
                elseif tick % 64 == 49 then
                    actor:stopeffect()
                end
                actor:x((i % 16) * 32 + actor:GetSecsIntoEffect())
                    :z(actor:GetEffectDelta())
            end
            frame:x(tick % 16)
        end)
    end,
}
for i = 1, 256 do
    root[#root + 1] = Def.Quad {
        Name = 'Clock' .. i,
        OnCommand = function(self)
            actors[#actors + 1] = self
            self:xy(i % 16 * 32, math.floor(i / 16) * 32):zoomto(16, 16)
            if i % 3 == 1 then self:effectclock('beat')
            elseif i % 3 == 2 then self:effectclock('music') end
        end,
    }
end
return root
