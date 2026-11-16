-- Repeated numeric math.min/max calls in an unobserved interpreter loop.
local min, max = math.min, math.max
local total = 0
for i = 1, 1000000 do
  total = total + min(i, 800000) + max(i, 800000)
end
assert(total == 1300000500000)
return total
