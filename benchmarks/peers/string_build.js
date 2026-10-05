// Port of benchmarks/vm/string_build.fg
function build(n) {
  let s = "";
  let i = 0;
  while (i < n) {
    s = s + "x";
    i = i + 1;
  }
  return s;
}
console.log(build(200000).length);
