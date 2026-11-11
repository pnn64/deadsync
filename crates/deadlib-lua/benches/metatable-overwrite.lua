-- Repeatedly overwrite an existing table key with __newindex installed.
-- The handler must never be called for a non-nil raw value.
local count = 1200000
local t = setmetatable({value = 0}, {
  __newindex = function() error('unexpected __newindex call') end
})
for i = 1, count do
  t.value = i
end
assert(t.value == count)
return t.value
