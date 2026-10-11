local phase = 0
local clock
return Def.ActorFrame{
    Name="Root",
    Def.Quad{
        Name="Walker",
        OnCommand=function(self)
            self:setsize(12,12):xy(40,120):sleep(.02):queuecommand("Update")
        end,
        UpdateCommand=function(self)
            self:addx(2)
            if GAMESTATE:GetSongBeat() < 8 then self:sleep(.02):queuecommand("Update") end
        end,
    },
    Def.Quad{
        Name="Clock",
        InitCommand=function(self) clock = self end,
        OnCommand=function(self)
            self:setsize(12,12):xy(40,180)
        end,
    },
    Def.ActorFrame{
        Name="ClockDriver",
        OnCommand=function(self)
            self:SetUpdateFunction(function(_, delta)
                phase = phase + delta
                clock:x(40 + phase * 20)
            end)
        end,
    },
}
