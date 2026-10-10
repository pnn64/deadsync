return Def.ActorFrame {
    OnCommand=function(self) self:sleep(1000) end,
    NOTESKIN:LoadActorForNoteSkin("Down", "Tap Note", "cyber")..{Name="Material"},
}
