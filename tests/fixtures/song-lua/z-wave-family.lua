local options, current = {}, {}
for player = 1, 2 do
    local state = GAMESTATE:GetPlayerState(player == 1 and PLAYER_1 or PLAYER_2)
    options[player] = state:GetPlayerOptions("ModsLevel_Song")
    current[player] = state:GetPlayerOptions("ModsLevel_Current")
end
local methods = {
    {"BounceZ", "BounceZOffset", "BounceZPeriod"},
    {"DigitalZ", "DigitalZOffset", "DigitalZPeriod"},
    {"DigitalZSteps", "TornadoZ", "TornadoZOffset"},
    {"TornadoZPeriod", "SawtoothZ", "SawtoothZPeriod"},
    {"Sawtooth", "SawtoothPeriod", "Tiny"},
}
local amounts = {
    {0.75, -16, 0.5}, {-0.5, 8, -0.25}, {2, 1.25, -32},
    {0.5, -0.75, 1.25}, {0.5, -0.5, 0.5},
}
local frame = Def.ActorFrame{
    OnCommand = function(self)
        for _, po in ipairs(options) do
            for group, names in ipairs(methods) do
                for axis, name in ipairs(names) do
                    po[name](po, amounts[group][axis], 0.5 + group)
                end
            end
        end
        local phase = 0
        self:SetUpdateFunction(function()
            local next_phase = math.floor(GAMESTATE:GetSongBeat())
            if next_phase ~= phase then
                phase = next_phase
                for _, po in ipairs(options) do
                    if phase == 1 then
                        po:FromString("*3 -25% BounceZ,*4 1600% BounceZOffset,*2 -25% BounceZPeriod," ..
                            "*3 75% DigitalZ,*4 -800% DigitalZOffset,*2 50% DigitalZPeriod," ..
                            "*2 -50% DigitalZSteps,*3 -50% TornadoZ,*4 3200% TornadoZOffset," ..
                            "*2 -25% TornadoZPeriod,*3 50% SawtoothZ,*2 -25% SawtoothZPeriod," ..
                            "*3 -75% Sawtooth,*2 25% SawtoothPeriod,*3 -50% Tiny")
                    elseif phase == 2 then
                        po:FromString("clearall")
                    elseif phase == 3 then
                        for group, names in ipairs(methods) do
                            for axis, name in ipairs(names) do po[name](po, amounts[group][axis], 9999) end
                        end
                        po:Reverse(1, 9999)
                    end
                end
            end
            for player, po in ipairs(current) do
                for group, names in ipairs(methods) do
                    self:GetChild("ZWaveCurrentP" .. player .. "G" .. group)
                        :x(po[names[1]](po)):y(po[names[2]](po)):z(po[names[3]](po))
                end
            end
        end)
    end,
}
for player = 1, 2 do
    for group = 1, 5 do
        frame[#frame + 1] = Def.Quad{Name = "ZWaveCurrentP" .. player .. "G" .. group}
    end
end
return frame
