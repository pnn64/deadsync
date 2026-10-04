return Def.ActorFrame{
    OnCommand=function()
        SCREENMAN:GetTopScreen():GetChild("SongForeground"):xy(12,-8):zoom(1.25):diffusealpha(0.6)
    end,
    Def.Quad{
        Name="Timer",
        OnCommand=function(self) self:sleep(2):queuecommand("Answer") end,
        AnswerCommand=function() MESSAGEMAN:Broadcast("ShowAnswer") end,
    },
    Def.Quad{
        Name="Panel",
        InitCommand=function(self) self:xy(200,200):zoomto(64,64) end,
        ShowAnswerMessageCommand=function(self) self:sleep(1):queuecommand("Clear") end,
        ClearCommand=function()
            SCREENMAN:GetTopScreen():GetChild("SongForeground"):visible(false)
        end,
    },
}
