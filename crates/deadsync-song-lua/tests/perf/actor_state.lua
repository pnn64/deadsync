-- Offline message capture snapshots/restores the receiver, touched actors,
-- wrapper membership, closure locals and scalar globals for each handler.
local actors = {}
local root = Def.ActorFrame { Name = 'SnapshotMessages' }
for i = 1, 16 do
    local actor = Def.Quad {
        Name = 'Snapshot' .. i,
        InitCommand = function(self)
            actors[i] = self
            self:xy(i, -i):zoomto(8, 8)
            self:AddWrapperState():rotationz(i)
        end,
    }
    actor['Snapshot' .. i .. 'MessageCommand'] = function(self)
        self:addx(i):diffusealpha(.5)
        actors[i % 16 + 1]:addy(i)
    end
    root[#root + 1] = actor
end
mod_actions = {{.25, 'Snapshot1', true}, {.5, 'Snapshot8', true}}
return root
