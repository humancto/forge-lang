-- Port of benchmarks/vm/loop.fg
local function count(n)
  local i = 0
  local total = 0
  while i < n do
    total = total + i
    i = i + 1
  end
  return total
end
print(count(20000000))
