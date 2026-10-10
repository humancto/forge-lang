-- Port of benchmarks/vm/toplevel_loop_fn.fg
local function run()
    local i = 0
    local total = 0
    while i < 5000000 do
        total = total + i
        i = i + 1
    end
    return total
end
print(run())
