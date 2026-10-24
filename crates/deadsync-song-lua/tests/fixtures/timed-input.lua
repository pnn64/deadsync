return Def.ActorFrame {
    InitCommand = function(self) self:SetUpdateRate(1.25) end,
    OnCommand = function(self)
        local clock = self:GetChild("Clock")
        local lamp = clock:GetChild("Lamp")
        local shared = { active = false, remaining = -0.0, ticks = -0.0, order = -0.0, root_delta = -0.0 }
        local alias = shared
        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            if event.GameButton == "Right" then
                shared.remaining = 0.125 + event.DeviceInput.level
                shared.active = true
                lamp:visible(true)
                return true
            elseif event.GameButton == "Left" then
                shared.active = false
                lamp:visible(false)
            end
            return false
        end)
        clock:SetUpdateFunction(function(_, delta)
            alias.order = alias.order * 2 + 1
            alias.ticks = alias.ticks + delta
            if alias.active then
                alias.remaining = alias.remaining - delta
                if alias.remaining <= 0 then
                    alias.active = false
                    lamp:visible(false)
                end
            end
            return true
        end)
        self:SetUpdateFunction(function(_, delta)
            shared.order = shared.order * 2
            shared.root_delta = delta
        end)
    end,
    Def.ActorFrame {
        Name = "Clock",
        InitCommand = function(self) self:SetUpdateRate(0.5):hibernate(0.0625) end,
        Def.Quad {
            Name = "Lamp",
            InitCommand = function(self)
                self:xy(32, 20):zoomto(12, 12):diffuse(1, 0.5, 0, 1):visible(false)
            end,
        },
    },
}
