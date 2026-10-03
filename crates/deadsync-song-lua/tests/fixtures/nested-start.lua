local fired = false
-- Leave an observed action pending so the native recorder retains events
-- between its ordinary quarter-beat samples, including queued broadcasts.
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
        ShowGame2MessageCommand = function(self) self:visible(true):playcommand("Start") end,
        Def.Quad {
            Name = "Body",
            StartCommand = function(self) self:sleep(0.5):queuecommand("SpawnPlayers") end,
            SpawnPlayersCommand = function(self)
                self:x(23)
                self:sleep(0.25):queuecommand("SetControlling")
            end,
            SetControllingCommand = function(self)
                self:y(45)
                MESSAGEMAN:Broadcast("BodyRotateBuildings")
            end,
        },
    },
    Def.Quad {
        Name = "Buildings",
        BodyRotateBuildingsMessageCommand = function(self) self:x(67) end,
    },
}
