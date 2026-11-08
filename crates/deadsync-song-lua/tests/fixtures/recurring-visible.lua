local before, after
return Def.ActorFrame {
    Def.Quad {
        Name = "Before",
        InitCommand = function(self) before = self end,
    },
    Def.Quad {
        Name = "Driver",
        InitCommand = function(self) self:queuecommand("Update") end,
        OnCommand = function(self) self:visible(false) end,
        UpdateCommand = function(self)
            before:visible(false):visible(true)
            after:visible(false):visible(true)
            self:sleep(0.02):queuecommand("Update")
        end,
    },
    Def.Quad {
        Name = "After",
        InitCommand = function(self) after = self end,
    },
}
