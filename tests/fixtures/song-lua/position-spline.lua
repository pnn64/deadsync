local points = {{-48,-135,2},{-16,-60,4},{36,50,-3},{60,140,8}}
return Def.ActorFrame{
    InitCommand=function(self)
        local field = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField")
        local column = field:GetColumnActors()[1]
        local pos, zoom = column:GetPosHandler(), column:GetZoomHandler()
        pos:SetBeatsPerT(0.5):SetReceptorT(0.25):SetSubtractSongBeat(true)
        zoom:SetBeatsPerT(0.5):SetReceptorT(0.25):SetSubtractSongBeat(true)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions("ModsLevel_Song"):Reverse(0.35)
            if beat < 1 or beat >= 3 then
                pos:SetSplineMode("NoteColumnSplineMode_Disabled")
                zoom:SetSplineMode("NoteColumnSplineMode_Disabled")
                return
            end
            pos:SetSplineMode("NoteColumnSplineMode_Position")
            local spline = pos:GetSpline():SetSize(4)
            for i, p in ipairs(points) do
                spline:SetPoint(i, {p[1], p[2] + (beat >= 2 and 20 or 0), p[3]})
            end
            spline:Solve()
            zoom:SetSplineMode("NoteColumnSplineMode_Position")
            local zs = zoom:GetSpline():SetSize(4)
            for i, value in ipairs({0.75, 1.25, 0.5, 1.5}) do
                zs:SetPoint(i, {value, value, value})
            end
            zs:Solve()
        end)
    end,
}
