local options, current = {}, {}
for index, player in ipairs({PLAYER_1, PLAYER_2}) do
    local state = GAMESTATE:GetPlayerState(player)
    options[index] = state:GetPlayerOptions("ModsLevel_Song")
    current[index] = state:GetPlayerOptions("ModsLevel_Current")
end
local methods = {
    {"ConfusionX", "ConfusionY", "ConfusionYOffset"},
    {"ConfusionXOffset", "Roll", "Twirl"},
}
local amounts = {{0.75, -0.5, 0.4}, {-0.2, 1.25, -0.75}}
local frame = Def.ActorFrame{
    OnCommand = function(self)
        for _, po in ipairs(options) do
            for group, names in ipairs(methods) do
                for axis, name in ipairs(names) do po[name](po, amounts[group][axis], 1 + axis * 0.25) end
            end
        end
        local phase = 0
        self:SetUpdateFunction(function()
            local next_phase = math.floor(GAMESTATE:GetSongBeat())
            if next_phase ~= phase then
                phase = next_phase
                for _, po in ipairs(options) do
                    if phase == 1 then
                        po:FromString("*2 -25% ConfusionX,*2 75% ConfusionY,*2 -40% ConfusionYOffset," ..
                            "*2 20% ConfusionXOffset,*2 -125% Roll,*2 75% Twirl")
                    elseif phase == 2 then
                        po:FromString("clearall")
                    elseif phase == 3 then
                        for group, names in ipairs(methods) do
                            for axis, name in ipairs(names) do po[name](po, amounts[group][axis], 9999) end
                        end
                        po:Reverse(1, 9999)
                        po:Tiny(0.5, 9999)
                    end
                end
            end
            for player, po in ipairs(current) do
                for group, names in ipairs(methods) do
                    self:GetChild("ConfusionCurrentP" .. player .. "G" .. group)
                        :x(po[names[1]](po)):y(po[names[2]](po)):z(po[names[3]](po))
                end
            end
        end)
    end,
}
for player = 1, 2 do
    for group = 1, 2 do
        frame[#frame + 1] = Def.Quad{Name = "ConfusionCurrentP" .. player .. "G" .. group}
    end
end
return frame
