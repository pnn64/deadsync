return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self) self:sleep(0.1):queuecommand('Set') end,
        SetCommand=function(self)
            for _, pn in ipairs(GAMESTATE:GetEnabledPlayers()) do
                GAMESTATE:GetPlayerState(pn):GetPlayerOptions('ModsLevel_Song'):Drunk(1, 100)
            end
            self:sleep(0.9):queuecommand('Freeze')
        end,
        FreezeCommand=function()
            for _, pn in ipairs(GAMESTATE:GetEnabledPlayers()) do
                GAMESTATE:GetPlayerState(pn):GetPlayerOptions('ModsLevel_Song'):Drunk(0, 0)
            end
        end,
    },
}
