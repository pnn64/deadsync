return Def.ActorFrame{
    Def.Actor{
        OnCommand=function(self) self:sleep(1):queuecommand("Start") end,
        StartCommand=function() MESSAGEMAN:Broadcast("StartCycle") end,
    },
    Def.Quad{
        Name="ImmediateCycle",
        OnCommand=function(self) self:xy(100,100):zoomto(40,30) end,
        StartCycleMessageCommand=function(self) self:queuecommand("Tick") end,
        TickCommand=function(self)
            self:addx(5):sleep(0.0166):queuecommand("Tick")
        end,
    },
    Def.Quad{
        Name="DelayedCycle",
        OnCommand=function(self) self:xy(100,200):zoomto(40,30) end,
        StartCycleMessageCommand=function(self) self:sleep(0.4):queuecommand("Tick") end,
        TickCommand=function(self)
            self:addy(5):sleep(0.0166):queuecommand("Tick")
        end,
    },
}
