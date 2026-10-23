return Def.Quad {
    Name = "ClosedInputQuad",
    InitCommand = function(self)
        self:xy(32, 48):zoomto(24, 24):diffuse(1, 0.25, 0.5, 0.75)
    end,
    OnCommand = function(self)
        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            return event.GameButton == "Right"
                and event.PlayerNumber == "PlayerNumber_P1"
                and event.DeviceInput.down
        end)
    end,
}
