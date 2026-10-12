-- Port of benchmarks/vm/push_records.fg
local function build(n)
  local recs = {}
  for i = 0, n - 1 do
    recs[#recs + 1] = { id = i, name = "r" .. tostring(i) }
  end
  return recs
end
print(#build(100000))
