local pools = {{}, {}}
local cursor, nextSpawn, calls = 1, 1, 0
local driver
local function trail(group, slot)
    return Def.Quad {
        Name = "Trail" .. group .. slot,
        InitCommand = function(self)
            pools[group][slot] = self
            self:zoomto(32, 16):visible(false)
        end,
        HideCommand = function(self) self:visible(false):sleep(0):aux(0) end,
    }
end
return Def.ActorFrame {
    trail(1, 1), trail(1, 2), trail(1, 3),
    Def.Quad {
        Name = "Driver",
        InitCommand = function(self)
            driver = self
            self:queuecommand("Update")
        end,
        OnCommand = function(self) self:visible(false) end,
        UpdateCommand = function(self)
            local beat = GAMESTATE:GetSongBeat()
            calls = calls + 1
            self:x(calls)
            if beat >= nextSpawn then
                for group = 1, 2 do
                    local a = pools[group][cursor]
                    a:finishtweening():visible(true):diffuse(group / 2, .3, 1, .8)
                    a:zoom(1):x(group * 100 + cursor * 40):y(120):z(0)
                    a:linear(.3):addz(50):diffusealpha(0):zoom(-.1):queuecommand("Hide")
                end
                cursor = cursor % 3 + 1
                nextSpawn = nextSpawn + .1
            end
            if beat < 3 then
                self:sleep(.02):queuecommand("Update")
            else
                self:linear(.4):y(80):queuecommand("Done")
            end
        end,
        DoneCommand = function(self) self:aux(1000) end,
    },
    trail(2, 1), trail(2, 2), trail(2, 3),
    Def.Quad {
        Name = "Witness",
        InitCommand = function(self) self:zoomto(16, 16) end,
        OnCommand = function(self)
            self:SetUpdateFunction(function(actor)
                actor:x(driver:GetX()):y(driver:GetY())
            end)
        end,
    },
}
