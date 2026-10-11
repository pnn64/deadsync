local order, ready = {}, false
local base = Def.Quad{
    Name = 'Merged',
    InitCommand = function(self)
        ready = true
        order[#order + 1] = 'base'
        self:x(19)
    end,
    Callback = function(self, value, tail)
        assert(value == 7 and tail == nil)
        return 'discarded'
    end,
    11, 22,
}
local extension = {
    InitCommand = function(self)
        assert(ready)
        order[#order + 1] = 'extension'
        self:addx(23)
    end,
    Callback = function(self, value, tail)
        assert(value == 7 and tail == nil)
        return 'result', nil, 31
    end,
    33,
}
local merged = base .. extension
assert(merged ~= base and merged ~= extension)
assert(getmetatable(merged) == getmetatable(base))
assert(base[1] == 11 and base[2] == 22)
assert(merged[1] == 33 and merged[2] == 22 and merged[3] == nil)
local a, b, c = merged.Callback(merged, 7, nil)
assert(a == 'result' and b == nil and c == 31)
merged.OnCommand = function(self)
    assert(ready and table.concat(order, ',') == 'base,extension')
    assert(self:GetX() == 42)
end
return Def.ActorFrame{merged}
