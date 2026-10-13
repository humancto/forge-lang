-- Port of benchmarks/vm/object_build.fg
local function build(n)
  local ob = {}
  local count = 0
  for i = 0, n - 1 do
    ob["k" .. tostring(i)] = i
    count = count + 1
  end
  return ob, count
end
local _, count = build(50000)
print(count)
