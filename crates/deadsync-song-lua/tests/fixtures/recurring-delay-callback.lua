local pulse, random, calls, results, previous = nil, nil, 0, 0, 0
return Def.ActorFrame {
    OnCommand = function(self) self:SetUpdateFunction(function() end) end,
    Def.Quad {
        Name = "PulseWitness",
        InitCommand = function(self) pulse = self; self:zoomto(16, 16) end,
    },
    Def.Quad {
        Name = "RngWitness",
        InitCommand = function(self) random = self; self:zoomto(16, 16):x(100) end,
    },
    Def.Actor {
        OnCommand = function(self)
            self:sleep(2):finishtweening():sleep(30/115):queuecommand("Pulse")
        end,
        PulseCommand = function(self)
            local beat = GAMESTATE:GetSongBeat()
            calls = calls + 1
            pulse:x(calls):y(beat)
            if beat > 1 then
                local value = math.random(1, 4)
                while previous == value do value = math.random(1, 4) end
                previous = value
                results = results + 1
                random:x(results):y(value)
            end
            self:sleep(60/115):queuecommand("Pulse")
        end,
    },
}
