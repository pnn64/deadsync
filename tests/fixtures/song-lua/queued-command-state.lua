local sent = false
return Def.ActorFrame{
    Name="Root",
    Def.ActorFrame{
        Name="Driver",
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                if not sent and GAMESTATE:GetSongBeat() >= 1 then
                    sent = true
                    MESSAGEMAN:Broadcast("QueueState")
                end
            end)
        end,
    },
    Def.Quad{
        Name="CommandState",
        OnCommand=function(self) self:setsize(20,20):x(100):y(200) end,
        QueueStateMessageCommand=function(self)
            self:sleep(.4):queuecommand("Mark"):x(180):diffusealpha(.5)
        end,
        MarkCommand=function(self) self:aux(7) end,
    },
    Def.Quad{
        Name="MessageState",
        OnCommand=function(self) self:setsize(20,20):x(300):y(200) end,
        QueueStateMessageCommand=function(self)
            self:sleep(.4):queuemessage("Mark"):x(380):diffusealpha(.5)
        end,
        MarkMessageCommand=function(self) self:aux(7) end,
    },
}
