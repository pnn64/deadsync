return Def.ActorFrame {
    OnCommand = function(self)
        local lamp = self:GetChild("Lamp")
        local shared = { x = 32, lit = true }
        for _ = 1, 16 do
            SCREENMAN:GetTopScreen():AddInputCallback(function(event)
                if event.GameButton == "Right" then
                    shared.x = shared.x + event.DeviceInput.level
                    lamp:x(shared.x)
                elseif event.GameButton == "Left" then
                    shared.lit = not shared.lit
                    lamp:visible(shared.lit)
                end
                return false
            end)
        end
    end,
    Def.Quad {
        Name = "Lamp",
        InitCommand = function(self)
            self:xy(32, 20):zoomto(12, 12):diffuse(1, 0.5, 0.25, 1)
        end,
    },
}
