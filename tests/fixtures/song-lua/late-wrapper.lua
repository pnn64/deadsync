return Def.ActorFrame {
    Name = "Root",
    Def.Quad {
        Name = "LateWrapper",
        OnCommand = function(self)
            self:setsize(40,20):xy(400,240):sleep(1):queuecommand("Wrap")
        end,
        WrapCommand = function(self)
            self:AddWrapperState():bob():effectmagnitude(50,20,0):effectperiod(0.5)
        end,
    },
}
