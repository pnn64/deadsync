-- ActorFrames own updates; observer frames retain their original sibling order.
local fired = false
controlling = false
curaction = 1
mod_actions = {{1, "Observe", true}}
return Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            if not fired and GAMESTATE:GetSongBeat() >= 1 then
                fired = true
                MESSAGEMAN:Broadcast("ShowGame2")
            end
        end)
    end,
    Def.ActorFrame {
        ShowGame2MessageCommand = function(self) self:playcommand("Start") end,
        Def.Quad {
            StartCommand = function(self) self:sleep(0.5):queuecommand("SpawnPlayers") end,
            SpawnPlayersCommand = function(self) self:sleep(0.25):queuecommand("SetControlling") end,
            SetControllingCommand = function(self)
                controlling = true
                MESSAGEMAN:Broadcast("BodyRotateBuildings")
            end,
        },
    },
    Def.ActorFrame {
        OnCommand = function(frame)
            local quad = frame:GetChild("ClockWitness")
            local self = quad
            local update = function(self)
                local beat = GAMESTATE:GetSongBeat()
                if beat < 1.75 then
                    assert(not controlling, "queued Lua state changed before dispatch")
                elseif beat >= 1.8 then
                    assert(controlling, "queued Lua state was never dispatched")
                end
                self:x(controlling and 100 or 0)
            end
            frame:SetUpdateFunction(function(_, delta) update(quad, delta) end)
        end,
        Def.Quad {
            Name = "ClockWitness",
            BodyRotateBuildingsMessageCommand = function(self) self:y(23) end,
        }
    },
}
