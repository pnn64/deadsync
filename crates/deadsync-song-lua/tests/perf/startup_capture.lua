-- Shared locals, cyclic tables, queued commands and function actions exercise
-- startup snapshot/restore without models, textures or external assets.
local state = { count = 0, nodes = {} }
for i = 1, 64 do
    state.nodes[i] = { value = i, label = 'node' .. i, values = { i, i * 2, i * 3 } }
end
state.self = state
for i = 1, 64 do state.nodes[i].next = state.nodes[i % 64 + 1] end

local root = Def.ActorFrame { Name = 'CaptureRoot' }
for i = 1, 8 do
    root[#root + 1] = Def.Quad {
        Name = 'Capture' .. i,
        OnCommand = function(self) self:zoomto(8, 8):queuecommand('Pulse') end,
        PulseCommand = function(self)
            state.count = state.count + 1
            state.nodes[i].value = state.nodes[i].value + state.count
            self:linear(.125):x(state.nodes[i].value):queuecommand('Tail')
        end,
        TailCommand = function(self)
            self:y(state.nodes[i].values[2]):diffusealpha(.75)
        end,
    }
end
mod_actions = {}
for i = 1, 8 do
    mod_actions[i] = { i / 8, function()
        state.count = state.count + 1
        state.nodes[i].values[1] = state.count
        root:GetChild('Capture' .. i):rotationz(state.nodes[i].values[1])
    end, true }
end
return root
