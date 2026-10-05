-- Port of benchmarks/vm/array_push.fg
local function fill(n)
  local a = {}
  local i = 0
  while i < n do
    a[#a + 1] = i
    i = i + 1
  end
  return a
end
print(#fill(100000))
