local options = {
    GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song"),
    GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions("ModsLevel_Song"),
}
return Def.ActorFrame{
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            options[1]:ConfusionYOffset((beat - 0.5) * math.pi, 9999)
            options[2]:ConfusionYOffset((0.25 - beat) * 2 * math.pi, 9999)
        end)
    end,
}
