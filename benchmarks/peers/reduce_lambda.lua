-- Port of benchmarks/vm/reduce_lambda.fg
local function reduce(t, init, f)
  local acc = init
  for i = 1, #t do acc = f(acc, t[i]) end
  return acc
end
local xs = {}
for i = 0, 999999 do xs[#xs + 1] = i end
print(math.floor(reduce(xs, 0, function(acc, x) return (acc + x * x) % 1000003 end)))
