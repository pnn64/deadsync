-- Function eases snapshot selected actors; actions collect touched actors and sounds.
local actors = {}
local linear = function(t, b, c, d) return b + c * t / d end
prefix_globals = {ease = {}, actions = {}}
local root = Def.ActorFrame {
    Name = 'CaptureInputs',
    InitCommand = function(self)
        for i = 1, 8 do
            prefix_globals.ease[i] = {i / 16, 0.125, 0, i,
                function(value)
                    for j = 1, 32 do actors[j]:x(value) end
                end, 'len', linear}
        end
        prefix_globals.actions[1] = {0.5, function()
            for j = 1, 16 do actors[j]:y(j * 3) end
            for j = 1, 32 do SOUND:PlayOnce('asset-string.wav') end
        end, true}
    end,
}
for i = 1, 32 do
    root[#root + 1] = Def.Quad {
        Name = 'CaptureInput' .. i,
        InitCommand = function(self)
            actors[i] = self
            self:xy(i, -i):zoomto(4, 8)
        end,
    }
end
return root
