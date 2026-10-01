return Def.ActorFrame{
    Name = "Root",
    Def.Quad{
        Name = "QueuedHide",
        OnCommand = function(self) self:queuecommand("Hide") end,
        HideCommand = function(self) self:visible(false) end,
    },
    Def.Quad{
        Name = "QueuedBuilder",
        OnCommand = cmd(queuecommand, "Hide"),
        HideCommand = function(self) self:visible(false) end,
    },
    Def.Quad{
        Name = "QueuedShow",
        OnCommand = function(self) self:visible(false):queuecommand("Show") end,
        ShowCommand = function(self) self:visible(true) end,
    },
    Def.Quad{
        Name = "DelayedRestore",
        OnCommand = function(self) self:queuecommand("Hide"):sleep(0.1):queuecommand("Show") end,
        HideCommand = function(self) self:visible(false) end,
        ShowCommand = function(self) self:visible(true) end,
    },
    Def.Quad{
        Name = "ImmediateHide",
        OnCommand = function(self) self:visible(false) end,
    },
}
