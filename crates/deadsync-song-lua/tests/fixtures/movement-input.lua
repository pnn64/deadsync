return Def.ActorFrame {
    InitCommand = function(self) self:SetUpdateRate(1.25) end,
    OnCommand = function(self)
        local clock = self:GetChild("Clock")
        local mover = clock:GetChild("Mover")
        local state = { active = false, x = 32, y = 20, remaining = -0.0, total = -0.0, moves = 0 }
        local alias = state
        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            if event.GameButton == "Right" then
                state.x = 32 + event.DeviceInput.level * 10
                state.y = 20
                state.remaining = 0.125 + event.DeviceInput.level
                state.active = true
                mover:x(state.x):y(state.y):visible(true)
                return true
            elseif event.GameButton == "Left" then
                state.x = 16 - event.DeviceInput.level * 4
                state.y = 28
                state.active = false
                mover:x(state.x):y(state.y):visible(false)
            end
            return false
        end)
        clock:SetUpdateFunction(function(_, delta)
            if alias.active then
                alias.moves = alias.moves + 1
                alias.y = alias.y + delta * 8
                mover:y(alias.y)
                alias.remaining = alias.remaining - delta
                if alias.remaining <= 0 then
                    alias.active = false
                    mover:visible(false)
                end
            end
        end)
        self:SetUpdateFunction(function(_, delta) state.total = state.total + delta end)
    end,
    Def.ActorFrame {
        Name = "Clock",
        InitCommand = function(self) self:SetUpdateRate(0.5):hibernate(0.0625) end,
        Def.Quad {
            Name = "Mover",
            InitCommand = function(self)
                self:xy(32, 20):zoomto(12, 12):diffuse(0.25, 0.75, 1, 1):visible(false)
            end,
        },
    },
}
