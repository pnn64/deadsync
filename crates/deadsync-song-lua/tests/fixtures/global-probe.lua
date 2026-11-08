local started = false
probe_world = { running = false, calls = 0, starts = 0,
    pose = { y = 100, gravity = 0 }, body = nil, witness = nil }
probe_alias = probe_world.pose
probe_alias.self = probe_alias
local helper_calls = 0
function advance_probe()
    assert(probe_world.pose == probe_alias and probe_alias.self == probe_alias)
    probe_world.calls = probe_world.calls + 1
    probe_alias.y = probe_alias.y + probe_alias.gravity
    probe_alias.gravity = math.min(probe_alias.gravity + .8, 8)
    if probe_alias.y > 160 then
        probe_alias.y = 160
        probe_alias.gravity = -8
    end
    probe_world.body:y(probe_alias.y)
    helper_calls = helper_calls + 1
    probe_world.witness:x(probe_world.calls):y(math.random(1, 4)):z(helper_calls)
end
return Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            if not started and beat >= 1 then
                started = true
                MESSAGEMAN:Broadcast("StartFall")
                probe_world.running = true
            end
            if beat >= 4 then probe_world.running = false end
        end)
    end,
    Def.ActorFrame {
        Name = "Scene",
        OnCommand = function(self) self:visible(false) end,
        StartFallMessageCommand = function(self) self:visible(true):playcommand("Start") end,
        Def.Actor {
            StartCommand = function(self)
                probe_world.starts = probe_world.starts + 1
                self:sleep(.02):queuecommand("Update")
            end,
            UpdateCommand = function(self)
                advance_probe()
                self:sleep(.02)
                if probe_world.running then self:queuecommand("Update") end
            end,
        },
        Def.Quad {
            Name = "Falling",
            InitCommand = function(self) probe_world.body = self; self:zoomto(16, 16):y(100) end,
        },
        Def.Quad {
            Name = "Witness",
            InitCommand = function(self) probe_world.witness = self; self:zoomto(8, 8):x(100) end,
        },
    },
}
