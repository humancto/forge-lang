//! Natural-syntax forms removed in edition 2027 (RFC 0008 section 5).
//!
//! From edition 2027 the lexer no longer reserves the natural-syntax words
//! (`lexer::token::REMOVED_IN_2027`): `set`, `hold`, `say`, ... are plain
//! identifiers, usable as names. This module recognises the *old* shapes
//! built from those identifiers (`set x to 1`, `hold f()`, `say "hi"`,
//! `otherwise { }`, ...) and reports them as a syntax error that names the
//! replacement, instead of the confusing error the classic grammar would
//! give. Code `E0035` (`semantics::errors`).
//!
//! The checks look only at `Token::Ident` and run only when the parser was
//! created under edition 2027 or later (`Parser::report_removed`): in 2026
//! these words are keywords, or soft keywords used as names
//! (`parser::soft`). A removed word used as an
//! ordinary name (`let set = 1`, `check(x)`, `safe.x`) is not one of the
//! shapes below and parses normally.

use super::{ParseError, Parser};
use crate::lexer::token::Token;
use crate::semantics::edition::removed_syntax;

impl Parser {
    fn ident_at(&self, offset: usize) -> Option<&str> {
        match self.tokens.get(self.pos + offset).map(|t| &t.token) {
            Some(Token::Ident(name)) => Some(name.as_str()),
            _ => None,
        }
    }

    fn token_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset).map(|t| &t.token)
    }

    /// The token `offset` ahead is on the same line as the current token.
    fn on_same_line(&self, offset: usize) -> bool {
        match (
            self.tokens.get(self.pos),
            self.tokens.get(self.pos + offset),
        ) {
            (Some(a), Some(b)) => a.line == b.line,
            _ => false,
        }
    }

    /// The token `offset` ahead (on the same line) can start an operand,
    /// and cannot continue an expression that ends at the current token.
    /// `(`, `[`, `-` and `{` are excluded: after a name they are a call, an
    /// index, a subtraction or a block.
    fn operand_follows(&self, offset: usize) -> bool {
        self.on_same_line(offset)
            && matches!(
                self.token_at(offset),
                Some(
                    Token::Ident(_)
                        | Token::Int(_)
                        | Token::Float(_)
                        | Token::StringLit(_)
                        | Token::RawStringLit(_)
                        | Token::Bool(_)
                        | Token::True
                        | Token::False
                        | Token::NullLit
                        | Token::Not
                        | Token::Fn
                        | Token::Await
                        | Token::Ask
                        | Token::Freeze
                        | Token::IntType
                        | Token::FloatType
                        | Token::StringType
                        | Token::BoolType
                        | Token::JsonType
                )
            )
    }

    /// Index (relative to the cursor) of the bracket closing the one at
    /// `offset`.
    fn matching_close(&self, offset: usize) -> Option<usize> {
        let mut depth = 0usize;
        let mut i = offset;
        loop {
            match self.token_at(i)? {
                Token::LParen | Token::LBracket | Token::LBrace => depth += 1,
                Token::RParen | Token::RBracket | Token::RBrace => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                Token::Eof => return None,
                _ => {}
            }
            i += 1;
        }
    }

    fn removed(&self, form: &str, replacement: &str) -> Option<ParseError> {
        Some(self.error(&removed_syntax(form, replacement)))
    }

    /// A removed statement form starting at the cursor (an identifier in
    /// edition 2027), as an error naming its replacement.
    pub(super) fn removed_statement_form(&self) -> Option<ParseError> {
        if !self.report_removed {
            return None;
        }
        let word = self.ident_at(0)?;
        let name = |offset: usize| self.ident_at(offset).unwrap_or("x").to_string();
        match word {
            "set" if matches!(self.token_at(1), Some(Token::Mut)) && self.on_same_line(1) => {
                let n = name(2);
                self.removed(
                    &format!("set mut {n} to ..."),
                    &format!("let mut {n} = ..."),
                )
            }
            "set" if self.ident_at(1).is_some() && self.on_same_line(1) => {
                let n = name(1);
                self.removed(&format!("set {n} to ..."), &format!("let {n} = ..."))
            }
            "change" if self.operand_follows(1) => {
                let n = name(1);
                self.removed(&format!("change {n} to ..."), &format!("{n} = ..."))
            }
            "define" if self.ident_at(1).is_some() && self.on_same_line(1) => {
                let n = name(1);
                self.removed(&format!("define {n}(...)"), &format!("fn {n}(...)"))
            }
            "forge"
                if self.on_same_line(1)
                    && (self.ident_at(1).is_some()
                        || matches!(self.token_at(1), Some(Token::Fn))) =>
            {
                let n = if matches!(self.token_at(1), Some(Token::Fn)) {
                    name(2)
                } else {
                    name(1)
                };
                self.removed(&format!("forge {n}(...)"), &format!("async fn {n}(...)"))
            }
            "otherwise" | "nah" if matches!(self.token_at(1), Some(Token::LBrace | Token::If)) => {
                self.removed(word, "else")
            }
            "emit" if self.operand_follows(1) => self.removed("emit value", "yield value"),
            "grab"
                if self.ident_at(1).is_some() && matches!(self.token_at(2), Some(Token::From)) =>
            {
                let n = name(1);
                self.removed(
                    &format!("grab {n} from url"),
                    &format!("let {n} = fetch(url)"),
                )
            }
            "repeat" if self.operand_follows(1) => {
                self.removed("repeat n times { }", "for _ in range(n) { }")
            }
            "unpack"
                if matches!(
                    self.token_at(1),
                    Some(Token::LBrace | Token::LBracket | Token::LParen)
                ) && self
                    .matching_close(1)
                    .is_some_and(|close| matches!(self.token_at(close + 1), Some(Token::From))) =>
            {
                let (open, close) = match self.token_at(1) {
                    Some(Token::LBrace) => ("{", "}"),
                    Some(Token::LBracket) => ("[", "]"),
                    _ => ("(", ")"),
                };
                self.removed(
                    &format!("unpack {open}...{close} from value"),
                    &format!("let {open}...{close} = value"),
                )
            }
            "safe" if matches!(self.token_at(1), Some(Token::LBrace)) => {
                self.removed("safe { ... }", "try { ... } catch e { }")
            }
            "check" if self.operand_follows(1) => {
                self.removed("check value ...", "assert(condition, message)")
            }
            "say" if self.operand_follows(1) => self.removed("say value", "println(value)"),
            "yell" if self.operand_follows(1) => {
                self.removed("yell value", "println(upper(str(value)))")
            }
            "whisper" if self.operand_follows(1) => {
                self.removed("whisper value", "println(lower(str(value)))")
            }
            _ => self.removed_expression_form(),
        }
    }

    /// A removed prefix form in expression position: `hold e`, `must e`,
    /// `safe { }`.
    pub(super) fn removed_expression_form(&self) -> Option<ParseError> {
        if !self.report_removed {
            return None;
        }
        match self.ident_at(0)? {
            "hold" if self.operand_follows(1) => self.removed("hold expr", "await expr"),
            "must" if self.operand_follows(1) => self.removed("must expr", "unwrap(expr)"),
            "safe" if matches!(self.token_at(1), Some(Token::LBrace)) => {
                self.removed("safe { ... }", "try { ... } catch e { }")
            }
            _ => None,
        }
    }

    /// `otherwise ->` / `nah ->` as a `when` arm.
    pub(super) fn removed_else_arm(&self) -> Option<ParseError> {
        if !self.report_removed {
            return None;
        }
        match self.ident_at(0)? {
            word @ ("otherwise" | "nah")
                if matches!(self.token_at(1), Some(Token::Arrow | Token::FatArrow)) =>
            {
                self.removed(&format!("{word} ->"), "else ->")
            }
            _ => None,
        }
    }
}
