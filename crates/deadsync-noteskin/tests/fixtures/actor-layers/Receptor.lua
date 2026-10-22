return Def.ActorFrame {
    Def.Sprite {
        Texture="_rail";
        InitCommand=function(self) self:valign(1):zoomy(SCREEN_HEIGHT):addrotationz(180) end;
    };
    Def.Sprite {
        Texture="_press";
        InitCommand=function(self) self:valign(1):diffusealpha(0):addrotationz(180) end;
        PressCommand=cmd(diffusealpha,1);
        LiftCommand=cmd(diffusealpha,0);
    };
    Def.Sprite {
        Texture="_target 4x1";
        Frames=Sprite.LinearFrames(4,1);
        InitCommand=function(self) self:addrotationz(180) end;
        NoneCommand=cmd(zoom,0.8;linear,0.1;zoom,1);
        W1Command=cmd(zoom,1.2;linear,0.1;zoom,1);
        PressCommand="";
        LiftCommand="";
    };
}
