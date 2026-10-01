local signal, stage = nil, 0
return Def.ActorFrame{
    Def.Actor{
        InitCommand=function(self) signal=self end,
        PushMessageCommand=function(self)
            self:decelerate(0.5):addx(1/3):sleep(0.25)
                :decelerate(0.5):addx(1/3)
        end,
        ResetMessageCommand=function(self)
            self:x(1):y(2):z(3):linear(0.5):x(0):y(0):z(0)
        end,
    },
    Def.Actor{
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                local beat=GAMESTATE:GetSongBeat()
                if stage==0 and beat>=1 then
                    MESSAGEMAN:Broadcast("Push")
                    stage=1
                end
                local options=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
                options:Invert(signal:GetX())
                options:Drunk(signal:GetY())
                options:Wave(signal:GetZ())
            end)
        end,
    },
}
