local instances = {}
local template = Def.Quad {
    Name = "Shared",
    InitCommand = function(self)
        instances[#instances + 1] = self
    end,
}

return Def.ActorFrame {
    Def.ActorFrame { Name = "Left", template },
    Def.ActorFrame { Name = "Right", template },
    OnCommand = function(self)
        assert(#instances == 2, "shared definition must run Init for each instance")
        local left = self:GetChild("Left"):GetChild("Shared")
        local right = self:GetChild("Right"):GetChild("Shared")
        assert(left ~= right, "shared definition must produce separate actors")
        assert(left:GetParent():GetName() == "Left")
        assert(right:GetParent():GetName() == "Right")
        left:x(23):diffusealpha(0.25)
        right:x(45):diffusealpha(0.75)
        assert(left:GetX() == 23 and right:GetX() == 45)
    end,
}
