local options = {
    GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song"),
    GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions("ModsLevel_Song"),
}
local current = {
    GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Current"),
    GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions("ModsLevel_Current"),
}
local methods = {
    {"Beat", "BeatY", "BeatZ"},
    {"BeatOffset", "BeatYOffset", "BeatZOffset"},
    {"BeatMult", "BeatYMult", "BeatZMult"},
    {"BeatPeriod", "BeatYPeriod", "BeatZPeriod"},
}
local amounts = {
    {0.75, -0.5, 1.25}, {0.1, -0.15, 1.3},
    {0.5, -0.25, 1.5}, {0.5, -0.75, 1.25},
}
local frame = Def.ActorFrame{
    OnCommand = function(self)
        for _, po in ipairs(options) do
            for group, names in ipairs(methods) do
                for axis, name in ipairs(names) do po[name](po, amounts[group][axis], 0.5 + group) end
            end
        end
        local phase = 0
        self:SetUpdateFunction(function()
            local next_phase = math.floor(GAMESTATE:GetSongBeat())
            if next_phase ~= phase then
                phase = next_phase
                for _, po in ipairs(options) do
                    if phase == 1 then
                        po:FromString("*3 -25% Beat,*4 50% BeatY,*5 -75% BeatZ," ..
                            "*2 -10% BeatOffset,*2 15% BeatYOffset,*2 -130% BeatZOffset," ..
                            "*2 -50% BeatMult,*2 25% BeatYMult,*2 -150% BeatZMult," ..
                            "*2 -50% BeatPeriod,*2 -25% BeatYPeriod,*2 25% BeatZPeriod")
                    elseif phase == 2 then
                        po:FromString("clearall")
                    elseif phase == 3 then
                        for group, names in ipairs(methods) do
                            for axis, name in ipairs(names) do po[name](po, amounts[group][axis], 9999) end
                        end
                        po:Tiny(0.5, 9999)
                        po:Reverse(1, 9999)
                    end
                end
            end
            for player, po in ipairs(current) do
                for group, names in ipairs(methods) do
                    self:GetChild("BeatCurrentP" .. player .. "G" .. group)
                        :x(po[names[1]](po)):y(po[names[2]](po)):z(po[names[3]](po))
                end
            end
        end)
    end,
}
for player = 1, 2 do
    for group = 1, 4 do
        frame[#frame + 1] = Def.Quad{Name = "BeatCurrentP" .. player .. "G" .. group}
    end
end
return frame
