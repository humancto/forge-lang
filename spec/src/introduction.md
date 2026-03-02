# Introduction

This document is the formal specification for the **Forge** programming language, version **0.3.3**.

Forge is an internet-native, general-purpose programming language implemented in Rust. It is designed for application-layer work: web services, scripts, data pipelines, prototypes, and tooling. Forge ships with built-in support for HTTP clients and servers, databases (SQLite and PostgreSQL), cryptography, JSON, CSV, terminal UI, AI/LLM integration, and more — eliminating the need for third-party packages for common internet-oriented tasks.

## Design Philosophy

Forge is guided by three core principles:

1. **Internet-native.** The operations developers perform most frequently — HTTP requests, database queries, JSON parsing, cryptographic hashing — are built into the language and its standard library. A REST API server is four lines. A database query is two.

2. **Dual syntax.** Every construct in Forge has two spellings: a _classic syntax_ familiar to developers who have written JavaScript, Python, Rust, or Go; and a _natural-language syntax_ that reads like English prose. Both forms compile to identical internal representations and may be mixed freely within the same source file.

3. **Batteries included.** Forge ships 16 standard library modules with over 230 functions, 30 interactive tutorials, a built-in test runner, a formatter, an LSP server, and a project scaffolding tool. A single `cargo install` provides a complete development environment.

## Language Overview

Forge is dynamically typed at runtime with optional type annotations (gradual typing). It supports first-class functions, closures, algebraic data types, pattern matching, Result/Option error handling, structural interfaces, composition via delegation, async/await concurrency, and channels. Programs are executed top-to-bottom without requiring a `main` function.

The default execution engine is a tree-walking interpreter. A bytecode VM (`--vm`) and a JIT compiler (`--jit`) are available for performance-critical workloads but support fewer features.

## How to Read This Specification

This specification is organized into five parts:

- **Part I: Language Core** covers lexical structure, types, expressions, statements, the type system, error handling, and concurrency.
- **Part II: Standard Library** documents each of the 16 built-in modules.
- **Part III: Built-in Functions** catalogs all globally available functions.
- **Part IV: Dual Syntax Reference** provides the complete mapping between classic and natural syntax forms.
- **Part V: Runtime and Internals** describes the interpreter, bytecode VM, JIT compiler, and HTTP server runtime.

Grammar rules are presented in block quotes using an informal EBNF notation. Code examples use `forge` syntax highlighting. Where both classic and natural syntax exist for a construct, both forms are shown.

## Notation Conventions

Throughout this specification:

- `monospace` text in prose refers to keywords, operators, or identifiers.
- Code blocks labeled `forge` contain valid Forge source code.
- Grammar productions use `→` for derivation, `|` for alternatives, `[ ]` for optional elements, and `{ }` for zero-or-more repetition.
- The term "implementation-defined" means the behavior is determined by the specific execution engine (interpreter, VM, or JIT).

## Links

- **Source code:** <https://github.com/humancto/forge-lang>
- **Website and book:** <https://humancto.github.io/forge-lang/>

## Credits

Forge was created by **Archith Rapaka / HumanCTO**.
