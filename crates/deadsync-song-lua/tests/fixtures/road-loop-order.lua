-- ActorFrames own updates; observer frames retain their original sibling order.
local road
return Def.ActorFrame {
    Def.ActorFrame {
        OnCommand = function(frame)
            local quad = frame:GetChild("EarlyWitness")
            local self = quad
            local update = function(self) self:x(road:GetY()) end
            frame:SetUpdateFunction(function(_, delta) update(quad, delta) end)
        end,
        Def.Quad {
            Name = "EarlyWitness",
        }
    },
    Def.Quad {
        Name = "Road",
        InitCommand = function(self) road = self end,
        OnCommand = function(self) self:queuecommand("Loop") end,
        LoopCommand = function(self)
            self:linear((480/115)/2):y(-1024):sleep(0):y(0):queuecommand("Loop")
        end,
    },
    Def.ActorFrame {
        OnCommand = function(frame)
            local quad = frame:GetChild("Witness")
            local self = quad
            local update = function(self) self:x(road:GetY()) end
            frame:SetUpdateFunction(function(_, delta) update(quad, delta) end)
        end,
        Def.Quad {
            Name = "Witness",
        }
    },
}
