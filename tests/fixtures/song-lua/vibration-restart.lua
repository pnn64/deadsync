return Def.ActorFrame {
    Name = "Vibration",
    OnCommand = function(self)
        self:vibrate():effectmagnitude(20, 12, 4):sleep(0.5):queuecommand("Stop")
    end,
    StopCommand = function(self)
        self:stopeffect():sleep(0.5):queuecommand("Restart")
    end,
    RestartCommand = function(self)
        self:vibrate():sleep(0.5):queuecommand("Override")
    end,
    OverrideCommand = function(self)
        self:effectmagnitude(2, 3, 4):vibrate():sleep(0.5):queuecommand("Custom")
    end,
    CustomCommand = function(self)
        self:vibrate():effectmagnitude(7, 8, 9):sleep(0.5):queuecommand("Stop")
    end,
    Def.Quad {
        Name = "Witness",
        OnCommand = function(self) self:xy(320, 240):zoomto(64, 32) end,
    },
}
