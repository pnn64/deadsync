local options, current = {}, {}
for player = 1, 2 do
    local state = GAMESTATE:GetPlayerState(player == 1 and PLAYER_1 or PLAYER_2)
    options[player] = state:GetPlayerOptions("ModsLevel_Song")
    current[player] = state:GetPlayerOptions("ModsLevel_Current")
end
local frame = Def.ActorFrame{
    OnCommand = function(self)
        for _, po in ipairs(options) do
            po:ShrinkLinear(0.75, 1.25)
            po:ShrinkMult(-0.125, 0.5)
            po:Tiny(0.5, 2)
        end
        local phase = 0
        self:SetUpdateFunction(function()
            local next_phase = math.floor(GAMESTATE:GetSongBeat())
            if next_phase ~= phase then
                phase = next_phase
                for _, po in ipairs(options) do
                    if phase == 1 then
                        po:FromString("*2 -150% ShrinkLinear,*3 50% ShrinkMult,*2 -50% Tiny")
                    elseif phase == 2 then
                        po:FromString("clearall")
                    elseif phase == 3 then
                        po:ShrinkLinear(0.75, 9999)
                        po:ShrinkMult(-0.125, 9999)
                        po:Tiny(0.5, 9999)
                        po:Reverse(1, 9999)
                    end
                end
            end
            for player, po in ipairs(current) do
                self:GetChild("ShrinkCurrentP" .. player)
                    :x(po:ShrinkLinear()):y(po:ShrinkMult()):z(po:Tiny())
            end
        end)
    end,
}
for player = 1, 2 do frame[#frame + 1] = Def.Quad{Name = "ShrinkCurrentP" .. player} end
return frame
