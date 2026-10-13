-- Port of benchmarks/vm/map_build.fg
local n = 100000
local m = {}
local count = 0
for i = 0, n - 1 do
  m["k" .. tostring(i)] = i
  count = count + 1
end
local total = 0
for j = 0, n - 1 do
  local v = m["k" .. tostring(j)]
  if v ~= nil then total = total + v end
end
print(count)
print(string.format("%d", total))
