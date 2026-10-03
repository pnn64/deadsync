local targets = {}
local started, appended, cleared, touched = false, false, false, false
queue_result = ""
curaction = 1
mod_actions = {{1, "Observe", true}}
return Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            if not started and beat >= 1 then
                started = true
                targets.chain:sleep(0.25):queuecommand("First"):sleep(0.25):queuecommand("Second")
                targets.stop:linear(0.5):x(64):queuecommand("Cancelled")
                targets.finish:linear(0.5):x(64):queuecommand("Cancelled")
            end
            if not appended and beat >= 1.1 then
                appended = true
                targets.chain:sleep(0.1):x(40):queuecommand("Third")
            end
            if not cleared and beat >= 1.2 then
                cleared = true
                targets.stop:stoptweening()
                targets.finish:finishtweening()
            end
            if not touched and beat >= 1.3 then
                touched = true
                targets.stop:playcommand("Touch")
                targets.finish:playcommand("Touch")
            end
        end)
    end,
    Def.Quad {
        Name = "Chain",
        InitCommand = function(self) targets.chain = self end,
        FirstCommand = function(self) queue_result = queue_result .. "1"; self:x(10) end,
        SecondCommand = function(self) queue_result = queue_result .. "2"; self:y(20) end,
        ThirdCommand = function(self) queue_result = queue_result .. "3"; self:z(30) end,
    },
    Def.Quad {
        Name = "Stopped",
        InitCommand = function(self) targets.stop = self end,
        CancelledCommand = function() error("stopped command was dispatched") end,
        TouchCommand = function(self) self:y(1) end,
    },
    Def.Quad {
        Name = "Finished",
        InitCommand = function(self) targets.finish = self end,
        CancelledCommand = function() error("finished command was dispatched") end,
        TouchCommand = function(self) self:y(1) end,
    },
    Def.Quad {
        Name = "Witness",
        OnCommand = function(self)
            self:SetUpdateFunction(function(self)
                if GAMESTATE:GetSongBeat() >= 1.8 then
                    assert(queue_result == "123", "queued commands changed order")
                end
                self:x(#queue_result)
            end)
        end,
    },
}
