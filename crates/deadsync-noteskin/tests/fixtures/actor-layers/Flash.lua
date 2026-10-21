return Def.ActorFrame {
    Def.Sprite {
        Texture="_flash 16x1";
        Frames=Sprite.LinearFrames(16,0.32);
        W1Command=cmd(setstate,0;blend,'BlendMode_Add';diffusealpha,1;sleep,0.31;diffusealpha,0);
        W2Command=cmd(diffusealpha,0);
    };
}
