-- Port of benchmarks/vm/toplevel_captured.fg
local total = 0
local function bump(n)
    total = total + n
end
local i = 0
while i < 2000000 do
    bump(i)
    i = i + 1
end
print(total)
