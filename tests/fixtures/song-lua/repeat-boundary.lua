local started=false
return Def.ActorFrame {
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            if not started and GAMESTATE:GetSongBeat() >= 75.17 then
                started=true; MESSAGEMAN:Broadcast("Show")
            end
        end)
    end,
    Def.ActorFrame {
        ShowMessageCommand=function(self) self:queuecommand("Update") end,
        Def.Quad {
            Name="Rotation",
            OnCommand=function(self) self:xy(96,240):zoomto(70,70):queuecommand("Update") end,
            UpdateCommand=function(self)
                self:rotationz(0):linear(0.1):rotationz(-90):sleep(0.9)
                    :linear(0.1):rotationz(-270):sleep(0.9)
                    :linear(0.1):rotationz(-360):sleep(0.9):queuecommand("Update")
            end,
        },
    },
}
