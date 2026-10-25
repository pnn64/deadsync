local root = Def.ActorFrame {
    OnCommand = function(self)
        local lamp = self:GetChild("Lamp")
        local state = { elapsed = -0.0, lit = true }
        self:SetUpdateFunction(function(_, delta)
            state.elapsed = state.elapsed + delta
            if state.elapsed >= 0.125 then
                state.elapsed = 0
                state.lit = not state.lit
                lamp:visible(state.lit)
            end
        end)
    end,
    Def.Quad {
        Name = "Lamp",
        InitCommand = function(self)
            self:xy(32, 20):zoomto(12, 12):diffuse(0.5, 1, 0.25, 1)
        end,
    },
}
for index = 1, 16 do
    root[#root + 1] = Def.ActorFrame { Name = "Branch" .. index }
end
return root
