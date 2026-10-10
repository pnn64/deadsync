local fired = false
return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            if not fired then
                MESSAGEMAN:Broadcast("StartMove")
                fired = true
            end
        end)
    end,
    Def.Quad{
        InitCommand=function(self) self:visible(false) end,
        OnCommand=function(self) self:sleep(1000) end,
        StartMoveMessageCommand=function()
            SCREENMAN:GetTopScreen():GetChild("PlayerP1")
                :accelerate(52):x(4.3*SCREEN_WIDTH/12)
            SCREENMAN:GetTopScreen():GetChild("PlayerP2")
                :accelerate(52):x(7.7*SCREEN_WIDTH/12)
        end,
    },
}
