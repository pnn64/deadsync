return Def.ActorFrame{
    OnCommand=function(self)
        local p1 = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
        local p2 = GAMESTATE:GetPlayerState(PLAYER_2):GetPlayerOptions("ModsLevel_Song")
        p1:XMod(4)
        p2:CMod(300)
        p2:ScrollSpeed(2, 0.25)
        p2:CMod(480)
        local _, speed = p1:XMod()
        assert(speed == 1)
        local _, x_speed = p2:ScrollSpeed()
        local _, c_speed = p2:CMod()
        assert(x_speed == 0.25 and c_speed == 1)
    end,
}
