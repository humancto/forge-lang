# Lexical Structure

This chapter defines the lexical grammar of Forge: how source text is decomposed into a sequence of tokens. The lexer (tokenizer) reads UTF-8 encoded source text and produces a flat stream of tokens that the parser consumes.

## Overview

A Forge source file is a sequence of Unicode characters encoded as UTF-8. The lexer processes this character stream left-to-right, greedily matching the longest valid token at each position. The resulting token stream consists of:

- **Keywords** — reserved words with special meaning (e.g., `let`, `fn`, `set`, `define`)
- **Identifiers** — user-defined names for variables, functions, types, and fields
- **Literals** — integer, float, string, boolean, and null values
- **Operators** — arithmetic, comparison, logical, assignment, and special operators
- **Punctuation** — delimiters and separators (`(`, `)`, `{`, `}`, `[`, `]`, `,`, `:`, `;`)
- **Comments** — line comments and block comments, discarded during tokenization
- **Newlines** — significant for statement termination
- **Decorators** — `@` prefixed annotations

## Whitespace and Line Termination

Spaces (U+0020) and horizontal tabs (U+0009) are whitespace characters. They separate tokens but are otherwise insignificant and are not included in the token stream.

Newline characters (U+000A, and the sequence U+000D U+000A) serve as statement terminators. Unlike whitespace, newlines are emitted as `Newline` tokens because Forge uses newlines (rather than semicolons) to separate statements. Semicolons (`;`) are recognized as explicit statement terminators but are not required.

## Tokenization Order

When the lexer encounters a character sequence, it applies the following precedence:

1. Skip whitespace (spaces and tabs).
2. If the character begins a comment (`//` or `/*`), consume the entire comment.
3. If the character is a newline, emit a `Newline` token.
4. If the character is a digit, lex a numeric literal (integer or float).
5. If the character is `"`, lex a string literal (or `"""` for raw strings).
6. If the character is a letter or underscore, lex an identifier or keyword.
7. Otherwise, lex an operator or punctuation token.

Each token carries a span consisting of the line number, column number, byte offset, and byte length within the source text.

## Subsections

The following subsections define each lexical element in detail:

- [Source Text](./lexical-structure/source-text.md) — encoding, file extension, line endings
- [Keywords](./lexical-structure/keywords.md) — complete keyword list with dual-syntax mappings
- [Identifiers](./lexical-structure/identifiers.md) — naming rules and special identifiers
- [Literals](./lexical-structure/literals.md) — numeric, string, boolean, null, array, and object literals
- [Operators and Punctuation](./lexical-structure/operators.md) — all operator and delimiter tokens
- [Comments](./lexical-structure/comments.md) — line and block comment syntax
