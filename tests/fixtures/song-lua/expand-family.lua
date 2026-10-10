local options, current = {}, {}
for player = 1, 2 do
    local state = GAMESTATE:GetPlayerState(player == 1 and PLAYER_1 or PLAYER_2)
    options[player] = state:GetPlayerOptions("ModsLevel_Song")
    current[player] = state:GetPlayerOptions("ModsLevel_Current")
end
local frame = Def.ActorFrame{
    OnCommand = function(self)
        for player, po in ipairs(options) do
            po:Expand(0.75, 1.25)
            po:ExpandPeriod(player == 1 and 0.5 or -0.5, 0.5)
            po:WavePeriod(0.25, 2)
        end
        local phase = 0
        self:SetUpdateFunction(function()
            local next_phase = math.floor(GAMESTATE:GetSongBeat())
            if next_phase ~= phase then
                phase = next_phase
                for player, po in ipairs(options) do
                    if phase == 1 then
                        po:FromString("*2 -150% Expand,*3 -125% ExpandPeriod,*2 -50% WavePeriod")
                    elseif phase == 2 then
                        po:FromString("clearall")
                    elseif phase == 3 then
                        po:Expand(-0.5, 9999)
                        po:ExpandPeriod(player == 1 and -1 or -1.5, 9999)
                        po:WavePeriod(0.75, 9999)
                    end
                end
            end
            for player, po in ipairs(current) do
                self:GetChild("ExpandCurrentP" .. player)
                    :x(po:Expand()):y(po:ExpandPeriod()):z(po:WavePeriod())
            end
        end)
    end,
}
for player = 1, 2 do frame[#frame + 1] = Def.Quad{Name = "ExpandCurrentP" .. player} end
return frame
