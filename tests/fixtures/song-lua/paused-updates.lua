local phase = 0
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
        OnCommand=function(self)
            self:setsize(12,12):xy(40,180)
            self:SetUpdateFunction(function(actor, delta)
                phase = phase + delta
                actor:x(40 + phase * 20)
            end)
        end,
    },
}
