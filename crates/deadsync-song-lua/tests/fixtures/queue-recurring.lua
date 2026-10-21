-- ActorFrames own updates; observer frames retain their original sibling order.
local loop, started, stopped
loop_count = 0
curaction = 1
mod_actions = {{1, "Observe", true}}
return Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            if not started and beat >= 1 then
                started = true
                loop:playcommand("Start")
            end
            if not stopped and beat >= 2 then
                stopped = true
                loop:stoptweening()
            end
        end)
    end,
    Def.Quad {
        Name = "Loop",
        InitCommand = function(self) loop = self end,
        StartCommand = function(self) self:sleep(0.07):queuecommand("Update") end,
        UpdateCommand = function(self)
            loop_count = loop_count + 1
            self:x(loop_count):sleep(0.02):queuecommand("Update")
        end,
    },
    Def.ActorFrame {
        OnCommand = function(frame)
            local quad = frame:GetChild("Witness")
            local self = quad
            local update = function(self) self:x(loop_count) end
            frame:SetUpdateFunction(function(_, delta) update(quad, delta) end)
        end,
        Def.Quad {
            Name = "Witness",
        }
    },
}
