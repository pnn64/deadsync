return Def.ActorFrame {
    InitCommand = function(self) self:SetUpdateRate(1.25) end,
    OnCommand = function(self)
        local clock = self:GetChild("Clock")
        local lamp = clock:GetChild("Lamp")
        local shared = { mode = "idle", previous = "idle", decoy = "idle", remaining = -0.0, cooldown = -0.0, total = -0.0 }
        local alias = shared
        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            if event.GameButton == "Right" then
                shared.previous = shared.mode
                if shared.mode == "armed" then
                    shared.mode = "cooldown"
                    shared.cooldown = 0.0625
                    lamp:visible(false)
                else
                    shared.mode = "armed"
                    shared.remaining = 0.125 + event.DeviceInput.level
                    lamp:visible(true)
                end
                return shared.mode == "armed"
            elseif event.GameButton == "Left" then
                shared.previous = shared.mode
                shared.mode = "idle"
                lamp:visible(false)
            end
            return false
        end)
        clock:SetUpdateFunction(function(_, delta)
            if alias.mode == "armed" then
                alias.remaining = alias.remaining - delta
                if alias.remaining <= 0 then
                    alias.previous = alias.mode
                    alias.mode = "cooldown"
                    alias.cooldown = 0.0625
                    lamp:visible(false)
                end
            elseif alias.mode == "cooldown" then
                alias.cooldown = alias.cooldown - delta
                if alias.cooldown <= 0 then
                    alias.previous = alias.mode
                    alias.mode = "idle"
                end
            end
        end)
        self:SetUpdateFunction(function(_, delta)
            shared.total = shared.total + delta
            return shared.mode
        end)
    end,
    Def.ActorFrame {
        Name = "Clock",
        InitCommand = function(self) self:SetUpdateRate(0.5):hibernate(0.0625) end,
        Def.Quad {
            Name = "Lamp",
            InitCommand = function(self)
                self:xy(32, 20):zoomto(12, 12):diffuse(0.25, 0.75, 1, 1):visible(false)
            end,
        },
    },
}
