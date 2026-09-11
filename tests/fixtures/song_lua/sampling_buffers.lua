-- Many static overlays alongside sparse and dense updates exercise the
-- song compiler's full-state snapshots without external assets.
local tween, counter, message
local tick = 0
local started = false
mod_actions = {{0.75, "Pulse", true}, {1.25, "Pulse", true}}
local root = Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function()
            tick = tick + 1
            local beat = GAMESTATE:GetSongBeat()
            if not started and beat >= 0.25 then
                started = true
                tween:linear(1.5):x(120)
            end
            if beat <= 0.5 then counter:y(tick) end
        end)
    end,
    Def.Quad{
        Name="Tween",
        InitCommand=function(self) tween = self self:x(-10) end,
    },
    Def.Quad{
        Name="Counter",
        InitCommand=function(self) counter = self self:y(-20) end,
    },
    Def.Quad{
        Name="Message",
        InitCommand=function(self) message = self self:x(5) end,
        PulseMessageCommand=function(self) self:addx(7) end,
    },
}
for i = 1, 128 do
    root[#root + 1] = Def.Quad{
        Name="Static" .. i,
        InitCommand=function(self) self:xy(i, -i):zoom(1 + i / 128) end,
    }
end
return root
