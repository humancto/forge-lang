-- Port of benchmarks/vm/range_loop.fg
local total = 0
for i = 0, 5000000 - 1 do
  total = total + i
end
print(total)
