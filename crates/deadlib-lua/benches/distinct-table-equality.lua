-- Distinct table comparisons without metatables should resolve by identity.
-- Run with the release official_suite example in rust-bench mode.
local count = 1000000
local a, b, c, d = {}, {}, {}, {}
local score = 0
for i = 1, count do
  if a ~= b then score = score + 1 end
  if b ~= c then score = score + 1 end
  if c ~= d then score = score + 1 end
  if d == a then score = score + 100 end
end
assert(score == count * 3)
return score
