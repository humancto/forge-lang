-- Port of benchmarks/vm/string_build.fg
local function build(n)
  local s = ""
  local i = 0
  while i < n do
    s = s .. "x"
    i = i + 1
  end
  return s
end
print(#build(200000))
