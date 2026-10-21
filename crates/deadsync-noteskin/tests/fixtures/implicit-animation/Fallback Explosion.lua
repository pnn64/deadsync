return Def.ActorFrame {
    -- An image actor inherits Sprite::LoadStatesFromTexture's default states.
    LoadActor(NOTESKIN:GetPath("Down", "Tap Explosion")) .. {
        InitCommand=cmd(diffusealpha,0);
        W1Command=cmd(diffusealpha,1;zoom,0.65;linear,0.22;zoom,1.4;diffusealpha,0);
    };
    -- The equivalent implicit Def.Sprite must animate the same way.
    Def.Sprite {
        Texture="Down Tap Explosion 2x1.png";
        InitCommand=cmd(diffusealpha,0);
        W2Command=cmd(diffusealpha,1;zoom,0.65;linear,0.22;zoom,1.4;diffusealpha,0);
    };
    Def.Sprite {
        Texture="Down Tap Explosion 2x1.png";
        Frame0000=1;
        Delay0000=0;
        InitCommand=cmd(diffusealpha,0);
        W3Command=cmd(diffusealpha,1;zoom,0.65;linear,0.22;zoom,1.4;diffusealpha,0);
    };
    Def.Sprite {
        Texture="Down Tap Explosion 2x1.png";
        InitCommand=cmd(pause;setstate,1;diffusealpha,0);
        W4Command=cmd(diffusealpha,1;zoom,0.65;linear,0.22;zoom,1.4;diffusealpha,0);
    };
    Def.Sprite {
        Texture="Down Tap Explosion 2x1.png";
        InitCommand=cmd(diffusealpha,0);
        W5Command=cmd(setstate,0;diffusealpha,1;linear,0.22;diffusealpha,0);
    };
}
