local before, after
local witnesses = {}
local function witness(index)
    return Def.Quad {
        Name = "Read" .. index,
        InitCommand = function(self)
            witnesses[index] = self
            self:zoomto(8, 8):y(240 + index * 20)
        end,
    }
end
local function follow(actor, beat)
    local y = math.max(3.5 - beat, 0) * 64
    if math.abs(actor:GetY() - y) < 20 then actor:linear(.02) end
    actor:x(0):diffusealpha(1):y(y):z(0):zoom(1):addy(0)
end
return Def.ActorFrame {
    Def.Quad {
        Name = "Before",
        InitCommand = function(self)
            before = self
            self:zoomto(16, 16):x(100):y(192)
        end,
    },
    Def.Actor {
        OnCommand = function(self) self:sleep(.5):queuecommand("Start") end,
        StartCommand = function(self) self:sleep(.02):queuecommand("Update") end,
        UpdateCommand = function(self)
            local beat = GAMESTATE:GetSongBeat()
            witnesses[1]:x(before:GetY())
            follow(before, beat)
            witnesses[2]:x(before:GetY())
            witnesses[3]:x(after:GetY())
            follow(after, beat)
            witnesses[4]:x(after:GetY())
            self:sleep(.02)
            if beat < 3.5 then self:queuecommand("Update") end
        end,
    },
    Def.Quad {
        Name = "After",
        InitCommand = function(self)
            after = self
            self:zoomto(16, 16):x(200):y(192)
        end,
    },
    witness(1), witness(2), witness(3), witness(4),
}
