-- Port of benchmarks/vm/sort_comparator.fg
local function make(n)
  local xs = {}
  local seed = 7
  for i = 1, n do
    seed = (seed * 1103515 + 12345) % 2147483648
    xs[#xs + 1] = seed % 1000000
  end
  return xs
end
local xs = make(200000)
table.sort(xs, function(a, b) return a - b < 0 end)
print(math.floor(xs[1]))
print(math.floor(xs[100001]))
print(math.floor(xs[200000]))
