//! Type-checker diagnostics: stable codes, severities, spans and fixes.
//!
//! Codes are part of Forge's public surface (they appear in CLI output, in
//! the LSP and in the spec's type-checking chapter), so a code is never
//! reused for a different problem. Add new codes at the end.

use crate::parser::index::Span;
use std::fmt;

/// A stable diagnostic code (`T0001` ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Code {
    /// A value does not match the declared type of a variable, field or
    /// default value.
    TypeMismatch,
    /// An argument does not match the parameter's declared type.
    ArgumentType,
    /// A returned value does not match the function's declared return type.
    ReturnType,
    /// Wrong number of arguments.
    Arity,
    /// An operator is applied to operand types it does not support.
    InvalidOperator,
    /// A name that is not defined anywhere in scope.
    UnknownName,
    /// A field or method that a struct does not have.
    UnknownField,
    /// A member that a stdlib module does not have.
    UnknownMember,
    /// A function with a declared return type can finish without returning.
    MissingReturn,
    /// A statement that can never run.
    Unreachable,
    /// A `match` that does not cover every case of its subject's type.
    NonExhaustiveMatch,
    /// A type does not provide what an interface requires.
    InterfaceNotSatisfied,
    /// A call of a value that is not a function.
    NotCallable,
    /// An `Option` used where its inner value is needed.
    OptionMisuse,
    /// A type annotation names a type that does not exist.
    UnknownType,
    /// A struct literal leaves out a field that has no default.
    MissingField,
    /// Assignment to a binding declared without `mut`.
    ImmutableAssign,
    /// A name read before the statement that defines it has run.
    UseBeforeDefinition,
}

impl Code {
    /// Every code, in numbering order (`forge explain` lists them).
    pub const ALL: &'static [Code] = &[
        Code::TypeMismatch,
        Code::ArgumentType,
        Code::ReturnType,
        Code::Arity,
        Code::InvalidOperator,
        Code::UnknownName,
        Code::UnknownField,
        Code::UnknownMember,
        Code::MissingReturn,
        Code::Unreachable,
        Code::NonExhaustiveMatch,
        Code::InterfaceNotSatisfied,
        Code::NotCallable,
        Code::OptionMisuse,
        Code::UnknownType,
        Code::MissingField,
        Code::ImmutableAssign,
        Code::UseBeforeDefinition,
    ];

    /// The stable identifier, e.g. `"T0006"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Code::TypeMismatch => "T0001",
            Code::ArgumentType => "T0002",
            Code::ReturnType => "T0003",
            Code::Arity => "T0004",
            Code::InvalidOperator => "T0005",
            Code::UnknownName => "T0006",
            Code::UnknownField => "T0007",
            Code::UnknownMember => "T0008",
            Code::MissingReturn => "T0009",
            Code::Unreachable => "T0010",
            Code::NonExhaustiveMatch => "T0011",
            Code::InterfaceNotSatisfied => "T0012",
            Code::NotCallable => "T0013",
            Code::OptionMisuse => "T0014",
            Code::UnknownType => "T0015",
            Code::MissingField => "T0016",
            Code::ImmutableAssign => "T0017",
            Code::UseBeforeDefinition => "T0018",
        }
    }
}

impl Code {
    /// Look up a code by its identifier (`"T0006"`, case-insensitive).
    pub fn parse(code: &str) -> Option<Code> {
        Code::ALL
            .iter()
            .copied()
            .find(|c| c.as_str().eq_ignore_ascii_case(code))
    }

    /// Short lowercase title.
    pub fn title(self) -> &'static str {
        match self {
            Code::TypeMismatch => "type mismatch",
            Code::ArgumentType => "argument type mismatch",
            Code::ReturnType => "return type mismatch",
            Code::Arity => "wrong number of arguments",
            Code::InvalidOperator => "operator applied to unsupported types",
            Code::UnknownName => "unknown name",
            Code::UnknownField => "unknown struct field or method",
            Code::UnknownMember => "unknown module member",
            Code::MissingReturn => "missing return",
            Code::Unreachable => "unreachable code",
            Code::NonExhaustiveMatch => "non-exhaustive match",
            Code::InterfaceNotSatisfied => "interface not satisfied",
            Code::NotCallable => "value is not callable",
            Code::OptionMisuse => "Option used as its inner value",
            Code::UnknownType => "unknown type",
            Code::MissingField => "missing struct field",
            Code::ImmutableAssign => "assignment to an immutable variable",
            Code::UseBeforeDefinition => "name used before its definition",
        }
    }

    /// Long explanation with an example and the fix (`forge explain`).
    /// Where a runtime error is the dynamic counterpart, it is named.
    pub fn explanation(self) -> &'static str {
        match self {
            Code::TypeMismatch => {
                "\
A value does not match the declared type of the variable, struct field or
default value it is assigned to.

Example:

    let age: Int = \"forty\"

Fix: assign a value of the declared type (`let age: Int = 40`), convert it
(`int(text)`), or change the annotation. Under `--strict` this is an error;
otherwise a warning, and `--strict` also checks annotations at run time
(E0017)."
            }
            Code::ArgumentType => {
                "\
An argument does not match the declared type of the parameter it is passed
to.

Example:

    fn greet(name: String) { say \"hi \" + name }
    greet(42)

Fix: pass a value of the parameter's type (`greet(str(42))`) or widen the
annotation. The run-time counterpart under `--strict` is E0017."
            }
            Code::ReturnType => {
                "\
A function returns a value that does not match its declared return type.

Example:

    fn half(n: Int) -> Int { return n / 2.0 }

Fix: return the declared type (`return int(n / 2)`) or change `-> Int` to
the type actually returned."
            }
            Code::Arity => {
                "\
A call passes fewer arguments than the function requires or more than it
accepts.

Example:

    fn add(a, b) { return a + b }
    add(1)

Fix: pass every required argument or give the parameter a default
(`fn add(a, b = 0)`). At run time this is E0005."
            }
            Code::InvalidOperator => {
                "\
An operator is applied to operand types it never supports, such as
`\"a\" * 3` or `null + 1`.

Example:

    let total = \"3\" - 1

Fix: convert the operands first (`int(\"3\") - 1`). At run time this is
E0007."
            }
            Code::UnknownName => {
                "\
A name is used that is not defined anywhere in scope. The message suggests
a defined name with similar spelling when there is one.

Example:

    let count = 1
    say coutn

Fix: correct the spelling or define the name first. At run time this is
E0003."
            }
            Code::UnknownField => {
                "\
A field or method is used that the struct type does not declare.

Example:

    struct User { name: String }
    let u = User { name: \"Ada\" }
    say u.email

Fix: use a declared field, or add the field to the struct. At run time this
is E0013."
            }
            Code::UnknownMember => {
                "\
A stdlib module is asked for a member it does not have.

Example:

    let r = math.squareroot(9)

Fix: use the member's real name (`math.sqrt(9)`); see llms.txt or
`forge doc` for each module's members."
            }
            Code::MissingReturn => {
                "\
A function with a declared return type has a path that reaches the end of
its body without returning a value.

Example:

    fn sign(n: Int) -> Int {
        if n > 0 { return 1 }
    }

Fix: return a value on every path (`return 0` at the end, or an `else`
branch)."
            }
            Code::Unreachable => {
                "\
A statement can never run, because the statement before it always leaves
the block (`return`, `break`, `continue`).

Example:

    fn f() {
        return 1
        say \"done\"
    }

Fix: delete the dead statement or move it before the exit."
            }
            Code::NonExhaustiveMatch => {
                "\
A `match` on an algebraic type does not handle every variant.

Example:

    type Shape = Circle(Float) | Square(Float)
    let s = Circle(1.0)
    match s { Circle(r) => say r }

Fix: add arms for the missing variants or a wildcard `_ => ...`. At run time
an unmatched value is E0026."
            }
            Code::InterfaceNotSatisfied => {
                "\
A type is declared to implement an interface (`power`) but does not provide
every method the interface requires, or provides one with the wrong
parameters.

Example:

    power Speak { fn speak() -> String }
    give Robot the power Speak { }

Fix: implement every required method with the required parameters."
            }
            Code::NotCallable => {
                "\
A value that is not a function is called, usually because a variable
shadows a function with the same name.

Example:

    let total = 5
    total()

Fix: call the function you meant or rename the variable. At run time this
is E0004."
            }
            Code::OptionMisuse => {
                "\
An `Option` value is used as if it were the value inside it (in arithmetic,
string concatenation, ...).

Example:

    let n = Some(2)
    say n + 1

Fix: unwrap it first — `unwrap_or(n, 0) + 1`, or `match n { Some(v) => ...,
None => ... }`."
            }
            Code::UnknownType => {
                "\
A type annotation names a type that is neither built in nor declared.

Example:

    let name: Strng = \"Ada\"

Fix: use a known type (`Int`, `Float`, `String`, `Bool`, `[Int]`, `?Int`,
`Result<T, E>`, ...) or declare the struct/type first."
            }
            Code::MissingField => {
                "\
A struct literal leaves out a field that has no default value.

Example:

    struct Point { x: Int, y: Int }
    let p = Point { x: 1 }

Fix: provide the field (`Point { x: 1, y: 0 }`) or give it a default in the
struct declaration (`y: Int = 0`)."
            }
            Code::ImmutableAssign => {
                "\
A variable declared without `mut` is reassigned.

Example:

    let x = 1
    x = 2

Fix: declare it with `let mut x = 1`. At run time this is E0006."
            }
            Code::UseBeforeDefinition => {
                "\
A name is read before the statement that defines it has run.

Example:

    say greeting
    let greeting = \"hi\"

Fix: move the definition above its first use. Functions declared with `fn`
at the top level may be called before their definition; variables may not."
            }
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

/// A suggested edit: replace `span` with `replacement`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    pub title: String,
    pub span: Span,
    pub replacement: String,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    pub message: String,
    /// Where the problem is. A zero-width span at a statement start when no
    /// more precise position is known; `Span::default()` (line 0) when the
    /// checked program carries no positions at all.
    pub span: Span,
    /// Extra explanation shown after the message (`did you mean ...`).
    pub help: Option<String>,
    pub fixes: Vec<Fix>,
}

impl Diagnostic {
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    pub fn line(&self) -> usize {
        self.span.start.line
    }

    pub fn col(&self) -> usize {
        self.span.start.col
    }

    /// `message` plus the help line, as shown in a terminal.
    pub fn full_message(&self) -> String {
        match &self.help {
            Some(help) => format!("{}\n  help: {}", self.message, help),
            None => self.message.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn codes_are_unique_and_round_trip() {
        let mut seen = HashSet::new();
        for code in Code::ALL {
            assert!(seen.insert(code.as_str()), "duplicate {}", code);
        }
        // Codes are numbered densely in declaration order.
        for (i, code) in Code::ALL.iter().enumerate() {
            assert_eq!(code.as_str(), format!("T{:04}", i + 1));
            assert_eq!(Code::parse(&code.as_str().to_lowercase()), Some(*code));
        }
    }

    /// The indented example of an explanation (`forge explain`).
    pub(crate) fn example_of(explanation: &str) -> String {
        explanation
            .split("Example:")
            .nth(1)
            .and_then(|rest| rest.split("\nFix").next())
            .unwrap_or("")
            .lines()
            .filter_map(|l| l.strip_prefix("    "))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every explanation's example really produces its code, so
    /// `forge explain` never shows an example that does not apply.
    #[test]
    fn explanation_examples_produce_their_code() {
        for code in Code::ALL {
            let text = code.explanation();
            assert!(text.contains("Fix"), "{} explanation needs a fix", code);
            let example = example_of(text);
            assert!(!example.is_empty(), "{} explanation needs an example", code);
            let analysis = crate::typechecker::analyze(
                &example,
                &crate::typechecker::CheckOptions {
                    strict: false,
                    file: None,
                },
            )
            .unwrap_or_else(|_| panic!("{} example does not parse:\n{}", code, example));
            assert!(
                analysis.diagnostics.iter().any(|d| d.code == *code),
                "{} example does not produce {}:\n{}\ngot: {:?}",
                code,
                code,
                example,
                analysis
                    .diagnostics
                    .iter()
                    .map(|d| format!("{} {}", d.code, d.message))
                    .collect::<Vec<_>>()
            );
        }
    }
}
