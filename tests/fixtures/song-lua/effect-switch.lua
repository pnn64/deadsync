local function start(self)
    self:bob():effectperiod(1):effectmagnitude(0,3,0)
    self:sleep(.5):queuecommand('Vibrate'):sleep(.5):queuecommand('Pulse')
        :sleep(.5):queuecommand('Rainbow'):sleep(.5):queuecommand('Vibrate')
        :sleep(.5):queuecommand('Bob'):sleep(.5):queuecommand('Stop')
end
local function vibrate(self) self:vibrate():effectmagnitude(3,3,3) end
local function pulse(self) self:pulse():effectperiod(.65):effectmagnitude(1,1.03,3) end
local function bob(self) self:bob():effectperiod(1):effectmagnitude(0,3,0) end
return Def.ActorFrame{
    Def.Quad{
        Name='Leaf', InitCommand=cmd(xy,200,320;setsize,80,112;valign,1;diffuse,.2,.4,.6,.8),
        OnCommand=start, VibrateCommand=vibrate, PulseCommand=pulse,
        RainbowCommand=function(self) self:rainbow(false) end,
        BobCommand=bob, StopCommand=cmd(stopeffect),
    },
    Def.ActorFrame{
        Name='Parent', InitCommand=cmd(xy,400,320),
        OnCommand=start, VibrateCommand=vibrate, PulseCommand=pulse,
        RainbowCommand=cmd(stopeffect), BobCommand=bob, StopCommand=cmd(stopeffect),
        Def.Quad{Name='Nested', InitCommand=cmd(x,35;setsize,80,112;valign,1)},
    },
}
