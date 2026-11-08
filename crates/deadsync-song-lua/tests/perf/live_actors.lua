-- Deterministic actor-heavy workload for the application-build benchmark.
-- Includes current/destination reads, per-vertex colors and recurring queues.
local actors = {}
local root = Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function(frame)
            local total = 0
            for _, actor in ipairs(actors) do total = total + actor:GetX() + actor:getaux() end
            frame:GetChild('GetterProbe'):x(total / #actors)
        end)
    end,
    Def.Quad { Name = 'GetterProbe', OnCommand = function(self) self:zoomto(1, 1) end },
    Def.ActorFrame {
        OnCommand = function(self)
            self:rotationz(15):spin():effectmagnitude(0, 0, 90)
                :linear(0.25):rotationz(45):sleep(0.125)
            self:SetUpdateFunction(function(frame)
                frame:GetChild('SpinProbe'):x(frame:GetRotationZ())
            end)
        end,
        Def.Quad { Name = 'SpinProbe', OnCommand = function(self) self:zoomto(1, 1) end },
    },
}
for i = 1, 256 do
    root[#root + 1] = Def.Quad {
        Name = 'Tween' .. i,
        OnCommand = function(self)
            actors[#actors + 1] = self
            self.sign = 1
            self:xy(i % 32 * 20, math.floor(i / 32) * 30):zoomto(8, 8)
            self:queuecommand('Cycle')
        end,
        CycleCommand = function(self)
            self.sign = -self.sign
            self:linear(0.25 + (i % 5) / 16)
                :addx(self.sign * 12):addy(self.sign * 5)
                :aux(i * self.sign):rotationz(self.sign * 30)
                :diffuse(0.4, 0.7, 1, self.sign > 0 and 0.8 or 0.3)
                :diffuseupperleft(1, 0.2, 0.3, 0.9)
                :glow(0.2, 0.1, 0.5, 0.3):queuecommand('Cycle')
        end,
    }
end
return root
