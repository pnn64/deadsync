return Def.ActorFrame {
    OnCommand = function(self)
        local lamp = self:GetChild("Lamp")
        local buttons = { {}, {} }
        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            local player = event.PlayerNumber == "PlayerNumber_P2" and 2 or 1
            if event.type == "InputEventType_Release" then
                buttons[player][event.GameButton] = nil
            else
                buttons[player][event.GameButton] = true
            end
            if buttons[1].Right or buttons[2].Left then
                lamp:visible(true)
            else
                lamp:visible(false)
            end
            return false
        end)
    end,
    Def.Quad {
        Name = "Lamp",
        InitCommand = function(self)
            self:xy(32, 20):zoomto(12, 12):diffuse(0.25, 0.75, 1, 1)
        end,
    },
}
