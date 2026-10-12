-- Port of benchmarks/vm/word_count.fg
local words = {}
local seed = 42
for i = 0, 99999 do
  seed = (seed * 75 + 74) % 65537
  words[#words + 1] = "w" .. tostring(seed % 5000)
end
local counts = {}
local distinct = 0
for _, w in ipairs(words) do
  local c = counts[w]
  if c == nil then
    distinct = distinct + 1
    c = 0
  end
  counts[w] = c + 1
end
print(distinct)
