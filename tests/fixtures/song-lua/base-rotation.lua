return Def.ActorFrame{
    Def.Quad{
        Name="BaseRotation",
        InitCommand=function(self)
            self:baserotationx(-60):baserotationy(20):baserotationz(90)
                :rotationx(10):rotationy(15):rotationz(5)
            mod_actions={{0, string.format("%.0f:%.0f:%.0f",
                self:GetRotationX(), self:GetRotationY(), self:GetRotationZ()), true}}
        end,
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                local beat=GAMESTATE:GetSongBeat()
                self:baserotationz(beat >= 1 and 45 or 90):rotationz(beat*10)
            end)
        end,
    },
}
