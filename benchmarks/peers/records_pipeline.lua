-- Port of benchmarks/vm/records_pipeline.fg
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
local ids = {}
for i = 0, 299999 do ids[#ids + 1] = i end
local rows = map(ids, function(i)
  return { id = i, price = (i * 7919) % 1000, qty = (i * 31) % 7 }
end)
local big = filter(rows, function(r) return r.qty > 2 end)
local totals = map(big, function(r) return r.price * r.qty end)
print(#totals)
print(math.floor(reduce(totals, 0, function(acc, t) return acc + t end)))
