-- Immediate native properties, mixed with queued tweens and authored fields.
local actors = {}
local root = Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            for _, actor in ipairs(actors) do
                actor.tick = actor.tick + 1
                local v = actor.tick % 32 / 32
                actor:SetWidth(10+v):SetHeight(16+v)
                    :baserotationx(v):baserotationy(v*2):baserotationz(v*3)
                    :shadowlengthx(v):shadowlengthy(-v):shadowcolor(v,0.2,0.3,0.4)
                    :effectmagnitude(v,2,3):effectcolor1(v,0.5,0.7,1)
                    :effectcolor2(1,0.7,0.5,v):effecttiming(0.1,0.2,0.3,0.4,0.5)
                    :effect_hold_at_full(v):effectperiod(2)
                assert(actor:GetWidth() == 10+v and actor:GetHeight() == 16+v)
            end
        end)
    end,
}
for i = 1, 128 do
    root[#root + 1] = Def.Quad {
        Name = 'Vector' .. i,
        OnCommand = function(self)
            actors[#actors + 1] = self
            self.tick = 0
            self:xy(i % 16 * 30, math.floor(i / 16) * 40):queuecommand('Cycle')
        end,
        CycleCommand = function(self)
            self:linear(0.25):addx(2):sleep(0.125):queuecommand('Cycle')
        end,
    }
end
return root
