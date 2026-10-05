local started=false
return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            if not started and GAMESTATE:GetSongBeat() >= 1 then
                started=true
                MESSAGEMAN:Broadcast("Show")
            end
        end)
    end,
    Def.ActorFrame{
        Name="Group",
        ShowMessageCommand=function(self) self:queuecommand("Update") end,
        Def.Quad{
            Name="Drift",
            OnCommand=function(self) self:x(854):y(240):zoomto(64,64) end,
            UpdateCommand=function(self)
                self:x(854):linear(0.4):addx(696):sleep(0):queuecommand("Update")
            end,
        },
    },
}
