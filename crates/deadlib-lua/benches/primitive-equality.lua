-- Repeated primitive comparisons in the ordinary, unobserved Lua interpreter.
-- Run with the release official_suite example in rust-bench mode.
local count = 1000000
local absent = nil
local enabled = true
local name = 'deadsync'
local same = 'deadsync'
local different = 'deadlua'
local table_value = {}
local score = 0
for i = 1, count do
  if absent == nil then score = score + 1 end
  if enabled ~= false then score = score + 1 end
  if name == same then score = score + 1 end
  if name ~= different then score = score + 1 end
  if table_value == table_value then score = score + 1 end
end
assert(score == count * 5)
return score
