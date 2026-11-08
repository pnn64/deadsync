return Def.ActorFrame {
    OnCommand = function(self)
        local left = self:GetChild("Left")
        local right = self:GetChild("Right")
        local alias = left
        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            if event.GameButton == "Left" then
                left:visible(event.DeviceInput.down)
                alias:visible(event.DeviceInput.level >= 0.5)
            elseif event.GameButton == "Right" then
                right:visible(event.DeviceInput.down):visible(event.type ~= "InputEventType_Release")
                left:visible(false)
            else
                left:visible(true)
                right:visible(false)
            end
            return event.DeviceInput.down and event.PlayerNumber == "PlayerNumber_P1"
        end)
    end,
    Def.Quad {
        Name = "Left",
        InitCommand = function(self)
            self:xy(20, 20):zoomto(12, 12):diffuse(1, 0, 0, 1):visible(false)
        end,
    },
    Def.Quad {
        Name = "Right",
        InitCommand = function(self)
            self:xy(40, 20):zoomto(12, 12):diffuse(0, 1, 0, 1):visible(true)
        end,
    },
}
