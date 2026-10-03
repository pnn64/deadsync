local road
return Def.ActorFrame {
    Def.Quad {
        Name = "EarlyWitness",
        OnCommand = function(self)
            self:SetUpdateFunction(function(self) self:x(road:GetY()) end)
        end,
    },
    Def.Quad {
        Name = "Road",
        InitCommand = function(self) road = self end,
        OnCommand = function(self) self:queuecommand("Loop") end,
        LoopCommand = function(self)
            self:linear((480/115)/2):y(-1024):sleep(0):y(0):queuecommand("Loop")
        end,
    },
    Def.Quad {
        Name = "Witness",
        OnCommand = function(self)
            self:SetUpdateFunction(function(self) self:x(road:GetY()) end)
        end,
    },
}
