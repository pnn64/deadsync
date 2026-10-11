local values = setmetatable({11, [3]=30}, {
    __index = function(_, index) if index == 2 then return 20 end end,
})
assert(select('#', ivalues(values)) == 1)
local iterator = ivalues(values)
values[1] = 10
assert(iterator('ignored', false) == 10)
assert(iterator() == 20 and iterator() == 30 and iterator() == nil)
values[5] = 50
assert(iterator() == 50)
local ok, delayed = pcall(ivalues, nil)
assert(ok and type(delayed) == 'function' and not pcall(delayed))
mod_actions = {{1, 'iterator native', true}}
