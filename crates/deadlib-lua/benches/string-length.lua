-- String lengths inside a tight unhooked VM loop.
local bytes = string.rep('abcdefgh', 16)
local total = 0
for i = 1, 2000000 do
  total = total + #bytes
end
assert(total == 256000000)
return total
