return Def.ActorFrame {
    OnCommand = function(self)
        local lamp = self:GetChild("Lamp")
        local shared = { mode = "idle" }
        for _ = 1, 16 do
            SCREENMAN:GetTopScreen():AddInputCallback(function(event)
                if event.GameButton == "Right" then
                    shared.mode = "armed"
                    lamp:visible(true)
                elseif event.GameButton == "Left" then
                    shared.mode = "idle"
                    lamp:visible(false)
                elseif shared.mode == "armed" then
                    shared.mode = "cooldown"
                    lamp:visible(false)
                end
                return false
            end)
        end
    end,
    Def.Quad {
        Name = "Lamp",
        InitCommand = function(self)
            self:xy(32, 20):zoomto(12, 12):diffuse(0.25, 1, 0.75, 1)
        end,
    },
}
