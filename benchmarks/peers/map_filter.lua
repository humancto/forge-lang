-- Port of benchmarks/vm/map_filter.fg
local function map(t, f)
  local out = {}
  for i = 1, #t do out[i] = f(t[i]) end
  return out
end
local function filter(t, f)
  local out = {}
  for i = 1, #t do
    if f(t[i]) then out[#out + 1] = t[i] end
  end
  return out
end
local function reduce(t, init, f)
  local acc = init
  for i = 1, #t do acc = f(acc, t[i]) end
  return acc
end
local xs = {}
for i = 0, 999999 do xs[#xs + 1] = i end
local ys = map(xs, function(x) return x * 2 end)
local zs = filter(ys, function(x) return x % 3 == 0 end)
print(#zs)
print(reduce(zs, 0, function(acc, x) return acc + x end))
