NAV_INPUT_GENERATIONS = (NAV_INPUT_GENERATIONS or 0) + 1
local target
mod_actions = {{4, function() target:x(9) end, true}}
return Def.ActorFrame{
    Def.Quad{
        Name='InputTarget',
        InitCommand=function(self) target=self; self:x(3) end,
    },
    Def.ActorFrame{
        Name='InputSource',
        ProbeMessageCommand=function(self) target:linear(0.1):x(17) end,
        OffCommand=function(self) MESSAGEMAN:Broadcast('InputOff') end,
    },
}
