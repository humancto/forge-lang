-- Port of benchmarks/vm/global_calls.fg
local function run(xs, s, n)
    local i = 0
    local total = 0
    while i < n do
        total = total + #xs + #s
        i = i + 1
    end
    return total
end
print(run({1, 2, 3}, "ab", 2000000))
