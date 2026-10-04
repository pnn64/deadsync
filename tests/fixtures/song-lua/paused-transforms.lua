local elapsed = 0
return Def.ActorFrame{
    OnCommand=function(self)
        local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
        local column = player:GetChild("NoteField"):GetColumnActors()[1]
        local handler = column:GetPosHandler():SetSplineMode("NoteColumnSplineMode_Offset")
        local spline = handler:GetSpline():SetSize(2)
        self:SetUpdateFunction(function(_, delta)
            elapsed = elapsed + delta
            player:x(200 + elapsed * 10)
            spline:SetPoint(1, {0, elapsed * 3, 0})
            spline:SetPoint(2, {0, elapsed * 3, 0})
            spline:Solve()
        end)
    end,
}
