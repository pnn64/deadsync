local function metric(name)
    assert(THEME:HasMetric("Player", name))
    return THEME:GetMetric("Player", name)
end
return Def.ActorFrame {
    Def.Quad {
        Name = "Standard",
        InitCommand = function(self) self:zoomto(16, 16):x(100):y(112) end,
        OnCommand = function(self)
            self:addy(metric("ReceptorArrowsYStandard") + 125)
        end,
    },
    Def.Quad {
        Name = "Reverse",
        InitCommand = function(self) self:zoomto(16, 16):x(200):y(385) end,
        OnCommand = function(self)
            self:addy(metric("ReceptorArrowsYReverse") - 145)
        end,
    },
}
