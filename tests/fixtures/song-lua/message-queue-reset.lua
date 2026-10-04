local schedule = {{1, "AyaOn"}, {3, "TVGrow"}, {6, "TVShrink"}}
local next_event = 1
return Def.ActorFrame{
    Name="Root",
    Def.ActorFrame{
        Name="Driver",
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                while schedule[next_event] and GAMESTATE:GetSongBeat() >= schedule[next_event][1] do
                    MESSAGEMAN:Broadcast(schedule[next_event][2])
                    next_event = next_event + 1
                end
            end)
        end,
    },
    Def.Quad{
        Name="Runner",
        OnCommand=function(self) self:setsize(20,20):x(1120):y(291) end,
        AyaOnMessageCommand=function(self)
            self:diffusealpha(1):x(1120):linear(.5):x(608):sleep(0):diffusealpha(0)
        end,
        TVGrowMessageCommand=function(self)
            self:sleep(.8):queuemessage("MoveInnerBGFast"):diffusealpha(1):x(1120)
        end,
        TVShrinkMessageCommand=function(self)
            self:x(608):diffusealpha(0):sleep(.4):diffusealpha(1)
                :accelerate(1):addx(-1000):sleep(0):x(1120)
        end,
    },
    Def.ActorFrame{
        Name="TV",
        OnCommand=function(self) self:x(832):y(288) end,
        AyaOnMessageCommand=function(self) self:linear(.5):addx(-512) end,
        TVGrowMessageCommand=function(self) self:playcommand("Grow"):linear(.8):zoom(2.65):y(380) end,
        TVShrinkMessageCommand=function(self)
            self:playcommand("Shrink"):sleep(.2):queuemessage("ReturnInnerBGFast")
                :linear(.2):zoom(1):y(288):accelerate(1):addx(-1000):sleep(0):x(832)
        end,
        Def.Quad{
            Name="Static",
            OnCommand=function(self) self:setsize(20,20):x(1):y(-54) end,
            GrowCommand=function(self) self:sleep(.9):linear(.3):diffusealpha(0) end,
            ShrinkCommand=function(self) self:linear(.2):diffusealpha(1) end,
        },
        Def.Quad{Name="Frame", OnCommand=function(self) self:setsize(20,20):x(1) end},
    },
    Def.ActorFrame{
        Name="InnerBG",
        MoveInnerBGFastMessageCommand=function(self) self:y(480) end,
        ReturnInnerBGFastMessageCommand=function(self) self:y(0) end,
        Def.Quad{Name="Backdrop", OnCommand=function(self) self:setsize(20,20):x(100) end},
    },
}
