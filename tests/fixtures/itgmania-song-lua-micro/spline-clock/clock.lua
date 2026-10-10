return Def.ActorFrame{
    Name="SplineClock",
    InitCommand=function(self)
        local column=SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField"):GetColumnActors()[1]
        local position=column:GetPosHandler()
        local rotation=column:GetRotHandler()
        self:SetUpdateFunction(function()
            local beat=GAMESTATE:GetSongBeat()
            self:x(GAMESTATE:GetSongPosition():GetMusicSeconds())
            if beat>=0.5 and beat<3 then
                position:SetSplineMode("NoteColumnSplineMode_Position"):SetBeatsPerT(1)
                position:GetSpline():SetSize(2):SetPoint(1,{0,-135,0}):SetPoint(2,{0,128,0}):Solve()
                rotation:SetSplineMode("NoteColumnSplineMode_Offset"):SetBeatsPerT(1)
                rotation:GetSpline():SetSize(1):SetPoint(1,{0,0,beat*0.25}):Solve()
            else
                position:SetSplineMode("NoteColumnSplineMode_Disabled")
                rotation:SetSplineMode("NoteColumnSplineMode_Disabled")
            end
        end)
    end,
    Def.Quad{InitCommand=function(self) self:zoomto(8,8) end}
}
