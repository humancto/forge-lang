# Peer-language reference programs

Line-for-line ports of `benchmarks/vm/*.fg` to Python, Node.js and Lua,
run by `tools/bench.sh --suite peers` for the comparison in
`docs/BENCHMARKS.md`. Keep each port the same algorithm as its Forge
original (same loop shape, same sizes, same printed result). Do not
replace it with the idiomatic fast path, such as `sum(range(n))` or
`"x" * n`.
