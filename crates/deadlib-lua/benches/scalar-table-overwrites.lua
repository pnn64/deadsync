-- Repeated writes to existing table fields through the ordinary interpreter.
local t = {a = 0, b = 0, c = 0, d = 0}
local count = 800000
for i = 1, count do
  t.a = i
  t.b = i + 1
  t.c = i + 2
  t.d = i + 3
end
assert(t.a == count and t.b == count + 1 and t.c == count + 2 and t.d == count + 3)
return t.a + t.b + t.c + t.d
