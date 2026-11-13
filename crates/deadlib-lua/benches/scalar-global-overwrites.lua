-- Rotating existing string keys defeats a two-entry write hint.
-- Only numeric values are written, so the write barrier is unnecessary.
a, b, c, d = 0, 0, 0, 0
local count = 800000
for i = 1, count do
  a = i
  b = i + 1
  c = i + 2
  d = i + 3
end
assert(a == count and b == count + 1 and c == count + 2 and d == count + 3)
return a + b + c + d
