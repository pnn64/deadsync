return Def.ActorFrame {
    Name = "Root",
    Def.Quad {
        Name = "LateWrapperVibrate",
        OnCommand = function(self)
            self:setsize(40,20):xy(400,240):sleep(1):queuecommand("Wrap")
        end,
        WrapCommand = function(self)
            self:AddWrapperState():vibrate():effectmagnitude(5,7,0):effectperiod(0.5)
        end,
    },
}
