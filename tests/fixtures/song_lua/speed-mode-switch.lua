local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            options:XMod(2, 1000)
            if beat >= 1 and beat < 2 then options:CMod(300, 1000) end
            if beat >= 3 and beat < 4 then options:MMod(600, 1000) end
        end)
    end,
}
