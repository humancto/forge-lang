# Source Text

## Encoding

Forge source files must be encoded in **UTF-8**. The lexer assumes UTF-8 input and will produce errors on invalid byte sequences. No byte-order mark (BOM) is required or expected; if present, it is treated as ordinary content and will likely cause a parse error.

## File Extension

Forge source files use the `.fg` file extension by convention. The CLI tools (`forge run`, `forge test`, `forge fmt`) expect this extension. Example:

```
hello.fg
server.fg
tests/math_test.fg
```

## Line Endings

Forge recognizes two line ending sequences:

| Sequence | Name                        | Unicode       |
| -------- | --------------------------- | ------------- |
| `\n`     | Line feed                   | U+000A        |
| `\r\n`   | Carriage return + line feed | U+000D U+000A |

A bare carriage return (`\r` without a following `\n`) is not treated as a line ending. Both recognized forms are normalized to a single `Newline` token in the token stream.

## Source Structure

A Forge source file consists of a sequence of top-level statements executed in order. There is no required `main` function, module declaration, or package header. The simplest valid Forge program is:

```forge
say "hello"
```

Forge programs are executed from the first statement to the last, top to bottom. Functions and type definitions are hoisted conceptually in that they can be referenced before their textual position, but side effects in top-level statements execute in source order.

## Character Set

Within string literals, Forge supports the full Unicode character set. Outside of string literals, the following characters are meaningful to the lexer:

- ASCII letters (`a`-`z`, `A`-`Z`) and underscore (`_`) begin identifiers and keywords.
- ASCII digits (`0`-`9`) begin numeric literals.
- Operator and punctuation characters: `+`, `-`, `*`, `/`, `%`, `=`, `!`, `<`, `>`, `&`, `|`, `.`, `,`, `:`, `;`, `(`, `)`, `{`, `}`, `[`, `]`, `@`, `?`, `#`.
- The double-quote character (`"`) begins string literals.
- Whitespace characters: space (U+0020), horizontal tab (U+0009).
- Line terminators: line feed (U+000A), carriage return (U+000D).

All other characters outside of string literals are lexer errors.
