local function set_column_y(column, value)
    local field = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
    local handler = field:GetColumnActors()[column]:GetPosHandler()
    handler:SetSplineMode("NoteColumnSplineMode_Offset")
    handler:SetBeatsPerT(10)
    local spline = handler:GetSpline()
    spline:SetSize(2)
    spline:SetPoint(1, {0, value, 0})
    spline:SetPoint(2, {0, value, 0.001})
    spline:Solve()
end

mods_ease = {
    {4, 0.5, 33.75, 0, function(value) set_column_y(8, value) end, "len", ease.outSine},
    {5, 0.5, 0, 33.75, function(value) set_column_y(7, value) end, "len", ease.inSine},
}

return Def.ActorFrame{}
