return Def.ActorFrame {
    OnCommand = function(self)
        local left = self:GetChild("Left")
        local right = self:GetChild("Right")
        local shared = { held = false, count = -0.0 }
        local alias = shared
        local independent = { held = false }
        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            if event.GameButton == "Left" then
                shared.held = not alias.held
                left:visible(shared.held)
            end
            return false
        end)
        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            if event.GameButton == "Right" then
                alias.count = alias.count + event.DeviceInput.level
                right:visible(alias.held and alias.count >= 0.5)
                if event.type == "InputEventType_Release" then
                    shared.held = false
                    left:visible(false)
                end
                return alias.held
            elseif event.GameButton == "Other" then
                independent.held = not independent.held
                right:visible(independent.held)
            end
            return false
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
