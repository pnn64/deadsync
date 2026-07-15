return Def.ActorFrame{
    NOTESKIN:LoadActorForNoteSkin("Down", "Tap Note", "ddr-note")..{
        Name="FixtureArrow",
        OnCommand=function(self)
            self:rotationz(90)
        end,
    },
}
