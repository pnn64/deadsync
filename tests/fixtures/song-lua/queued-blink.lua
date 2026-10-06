return Def.ActorFrame{ Def.Quad{
    Name='Blink', OnCommand=cmd(zoomto,64,32;xy,100,100;sleep,0.15;queuecommand,'Wallop'),
    WallopCommand=cmd(diffusealpha,0;zoom,6;linear,0.15;zoom,0.96;diffusealpha,1;linear,0.05;zoom,1;queuecommand,'Wait'),
    WaitCommand=cmd(diffuseblink;effectperiod,0.1;effectcolor1,1,1,1,1;effectcolor2,0.7,0.7,0.7,1;sleep,1.2;queuecommand,'Away'),
    AwayCommand=cmd(stopeffect;linear,0.2;zoom,6;diffusealpha,0),
} }
