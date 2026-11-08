return Def.ActorFrame{
    Def.Quad{
        Name="Timer",
        OnCommand=function(self) self:sleep(2):queuecommand("Answer") end,
        AnswerCommand=function() MESSAGEMAN:Broadcast("ShowAnswer") end,
    },
    Def.Quad{
        Name="Panel",
        ShowAnswerMessageCommand=function(self) self:visible(false) end,
    },
}
