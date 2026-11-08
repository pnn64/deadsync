return Def.ActorFrame {
    NOTESKIN:LoadActor(Var "Button", "HitMine Explosion") .. {
        InitCommand=NOTESKIN:GetMetricA("GhostArrowBright", "MineInitCommand"),
        HitMineCommand=NOTESKIN:GetMetricA("GhostArrowBright", "HitMineCommand"),
    },
}
