local player, target
local fired = 0
local alpha = .4
return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            if beat >= .5 then
                player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            end
            if fired == 0 and beat >= 1 then
                MESSAGEMAN:Broadcast("Late")
                fired = 1
            end
            if fired == 1 and beat >= 2 then
                alpha = .8
                MESSAGEMAN:Broadcast("Late")
                fired = 2
            end
        end)
    end,
    Def.Quad{
        Name="Target",
        InitCommand=function(self) target = self end,
    },
    Def.Quad{
        Name="Receiver",
        LateMessageCommand=function(self)
            player:skewx(.25)
            self:diffusealpha(alpha)
            target:rotationz(15)
        end,
        UnsentMessageCommand=function(self)
            player:skewx(.5)
            self:zoomx(.6)
            target:rotationz(30)
        end,
        UnsentAuxMessageCommand=function(self)
            player:GetName()
            self:aux(.7)
            target:aux(.8)
            player:aux(.9)
        end,
        BrokenMessageCommand=function(self)
            local missing = nil
            missing:GetName()
        end,
    },
}
