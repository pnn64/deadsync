return Def.ActorFrame{
    Name="Root",
    Def.Quad{
        Name="Flash", InitCommand=cmd(xy,200,240;setsize,64,64;diffusealpha,0),
        OnCommand=cmd(sleep,1.2;diffusealpha,1;linear,.1;diffusealpha,0;sleep,.2;diffusealpha,1;linear,.1;diffusealpha,0),
    },
    Def.Quad{
        Name="Fade", InitCommand=cmd(xy,400,240;setsize,64,64),
        OnCommand=cmd(linear,3;diffusealpha,0),
    },
    Def.Quad{
        Name="Glow", InitCommand=cmd(xy,600,240;setsize,64,64;diffusealpha,0;glow,.2,.4,.6,1),
        OnCommand=cmd(sleep,.3;decelerate,2.7;glow,.8,.6,.4,0),
    },
}
