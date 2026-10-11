local options = {
    GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song"),
    GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions("ModsLevel_Song"),
}
return Def.ActorFrame{
    OnCommand = function(self)
        for _, po in ipairs(options) do
            po:Wave(0.75, 9999)
            po:WavePeriod(0.5, 9999)
        end
        local phase = 0
        self:SetUpdateFunction(function()
            local next_phase = math.floor(GAMESTATE:GetSongBeat())
            if next_phase == phase then return end
            phase = next_phase
            for _, po in ipairs(options) do
                if phase == 1 then
                    po:FromString("*9999 -50% Wave, *9999 -25% WavePeriod")
                elseif phase == 2 then
                    po:FromString("clearall")
                elseif phase == 3 then
                    po:Wave(-0.75, 9999)
                    po:WavePeriod(1.25, 9999)
                end
            end
        end)
    end,
}
