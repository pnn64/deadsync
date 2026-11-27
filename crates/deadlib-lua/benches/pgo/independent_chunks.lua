local patterns = {
  [[local low = %d
local high = %d
local accepted = low < high and sample(low)
local rejected = high < low or not sample(high)
if accepted then result = high else result = low end
if rejected then result = low end
assert(accepted and not rejected)]],
  [[local candidate = %d
local ceiling = %d
local checked = sample(candidate) and not (candidate > ceiling)
local accepted = checked and (candidate ~= ceiling)
if not accepted then result = candidate else result = ceiling end
assert(accepted and result == ceiling)]],
  [[local lower = %d
local upper = %d
local nonempty = lower ~= upper
local result_value
while nonempty and sample(upper) do result_value = upper; break end
repeat if result_value == upper then break end until not nonempty
result = result_value
assert(result == upper)]],
  [[local floor = %d
local ceiling = %d
local eligible = (floor < ceiling and sample(floor)) == true
do
  local backup = ceiling
  if not eligible then backup = floor end
  result = backup
end
assert(eligible and result == ceiling)]],
}

function sample(value)
  return value > 0
end

local count = 0
for row = 1, 2048 do
  for j = 1, #patterns do
    local low = row * 4 + j
    local high = low + j
    local input = string.format(patterns[j], low, high)
    assert(loadstring(input, '=selection'))()
    assert(result == high)
    count = count + 1
  end
end
assert(count == 8192)
print('compiled 8192 independent chunks')
