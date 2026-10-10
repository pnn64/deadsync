local options = {
    GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song"),
    GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions("ModsLevel_Song"),
}
local current = {
    GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Current"),
    GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions("ModsLevel_Current"),
}
return Def.ActorFrame{
    OnCommand = function(self)
        for _, po in ipairs(options) do
            po:AttenuateX(0.75, 2)
            po:AttenuateY(-0.5, 0.75)
            po:AttenuateZ(1.25, 1.5)
        end
        local phase = 0
        self:SetUpdateFunction(function()
            local next_phase = math.floor(GAMESTATE:GetSongBeat())
            if next_phase ~= phase then
                phase = next_phase
                for _, po in ipairs(options) do
                    if phase == 1 then
                        po:FromString("*3 -25% AttenuateX,*4 50% AttenuateY,*5 -75% AttenuateZ")
                    elseif phase == 2 then
                        po:FromString("clearall")
                    elseif phase == 3 then
                        po:AttenuateX(-0.75, 9999)
                        po:AttenuateY(0.5, 9999)
                        po:AttenuateZ(-1.25, 9999)
                    end
                end
            end
            for player, po in ipairs(current) do
                self:GetChild("AttenuateCurrentP" .. player)
                    :x(po:AttenuateX()):y(po:AttenuateY()):z(po:AttenuateZ())
            end
        end)
    end,
    Def.Quad{Name = "AttenuateCurrentP1"},
    Def.Quad{Name = "AttenuateCurrentP2"},
}
