-- Port of benchmarks/vm/stream_chain.fg (lazy iterator chain)
local xs = {}
for i = 0, 999999 do xs[#xs + 1] = i end
local function iter(t)
  local i = 0
  return function()
    i = i + 1
    return t[i]
  end
end
local function filter(it, f)
  return function()
    while true do
      local x = it()
      if x == nil or f(x) then return x end
    end
  end
end
local function map(it, f)
  return function()
    local x = it()
    if x == nil then return nil end
    return f(x)
  end
end
local function reduce(it, init, f)
  local acc = init
  for x in it do acc = f(acc, x) end
  return acc
end
local multiples = filter(iter(xs), function(x) return x % 3 == 0 end)
local doubled = map(multiples, function(x) return x * 2 end)
print(math.floor(reduce(doubled, 0, function(acc, x) return acc + x end)))
