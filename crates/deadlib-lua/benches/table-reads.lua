-- Repeated reads of existing string-keyed fields in the normal interpreter.
local table = {a = 1, b = 2, c = 3, d = 4}
local total = 0
for i = 1, 2000000 do
  total = total + table.a + table.b + table.c + table.d
end
assert(total == 20000000)
return total
