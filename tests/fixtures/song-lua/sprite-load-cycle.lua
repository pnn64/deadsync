local target
return Def.ActorFrame {
    Def.Sprite {
        Name = "Face", Texture = "Normal 2x6.png",
        Frames = {{Frame=5, Delay=100}},
        InitCommand = function(self) target = self; self:xy(320,240):pause() end,
    },
    Def.Actor {
        OnCommand = function(self) self:sleep(0.2):queuecommand("Swap") end,
        SwapCommand = function(self)
            target:Load("Fake 2x6.png"):setstate(1)
            self:sleep(0.3):queuecommand("Restore")
        end,
        RestoreCommand = function(self)
            target:Load("Normal 2x6.png"):setstate(2)
        end,
    },
}
