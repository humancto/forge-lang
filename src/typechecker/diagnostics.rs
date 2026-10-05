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
    #[cfg(test)]
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
        }
    }
}
