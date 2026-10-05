local zero, observer = 0, nil
return Def.ActorFrame{
    Name="Root", OnCommand=function(self)
        self:queuemessage("Zero")
        self:SetUpdateFunction(function() observer:x(zero * 50) end)
    end,
    Def.Quad{
        Name="Early", InitCommand=cmd(xy,160,200;setsize,32,32;diffusealpha,0),
        ZeroMessageCommand=function(self) zero = 1 end,
        HitMessageCommand=cmd(diffusealpha,1;linear,.1;x,180),
        AgainMessageCommand=cmd(diffusealpha,.4),
        NeverMessageCommand=cmd(y,300),
    },
    Def.Quad{
        Name="Sender", InitCommand=cmd(xy,300,200;setsize,32,32),
        OnCommand=function(self)
            self:sleep(.2):queuemessage("Hit"):sleep(.15):queuemessage("Again")
                :sleep(.25):queuecommand("Cancel"):sleep(.3):queuemessage("Never")
        end,
        CancelCommand=cmd(stoptweening),
    },
    Def.Quad{
        Name="Late", InitCommand=cmd(xy,450,200;setsize,32,32;diffusealpha,0),
        ZeroMessageCommand=function(self) end,
        HitMessageCommand=cmd(diffusealpha,1;linear,.1;x,470),
        AgainMessageCommand=cmd(diffusealpha,.4),
        NeverMessageCommand=cmd(y,300),
    },
    Def.Quad{
        Name="Observer", InitCommand=function(self) observer = self; self:xy(0,380):setsize(16,16) end,
    },
}
