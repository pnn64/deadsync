local options, current = {}, {}
for player = 1, 2 do
    local state = GAMESTATE:GetPlayerState(player == 1 and PLAYER_1 or PLAYER_2)
    options[player] = state:GetPlayerOptions("ModsLevel_Song")
    current[player] = state:GetPlayerOptions("ModsLevel_Current")
end
local frame = Def.ActorFrame{
    OnCommand = function(self)
        for player, po in ipairs(options) do
            po:NoAttack(player == 1 and 0.75 or -0.75, 1.25)
            po:RandAttack(player == 1 and -0.5 or 0.5, 0.5)
        end
        local phase = 0
        self:SetUpdateFunction(function()
            local next_phase = math.floor(GAMESTATE:GetSongBeat())
            if next_phase ~= phase then
                phase = next_phase
                for player, po in ipairs(options) do
                    if phase == 1 then
                        po:FromString("*2 -150% NoAttacks,*3 -125% RandomAttacks")
                    elseif phase == 2 then
                        po:FromString("clearall")
                    elseif phase == 3 then
                        po:NoAttack(player == 1 and -0.5 or 0, 9999)
                        po:RandAttack(player == 1 and -1 or 1.5, 9999)
                    end
                end
            end
            for player, po in ipairs(current) do
                self:GetChild("AttackCurrentP" .. player)
                    :x(po:NoAttack()):y(po:RandAttack()):z(po:GetStepAttacks())
            end
        end)
    end,
}
for player = 1, 2 do frame[#frame + 1] = Def.Quad{Name = "AttackCurrentP" .. player} end
return frame
