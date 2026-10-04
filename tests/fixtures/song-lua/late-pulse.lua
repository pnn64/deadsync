local function start(self)
    self:sleep(3.25):queuecommand('Pulse'):sleep(1.4):queuecommand('Change')
end
local function pulse(self)
    self:pulse():effectmagnitude(1, 1.03, 1):effectperiod(0.65)
end
local function change(self)
    self:pulse():effectmagnitude(1, 1.03, 1):effectperiod(0.95)
end
return Def.ActorFrame{
    Def.Quad{
        Name='Leaf', InitCommand=cmd(xy,200,320;setsize,80,112;valign,1),
        OnCommand=start, PulseCommand=pulse, ChangeCommand=change,
    },
    Def.ActorFrame{
        Name='Parent', InitCommand=cmd(xy,400,320),
        OnCommand=start, PulseCommand=pulse, ChangeCommand=change,
        Def.Quad{Name='Nested', InitCommand=cmd(x,35;setsize,80,112;valign,1)},
    },
}
