local player = nil
prefix_globals = {}

return Def.ActorFrame{
    InitCommand=function(self)
        prefix_globals.ease = {
            {26.5, 1.5, 0, -200, "tiny", "len", ease.linear, 1},
            {26.5, 1.5, 0, 50, "flip", "len", ease.linear, 1},
            {28, 0.1, 0, 100, "dark", "len", ease.linear, 1},
            {166, 0.125, 0, 3, "skewx", "len", ease.outQuad, 1},
            {182, 0.125, 0, -3, "skewx", "len", ease.outQuad, 1},
            {189, 1, 0, 20, function(value) player:rotationx(value) end, "len", ease.outQuad},
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
}
