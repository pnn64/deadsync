local player = nil
prefix_globals = {}

mod_actions = {
    {1, "FixtureStart", true},
    {2, "FixtureEnd", true},
}

return Def.ActorFrame{
    InitCommand=function(self)
        prefix_globals.ease = {
            {4, 1, 0, 90, function(value) player:rotationz(value) end, "len", ease.outCirc},
            {5, 1, 0, 0.25, function(value) player:skewx(value) end, "len", ease.outExpo},
        }
    end,
    Def.ActorFrame{
        OnCommand=function(self)
            self:queuecommand("BindPlayer")
        end,
        BindPlayerCommand=function(self)
            player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        end,
    },
    Def.Quad{Name="OverlayA"},
    Def.Quad{Name="OverlayB"},
    Def.Quad{Name="OverlayC"},
}
