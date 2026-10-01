local position_actor
local sent=false
return Def.ActorFrame{
    Def.Actor{
        InitCommand=function(self) position_actor=self end,
        StepsMessageCommand=function(self)
            self:sleep(0):addx(0.125):sleep(0.125):addx(0.125):sleep(0.125):addx(0.125):sleep(0.125):addx(0.125)
        end,
    },
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            local position=GAMESTATE:GetSongPosition()
            local options=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song")
            if not sent and position:GetSongBeat() >= 2 then
                MESSAGEMAN:Broadcast("Steps")
                sent=true
            end
            options:Invert(position:GetSongBeat()/10)
            options:Drunk(position:GetFreeze() and 1 or 0)
            options:Wave(position:GetDelay() and 1 or 0)
            options:XMod(position:GetCurBPS())
            options:Tornado(position_actor:GetX())
        end)
    end,
}
