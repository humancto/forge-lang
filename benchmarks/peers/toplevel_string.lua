-- Port of benchmarks/vm/toplevel_string.fg
local s = ""
local i = 0
while i < 200000 do
    s = s .. "x"
    i = i + 1
end
print(#s)
