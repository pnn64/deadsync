local target
local values = {{x=150}}
return Def.ActorFrame{
    Def.Quad{
        Name="Timer",
        OnCommand=function(self) self:sleep(2):queuecommand("Send") end,
        SendCommand=function() MESSAGEMAN:Broadcast("Consume") end,
    },
    Def.ActorFrame{
        Name="Receiver",
        ConsumeMessageCommand=function()
            target:x(table.remove(values,1).x)
        end,
    },
    Def.Quad{
        Name="Target",
        InitCommand=function(self) target=self end,
    },
}
