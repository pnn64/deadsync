local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
return Def.ActorFrame{OnCommand=function(self)
    options:BumpyPeriod(-0.66, 7)
    options:Drunk(0.25, 3)
    self:SetUpdateFunction(function(self)
        if GAMESTATE:GetSongBeat() < 0.5 then return end
        options:FromString('*1000 100% BumpPeriod, *2 99% completely_unknown')
        assert(math.abs(options:BumpyPeriod() + 0.66) < 0.000001)
        assert(options:Drunk() == 0.25)
        self:SetUpdateFunction(nil)
    end)
end}
