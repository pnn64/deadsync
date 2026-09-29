local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
assert(loadstring("return 5.0 .. ' centered'")() == '5 centered')

return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            options:Tiny(beat / 100, 9999)
            options:Flip(beat >= 140 and -0.25 or 0, 9999)
        end)
    end,
}
