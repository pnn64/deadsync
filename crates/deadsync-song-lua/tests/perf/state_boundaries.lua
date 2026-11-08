-- Shared upvalue cells and mutable table aliases are restored between probes.
local actors = {}
local counter = 0
local shared = {count=0}
local alias = shared
prefix_globals = {actions={}}
local root = Def.ActorFrame {
    Name = 'StateBoundaries',
    InitCommand = function()
        prefix_globals.actions[1] = {0.25, function()
            counter = counter + 1
            alias.count = alias.count + counter
            shared = {count=alias.count + 2}
            for i=1,8 do
                actors[i]:x(counter + shared.count):diffuse({-0.0, '0.5', 0.75})
            end
        end, true}
        prefix_globals.actions[2] = {0.5, function()
            counter = counter + 1
            alias.count = alias.count + counter
            for i=1,8 do
                actors[i]:y(alias.count):glow({0.25, 0.5, '0.75', 0.5})
            end
        end, true}
    end,
}
for i=1,8 do
    root[#root+1] = Def.Quad {
        Name = 'Boundary'..i,
        InitCommand = function(self)
            actors[i] = self
            self:xy(i,-i):zoomto(4,8):diffuse({0.25,0.5,0.75,1})
        end,
    }
end
return root
