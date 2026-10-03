local fired = false
local controlling = false
local account = {nested = {count = 0}}
account.self = account
local original = account
local marker = "cold"
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
                account.nested.count = account.nested.count + 1
                self:playcommand("Bump")
                MESSAGEMAN:Broadcast("BodyRotateBuildings")
            end,
            BumpCommand = function(self)
                account.nested.count = account.nested.count + 1
                marker = "hot"
            end,
        },
    },
    Def.Quad {
        Name = "ClockWitness",
        OnCommand = function(self)
            self:SetUpdateFunction(function(self)
                local beat = GAMESTATE:GetSongBeat()
                if beat < 1.75 then
                    assert(not controlling, "queued Lua state changed before dispatch")
                    assert(account == original and account.self == account)
                    assert(account.nested.count == 0 and marker == "cold")
                elseif beat >= 1.8 then
                    assert(controlling, "queued Lua state was never dispatched")
                    assert(account == original and account.self == account)
                    assert(account.nested.count == 2 and marker == "hot")
                end
                self:x(controlling and 100 or 0)
            end)
        end,
        BodyRotateBuildingsMessageCommand = function(self) self:y(23) end,
    },
}
