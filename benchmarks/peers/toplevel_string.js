// Port of benchmarks/vm/toplevel_string.fg
let s = "";
let i = 0;
while (i < 200000) {
    s = s + "x";
    i = i + 1;
}
console.log(s.length);
