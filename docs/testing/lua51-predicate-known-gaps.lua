-- Retained observations of open debug/native-export compatibility gaps.
local function events(f, ...)
  local count = 0
  debug.sethook(function()
    if debug.getinfo(2, 'f').func == f then count = count + 1 end
  end, '', 1)
  local result = f(...)
  debug.sethook()
  return result, count
end

local function f(a, b)
  local n = 0
  while n < 3 and (a or b) do
    n = n + 1
    if n == 2 then break end
  end
  return n
end
print('source', events(f, false, true))
print('exported', events(assert(loadstring(string.dump(f))), false, true))

local function nil_condition()
  if nil then return 0 end
  return 42
end
local slots = {}
debug.sethook(function()
  if debug.getinfo(2, 'f').func == nil_condition then
    local name, value = debug.getlocal(2, 1)
    slots[#slots + 1] = tostring(name)..':'..type(value)..':'..tostring(value)
  end
end, '', 1)
local value = nil_condition()
debug.sethook()
print('slots', value, table.concat(slots, '|'))
