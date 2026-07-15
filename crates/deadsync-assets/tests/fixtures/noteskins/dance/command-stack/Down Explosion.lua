return Def.ActorFrame {
    NOTESKIN:LoadActor(Var "Button", "Flash")..{
        W1Command=cmd(diffusealpha,1),
        W2Command=cmd(diffusealpha,1),
        W3Command=cmd(diffusealpha,1),
        W4Command=cmd(diffusealpha,1),
        W5Command=cmd(diffusealpha,1),
    },
    NOTESKIN:LoadActor(Var "Button", "Glow")..{
        W1Command=cmd(diffusealpha,1),
        W2Command=cmd(diffusealpha,1),
        W3Command=cmd(diffusealpha,1),
        W4Command=cmd(diffusealpha,1),
        W5Command=cmd(diffusealpha,1),
    },
    NOTESKIN:LoadActor(Var "Button", "Spark")..{
        W1Command=cmd(diffusealpha,1),
        W2Command=cmd(diffusealpha,1),
        W3Command=cmd(diffusealpha,1),
        W4Command=cmd(diffusealpha,1),
        W5Command=cmd(diffusealpha,1),
    },
    NOTESKIN:LoadActor(Var "Button", "Mine Emitter")..{
        HitMineCommand=cmd(),
        ECommand=cmd(diffusealpha,1;sleep,1.0666667;diffusealpha,0),
        E2Command=cmd(diffusealpha,1;blend,'BlendMode_Add';sleep,1.0666667;diffusealpha,0),
    },
}
