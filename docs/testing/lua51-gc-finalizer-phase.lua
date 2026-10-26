-- Regression: finish pending finalization without starting another mark/sweep.
-- Expected tab-separated output: true false true false.
collectgarbage('restart')
collectgarbage('setpause', 200)
collectgarbage()
collectgarbage('stop')
local weak = setmetatable({}, {__mode = 'v'})
local keep = {}
for i = 1, 1000 do
  local t = {}
  weak[i] = t
  keep[i] = t
end
local tag = {}
local hit = false
local p = newproxy(true)
getmetatable(p).__gc = function()
  hit = true
  collectgarbage('setpause', 2000000000)
  error(tag, 0)
end
p = nil
local ok, e = pcall(collectgarbage)
keep = nil
for i = 1, 10000 do local t = {} end
print(hit, ok, e == tag, next(weak) == nil)
