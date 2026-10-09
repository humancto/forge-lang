-- Port of benchmarks/vm/repeat_loop.fg
local total = 0
for _ = 1, 5000000 do
  total = total + 3
end
print(total)
