local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
mods = {
    {0, 4, 'reset', 'len'},
    {0, 1, 'pulse', 'len'},
}
mods_ease = {
    {1, 2, 0, 100, 'tipsy', 'len', function(t, b, c, d, a, p)
        return b + c * (t / d) ^ a + p
    end, 1, nil, 2, 5},
}

return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            for _, mod in ipairs(mods) do
                if beat >= mod[1] and beat <= mod[1] + mod[2] then
                    options:Drunk(mod[3] == 'pulse' and 0.1 or 0, 1000)
                end
            end
            for _, mod in ipairs(mods_ease) do
                if beat >= mod[1] and beat <= mod[1] + mod[2] then
                    options:Tipsy(mod[7](beat-mod[1], mod[3], mod[4]-mod[3], mod[2], mod[10], mod[11]) / 100, 1000)
                end
            end
        end)
    end,
}
