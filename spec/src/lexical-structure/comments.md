# Comments

Comments are annotations in source code intended for human readers. The lexer recognizes comments and discards them; they do not appear in the token stream and have no effect on program behavior.

Forge supports two forms of comments: line comments and block comments.

## Line Comments

> _LineComment_ → `//` _Character_\* _Newline_

A line comment begins with `//` and extends to the end of the line (the next newline character or end of file). Everything after `//` on that line is ignored by the lexer.

```forge
// This is a line comment
let x = 42  // This is an inline comment
```

Line comments may appear on their own line or at the end of a line containing code.

## Block Comments

> _BlockComment_ → `/*` _Character_\* `*/`

A block comment begins with `/*` and ends with the next occurrence of `*/`. Block comments may span multiple lines.

```forge
/* This is a block comment */

/*
  This block comment
  spans multiple lines.
*/

let x = /* inline block comment */ 42
```

Block comments do **not** nest. The first `*/` encountered after a `/*` ends the comment, regardless of any intervening `/*` sequences:

```forge
/* outer /* inner */ this is NOT inside the comment */
```

In the example above, the comment ends at the first `*/`, and the text `this is NOT inside the comment */` would be parsed as code (and likely produce a syntax error).

## Doc Comments

Forge does not currently have a dedicated doc comment syntax (such as `///` or `/** */`). Documentation is written using regular line comments or block comments by convention.

## Comments in String Literals

Comment sequences (`//` and `/* */`) within string literals are part of the string content and are not treated as comments:

```forge
let url = "https://example.com"   // The // in the string is not a comment
let msg = "use /* carefully */"    // The /* */ in the string is not a comment
```

## Placement

Comments may appear anywhere that whitespace is permitted. They cannot appear inside tokens (e.g., in the middle of an identifier or numeric literal).
