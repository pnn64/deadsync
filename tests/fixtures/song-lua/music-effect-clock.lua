return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function(frame)
            for _, name in ipairs({'Timer', 'Music', 'NoOffset'}) do
                local actor=frame:GetChild(name)
                actor:xy(actor:GetSecsIntoEffect(), actor:GetEffectDelta())
                actor:z(actor:GetRotationZ())
            end
        end)
    end,
    Def.Quad{Name='Timer', OnCommand=function(self)
        self:spin():effectmagnitude(0,0,80):effectperiod(1.5)
    end},
    Def.Quad{Name='Music', OnCommand=function(self)
        self:spin():effectmagnitude(0,0,80):effectperiod(1.5):effectclock('music')
    end},
    Def.Quad{Name='NoOffset', OnCommand=function(self)
        self:spin():effectmagnitude(0,0,80):effectperiod(1.5):effectclock('musicnooffset')
    end},
    Def.Quad{Name='PulseTimer', InitCommand=function(self) self:xy(200,320):setsize(80,112):valign(1) end,
        OnCommand=function(self) self:pulse():effectmagnitude(1,1.3,1):effectperiod(1.5):effectclock('timer') end},
    Def.Quad{Name='PulseMusic', InitCommand=function(self) self:xy(400,320):setsize(80,112):valign(1) end,
        OnCommand=function(self) self:pulse():effectmagnitude(1,1.3,1):effectperiod(1.5):effectclock('music') end},
    Def.Quad{Name='PulseNoOffset', InitCommand=function(self) self:xy(600,320):setsize(80,112):valign(1) end,
        OnCommand=function(self) self:pulse():effectmagnitude(1,1.3,1):effectperiod(1.5):effectclock('musicnooffset') end},
}
