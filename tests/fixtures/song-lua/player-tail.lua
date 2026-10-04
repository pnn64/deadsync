local action = 1
local p2
local actions = {
    {1, function() p2:x(503.059375) end},
    {2, function() p2:x(640.5) end},
    {3, function() p2:addx(133.4375) end},
}
return Def.ActorFrame {
    OnCommand = function(self)
        p2 = SCREENMAN:GetTopScreen():GetChild("PlayerP2")
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            while action <= #actions and beat >= actions[action][1] do
                actions[action][2]()
                action = action + 1
            end
        end)
    end,
}
