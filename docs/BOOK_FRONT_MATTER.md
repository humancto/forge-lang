---
title: "Programming Forge"
subtitle: "The Internet-Native Language That Reads Like English"
author: "Archith Rapaka"
edition: "First Edition"
version: "0.2.0"
year: "2026"
publisher: "Self-Published"
lang: "en"
toc: true
toc-depth: 3
numbersections: true
geometry: "margin=1in"
fontsize: 11pt
mainfont: "Georgia"
monofont: "Menlo"
linkcolor: "blue"
cover_description: "A glowing anvil in a digital forge, sparks flying as molten code streams pour from it, forming HTTP requests, database queries, and terminal UI elements — all set against a dark navy background with subtle circuit-board patterns. The anvil sits on a workbench made of keyboard keys. The sparks form recognizable code symbols: curly braces, arrows, pipes. Color palette: deep navy (#0a192f), forge orange (#ff6b35), electric blue (#64ffda), white spark highlights."
---

\newpage

# Programming Forge

**The Internet-Native Language That Reads Like English**

*By Archith Rapaka*

*First Edition — February 2026*

---

Copyright (c) 2026 Archith Rapaka. All rights reserved.

Published under the MIT License.

Forge is an open-source programming language. Visit https://github.com/forge-lang/forge for source code and community.

*While every precaution has been taken in the preparation of this book, the author assumes no responsibility for errors or omissions, or for damages resulting from the use of the information contained herein.*

\newpage

## Preface

I built Forge because I was tired of installing thirty packages to do what a programming language should do out of the box.

Every modern application talks to HTTP endpoints, reads from databases, handles JSON, and hashes passwords. Yet in every mainstream language, these are third-party concerns. You pip-install a web framework. You npm-install a database driver. You go-get a crypto library. You wrestle with dependency conflicts, version mismatches, and supply chain vulnerabilities — all before writing a single line of your actual application.

Forge takes a different approach. HTTP, databases, cryptography, file I/O, regular expressions, CSV parsing, terminal UI — they're all built into the language itself. Not as a bloated standard library you have to import, but as primitives that are always available, always documented, and always tested.

The result is a language where a REST API server is 10 lines of code. Where querying a database is a single function call. Where hashing a password doesn't require reading a third-party library's documentation.

Forge also reads like English. You can write `say "hello"` or `println("hello")` — both work. You can define functions with `fn` or `define`. You can write `else` or `otherwise` or even `nah`. The language doesn't force a style on you. It meets you where you are.

This book is organized in four parts:

- **Part I: Foundations** covers installation, syntax, control flow, functions, collections, and error handling — everything you need to read and write Forge.
- **Part II: The Standard Library** documents all 15 built-in modules, function by function, with recipes for common tasks.
- **Part III: Building Real Things** walks through complete projects — REST APIs, data pipelines, DevOps scripts, and AI integration.
- **Part IV: Under the Hood** explains how Forge works internally — the lexer, parser, interpreter, bytecode VM, and toolchain.

Whether you're a student writing your first program, a backend developer building APIs, or a language enthusiast curious about implementation — welcome to Forge.

*Archith Rapaka*
*February 2026*

\newpage

## About the Author

**Archith Rapaka** is a software engineer and programming language designer. He believes programming languages should be as natural to read as they are powerful to write, and that the tools developers use most — HTTP, databases, cryptography — belong in the language itself, not in package registries. Forge is the embodiment of this philosophy.

\newpage

## How to Read This Book

**If you're new to programming:** Start with Part I, work through every "Try It Yourself" exercise, and run `forge learn` for interactive tutorials alongside the book.

**If you're an experienced developer:** Skim Part I for syntax differences, then dive into Part II (standard library) and Part III (projects) for practical usage.

**If you're a language implementer:** Skip directly to Part IV for the architecture, bytecode VM, and garbage collector internals.

**Conventions used in this book:**

- Code examples are shown in monospace blocks and can be saved as `.fg` files and run with `forge run filename.fg`
- Terminal commands are prefixed with `$`
- Output is shown after commands or as comments
- Tips and important notes are formatted as blockquotes

\newpage

