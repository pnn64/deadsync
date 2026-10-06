local probe
return Def.ActorFrame{
    OnCommand=function(self) self:sleep(1):queuecommand("Update") end,
    UpdateCommand=function(self)
        probe:zoomto(60,40):pulse():effectperiod(0.2):effect_hold_at_full(0.8)
        probe:visible(true):sleep(0):linear(0.5):visible(false):zoomx(6)
    end,
    Def.Quad{
        Name="Probe",
        InitCommand=function(self)
            probe=self
            self:setsize(20,10):xy(120,100):visible(false)
        end,
    },
}
