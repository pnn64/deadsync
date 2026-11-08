return Def.ActorFrame {
    NOTESKIN:LoadActor(Var "Button", "Tap Explosion Bright") .. {
        InitCommand=cmd(diffusealpha,0);
        W1Command=cmd(diffusealpha,1);
        W2Command=cmd(diffusealpha,1);
        DimCommand=cmd(visible,true);
        BrightCommand=cmd(visible,true);
    };
}
