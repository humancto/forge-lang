// The guided tour: short lessons adapted from `forge learn` (src/learn.rs)
// for the browser. Every lesson must run cleanly on both engines; lessons
// with `expect` must print exactly that. bindings/wasm/tests/smoke.mjs
// checks both in CI.
//
// `body` is trusted HTML written here (never user input).

export const TOUR = [
  {
    title: "Hello, Forge",
    body: `<p>Welcome! Forge reads like English. <code>say</code> prints a line —
      and it has two siblings: <code>yell</code> (LOUD) and <code>whisper</code> (quiet).</p>
      <p>Press <kbd>Run</kbd> (or <kbd>Ctrl</kbd>/<kbd>⌘</kbd>+<kbd>Enter</kbd>) to run the code,
      then change it and run it again.</p>`,
    code: String.raw`say "Hello, World!"
yell "I'm loud!"
whisper "I'm QUIET"`,
    expect: "Hello, World!\nI'M LOUD!\ni'm quiet\n",
  },
  {
    title: "Variables",
    body: `<p>Create variables with <code>set … to …</code> (or the classic <code>let x = …</code>).
      They are immutable unless you add <code>mut</code>; update a mutable one with
      <code>change … to …</code>.</p>
      <p>Strings interpolate anything in <code>{braces}</code>.</p>`,
    code: String.raw`set name to "Forge"
set mut score to 0
change score to score + 10
let bonus = 5
say "Welcome to {name}! Score: {score + bonus}"`,
    expect: "Welcome to Forge! Score: 15\n",
  },
  {
    title: "Functions",
    body: `<p>Define functions with <code>define</code> or <code>fn</code>. Both mean the same thing —
      pick the style that reads best.</p>`,
    code: String.raw`define greet(name) {
    return "Hello, {name}!"
}

fn square(x) { return x * x }

say greet("Developer")
say square(12)`,
    expect: "Hello, Developer!\n144\n",
  },
  {
    title: "Lists and loops",
    body: `<p>Arrays use <code>[…]</code>. Loop over them with <code>for each … in …</code>,
      or repeat a block a fixed number of times with <code>repeat N times</code>.</p>`,
    code: String.raw`set colors to ["red", "green", "blue"]
for each color in colors {
    say color
}

set mut count to 0
repeat 3 times {
    change count to count + 1
}
say "Counted to {count}"`,
    expect: "red\ngreen\nblue\nCounted to 3\n",
  },
  {
    title: "Objects and destructuring",
    body: `<p>Objects are JSON-shaped: <code>{ key: value }</code>. Read fields with a dot, and pull
      several out at once with <code>unpack</code>.</p>`,
    code: String.raw`set user to { name: "Alice", age: 30, langs: ["Forge", "Rust"] }
say user.name
unpack { name, age } from user
say "{name} is {age}"
unpack [first, ...rest] from user.langs
say "Favourite: {first}, also: {rest}"`,
    expect: "Alice\nAlice is 30\nFavourite: Forge, also: [Rust]\n",
  },
  {
    title: "When guards",
    body: `<p><code>when</code> picks the first arm whose comparison matches — a tidy
      replacement for a chain of <code>if</code>/<code>else</code>. It is an expression,
      so you can assign its result.</p>`,
    code: String.raw`define stage(age) {
    return when age {
        < 13 -> "kid",
        < 20 -> "teen",
        < 65 -> "adult",
        else -> "senior"
    }
}
for each age in [8, 16, 42, 80] {
    say "{age}: {stage(age)}"
}`,
    expect: "8: kid\n16: teen\n42: adult\n80: senior\n",
  },
  {
    title: "Handling errors",
    body: `<p>Errors are values you can catch: <code>try</code>/<code>catch</code> gives you the
      error's <code>message</code> and <code>type</code>; <code>safe { … }</code> simply swallows
      failures. Runtime errors you do not catch stop the program and point at the line.</p>`,
    code: String.raw`try {
    let ratio = 10 / 0
} catch err {
    say "Caught {err.type}"
}

safe {
    let boom = 1 / 0
}
say "Still running!"`,
    expect: "Caught ArithmeticError\nStill running!\n",
  },
  {
    title: "Transforming data",
    body: `<p><code>map</code>, <code>filter</code> and <code>reduce</code> take a function — write one
      inline with <code>fn(x) { … }</code>.</p>`,
    code: String.raw`let people = [
    { name: "Alice", age: 30 },
    { name: "Bob", age: 17 },
    { name: "Charlie", age: 25 }
]
let adults = filter(people, fn(p) { return p.age >= 18 })
let names = map(adults, fn(p) { return p.name })
say join(names, ", ")
say reduce([1, 2, 3, 4], 0, fn(total, n) { return total + n })`,
    expect: "Alice, Charlie\n10\n",
  },
  {
    title: "Types and pattern matching",
    body: `<p>Define your own algebraic types with <code>type</code> and branch on them with
      <code>match</code>. Each variant can carry data.</p>`,
    code: String.raw`type Shape = Circle(Float) | Square(Float)

define area(shape) {
    match shape {
        Circle(r) => return 3.0 * r * r
        Square(side) => return side * side
    }
}
say area(Circle(2.0))
say area(Square(4.0))`,
    expect: "12\n16\n",
  },
  {
    title: "Result and Option",
    body: `<p>Instead of <code>null</code> surprises, Forge has <code>Ok</code>/<code>Err</code> and
      <code>Some</code>/<code>None</code>, with helpers to unwrap them safely.</p>`,
    code: String.raw`define parse_age(text) {
    let n = int(text)
    if n < 0 { return Err("age can't be negative") }
    return Ok(n)
}
let good = parse_age("42")
let bad = parse_age("-1")
say "good ok? {is_ok(good)} -> {unwrap(good)}"
say "bad: {unwrap_or(bad, 0)}"

let nickname = None
say unwrap_or(nickname, "no nickname")`,
    expect: "good ok? true -> 42\nbad: 0\nno nickname\n",
  },
  {
    title: "Strings",
    body: `<p>A big toolbox of string helpers is built in — no imports — plus
      <code>regex</code> for patterns (note the order: text first, then pattern).</p>`,
    code: String.raw`let title_text = "the quick brown fox"
say title(title_text)
say slugify("Hello World! 2024")
say snake_case("myAPIKey")
say pad_start("42", 6, "0")
say split("a,b,c", ",")
say regex.find("Order #42 shipped", "\\d+")`,
    expect: "The Quick Brown Fox\nhello-world-2024\nmy_api_key\n000042\n[a, b, c]\n42\n",
  },
  {
    title: "Collection power tools",
    body: `<p>Summaries, grouping and chunking are one call away.</p>`,
    code: String.raw`let nums = [1, 2, 3, 4, 5, 6]
say "Sum: {sum(nums)}, max: {max_of(nums)}"
say "Any over 5? {any(nums, fn(x) { return x > 5 })}"
say "Chunks: {chunk(nums, 2)}"
let groups = group_by(nums, fn(x) {
    if x % 2 == 0 { return "even" }
    return "odd"
})
say "Even: {groups.even}, odd: {groups.odd}"`,
    expect: "Sum: 21, max: 6\nAny over 5? true\nChunks: [[1, 2], [3, 4], [5, 6]]\nEven: [2, 4, 6], odd: [1, 3, 5]\n",
  },
  {
    title: "The GenZ debug kit",
    body: `<p>Debugging with attitude: <code>sus()</code> inspects a value (on stderr) and passes
      it through, <code>bet()</code> and <code>no_cap()</code> assert, and <code>yolo()</code>
      turns any error into <code>None</code>.</p>`,
    code: String.raw`let x = sus(42)
bet(x == 42, "math is broken")
no_cap(len("hello"), 5)
let result = yolo(fn() {
    let oops = 1 / 0
    return "never"
})
say "x is {x}, yolo gave {result}"`,
    expect: "x is 42, yolo gave None\n",
  },
  {
    title: "Fake data and colors",
    body: `<p><code>npc</code> generates realistic fake data for tests and demos, and
      <code>term</code> adds color. Run it a few times — the data changes.</p>`,
    code: String.raw`say term.bold("Today's guest list")
repeat 3 times {
    say "{npc.name()} <{npc.email()}> from {npc.company()}"
}
say term.green("Dice roll: {npc.number(1, 6)}")`,
  },
  {
    title: "Where to go next",
    body: `<p>The playground runs the Forge core in your browser. The parts that need an
      operating system — <code>http</code>, files (<code>fs</code>), databases, shell commands,
      <code>spawn</code>/<code>squad</code> concurrency and <code>@server</code> APIs — say
      <em>not available in the browser playground</em> here. Run this to see:</p>
      <p>Install the CLI to use them all, and run <code>forge learn</code> for 30 more lessons.</p>`,
    code: String.raw`try {
    let page = http.get("https://example.com")
} catch err {
    say err.message
}`,
    expect:
      "`http.get` is not available in the browser playground (install the Forge CLI to use it)\n",
  },
];
