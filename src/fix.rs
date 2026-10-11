//! `forge fix --edition 2027`: migrate edition 2026 source to edition 2027
//! (RFC 0008 section 5, docs/editions/2027.md).
//!
//! The rewriter works on the edition-2026 token stream and edits the source
//! text at token offsets, so comments, blank lines and layout are kept
//! byte for byte outside the rewritten tokens. Every natural-syntax form
//! removed in 2027 becomes its replacement (`set x to 1` → `let x = 1`,
//! `say x` → `println(x)`, `repeat n times { }` → `for _ in range(n) { }`,
//! ...), and calls of the removed builtins that have an exact equivalent
//! are renamed (`bet(c)` → `assert(c)`). Whatever cannot be rewritten
//! mechanically (`safe { }` used as a value, `sus(x)`, which printed to
//! stderr, ...) is reported with its position and left untouched.
//!
//! The input must be edition-2026 code (it is lexed with the 2026 keyword
//! set whatever edition the process runs under), so running the fixer on
//! its own output changes nothing.

use crate::lexer::token::{Spanned, Token};
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::semantics::edition::{self, Edition};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A form the fixer left untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unfixable {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

/// The result of fixing one source text.
#[derive(Debug, Clone)]
pub struct Fixed {
    pub output: String,
    /// Number of rewritten forms.
    pub rewrites: usize,
    pub unfixable: Vec<Unfixable>,
}

#[derive(Debug)]
struct Edit {
    start: usize,
    end: usize,
    text: String,
}

/// Builtins removed in 2027 that have no mechanical equivalent, and why.
const MANUAL_BUILTINS: &[(&str, &str)] = &[
    (
        "sus",
        "sus(x) printed `x` and its type to stderr and returned `x`; use println(x) (stdout) or remove it",
    ),
    (
        "cook",
        "cook(f) called f and printed its run time to stderr; call f() and time it with time.now() if needed",
    ),
    (
        "yolo",
        "yolo(f) returned None when f failed; use try { ... } catch e { ... }",
    ),
    (
        "slay",
        "slay(f, n) benchmarked f; write a loop timing f() with time.now()",
    ),
];

/// Rewrite edition-2026 `source` to edition 2027.
pub fn fix_source(source: &str) -> Result<Fixed, String> {
    // Resolution of builtin names (is `bet` the builtin here?) must use the
    // 2026 globals.
    let _edition = edition::scope(Some(Edition::E2026));
    let tokens = Lexer::with_edition(source, Edition::E2026)
        .tokenize()
        .map_err(|e| format!("{}:{}: {}", e.line, e.col, e.message))?;
    let mut parser = Parser::with_index(tokens.clone(), source);
    parser.parse_program().map_err(|e| {
        format!(
            "{}:{}: does not parse as edition 2026 code: {}",
            e.line,
            e.col,
            crate::semantics::errors::headline(&e.message)
        )
    })?;
    let index = parser.take_index().unwrap_or_default();
    let resolution = crate::typechecker::resolve::resolve(&index, &[]);
    let removed_globals: HashSet<(usize, usize)> = index
        .occurrences
        .iter()
        .enumerate()
        .filter(|(id, occ)| {
            matches!(occ.role, crate::parser::index::Role::Ref)
                && crate::builtins_registry::removal(&occ.name).is_some()
                && resolution.resolved[*id] == crate::typechecker::resolve::Resolved::Global
        })
        .map(|(_, occ)| (occ.span.start.line, occ.span.start.col))
        .collect();

    let chars: Vec<char> = source.chars().collect();
    let mut fixer = Fixer {
        toks: &tokens,
        chars: &chars,
        edits: Vec::new(),
        unfixable: Vec::new(),
        rewrites: 0,
        removed_globals,
    };
    fixer.run();
    let Fixer {
        mut edits,
        mut unfixable,
        rewrites,
        ..
    } = fixer;

    // Apply (insertions at a position go before a replacement starting
    // there; edits never overlap).
    edits.sort_by_key(|e| (e.start, e.end != e.start));
    let mut output = String::with_capacity(source.len() + 64);
    let mut at = 0;
    for edit in &edits {
        if edit.start < at {
            return Err(format!(
                "BUG: overlapping rewrites at character {} ({:?})",
                edit.start, edit.text
            ));
        }
        output.extend(&chars[at..edit.start]);
        output.push_str(&edit.text);
        at = edit.end;
    }
    output.extend(&chars[at..]);

    if rewrites > 0 && unfixable.is_empty() {
        // The result must be edition-2027 code.
        let check = Lexer::with_edition(&output, Edition::E2027)
            .tokenize()
            .map_err(|e| (e.line, e.col, e.message))
            .and_then(|t| {
                let _e = edition::scope(Some(Edition::E2027));
                Parser::new(t)
                    .parse_program()
                    .map(|_| ())
                    .map_err(|e| (e.line, e.col, e.message))
            });
        if let Err((line, col, message)) = check {
            unfixable.push(Unfixable {
                line,
                col,
                message: format!(
                    "the rewritten file does not parse under edition 2027: {}",
                    crate::semantics::errors::headline(&message)
                ),
            });
        }
    }
    unfixable.sort_by_key(|u| (u.line, u.col));
    Ok(Fixed {
        output,
        rewrites,
        unfixable,
    })
}

struct Fixer<'a> {
    toks: &'a [Spanned],
    chars: &'a [char],
    edits: Vec<Edit>,
    unfixable: Vec<Unfixable>,
    rewrites: usize,
    /// Positions of references to removed builtins that resolve to the
    /// builtin (not a user definition of the same name).
    removed_globals: HashSet<(usize, usize)>,
}

fn is_open(t: &Token) -> bool {
    matches!(t, Token::LParen | Token::LBracket | Token::LBrace)
}

fn is_close(t: &Token) -> bool {
    matches!(t, Token::RParen | Token::RBracket | Token::RBrace)
}

/// A token after which a newline does not end the expression.
fn continues(t: &Token) -> bool {
    matches!(
        t,
        Token::Plus
            | Token::Minus
            | Token::Star
            | Token::Slash
            | Token::Percent
            | Token::EqEq
            | Token::NotEq
            | Token::Lt
            | Token::Gt
            | Token::LtEq
            | Token::GtEq
            | Token::And
            | Token::Or
            | Token::Pipe
            | Token::PipeRight
            | Token::Dot
            | Token::Comma
            | Token::Eq
            | Token::Arrow
            | Token::FatArrow
            | Token::Colon
            | Token::Not
    )
}

fn ident_is(t: &Token, word: &str) -> bool {
    matches!(t, Token::Ident(w) if w == word)
}

impl Fixer<'_> {
    fn tok(&self, i: usize) -> &Token {
        self.toks.get(i).map(|t| &t.token).unwrap_or(&Token::Eof)
    }

    fn start(&self, i: usize) -> usize {
        self.toks[i].offset
    }

    fn end(&self, i: usize) -> usize {
        self.toks[i].offset + self.toks[i].len
    }

    fn text(&self, from: usize, to_inclusive: usize) -> String {
        self.chars[self.start(from)..self.end(to_inclusive)]
            .iter()
            .collect()
    }

    fn replace(&mut self, start: usize, end: usize, text: impl Into<String>) {
        self.edits.push(Edit {
            start,
            end,
            text: text.into(),
        });
    }

    /// Replace token `i` with `text`.
    fn replace_tok(&mut self, i: usize, text: &str) {
        let (s, e) = (self.start(i), self.end(i));
        self.replace(s, e, text);
    }

    /// Replace token `i` and the whitespace after it with `text`.
    fn replace_tok_and_space(&mut self, i: usize, text: &str) {
        let (s, e) = (self.start(i), self.start(i + 1));
        self.replace(s, e, text);
    }

    fn insert_after(&mut self, i: usize, text: &str) {
        let e = self.end(i);
        self.replace(e, e, text);
    }

    fn report(&mut self, i: usize, message: impl Into<String>) {
        let t = &self.toks[i];
        self.unfixable.push(Unfixable {
            line: t.line,
            col: t.col,
            message: message.into(),
        });
    }

    fn matching(&self, open: usize) -> Option<usize> {
        let mut depth = 0usize;
        for j in open..self.toks.len() {
            let t = self.tok(j);
            if is_open(t) {
                depth += 1;
            } else if is_close(t) {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(j);
                }
            } else if matches!(t, Token::Eof) {
                return None;
            }
        }
        None
    }

    /// Index of the last token of the expression starting at `from`, which
    /// runs to the end of the statement or to a token matching `stop` at
    /// bracket depth 0.
    fn expr_end(&self, from: usize, stop: &dyn Fn(&Token) -> bool) -> Option<usize> {
        let mut j = from;
        let mut last = None;
        loop {
            let t = self.tok(j);
            match t {
                Token::Eof | Token::Semicolon | Token::Comma => break,
                Token::Newline => {
                    if last.is_some_and(|l| continues(self.tok(l))) {
                        j += 1;
                        continue;
                    }
                    break;
                }
                t if is_close(t) => break,
                t if stop(t) => break,
                t if is_open(t) => {
                    let close = self.matching(j)?;
                    last = Some(close);
                    j = close + 1;
                }
                _ => {
                    last = Some(j);
                    j += 1;
                }
            }
        }
        last
    }

    /// Index of the last token of the unary operand starting at `from`
    /// (prefix operators, a primary, then `.name`, calls, indexes and `?`).
    fn operand_end(&self, from: usize) -> Option<usize> {
        let mut j = from;
        while matches!(
            self.tok(j),
            Token::Not
                | Token::Minus
                | Token::Await
                | Token::Hold
                | Token::Must
                | Token::Freeze
                | Token::Ask
        ) {
            j += 1;
        }
        let mut end = match self.tok(j) {
            Token::Ident(_)
            | Token::Int(_)
            | Token::Float(_)
            | Token::StringLit(_)
            | Token::RawStringLit(_)
            | Token::Bool(_)
            | Token::True
            | Token::False
            | Token::NullLit
            | Token::Set
            | Token::Any => j,
            t if is_open(t) => self.matching(j)?,
            _ => return None,
        };
        loop {
            match self.tok(end + 1) {
                Token::Dot => end += 2,
                Token::LParen | Token::LBracket => end = self.matching(end + 1)?,
                Token::Question => end += 1,
                _ => return Some(end),
            }
        }
    }

    /// Whether token `i` is a keyword used as a name (`obj.check`,
    /// `{safe: 1}`, `fn check()`, `safe(x)`), not the keyword.
    fn used_as_name(&self, i: usize) -> bool {
        let prev = if i == 0 {
            &Token::Newline
        } else {
            self.tok(i - 1)
        };
        if matches!(
            prev,
            Token::Dot
                | Token::Fn
                | Token::Define
                | Token::Let
                | Token::Mut
                | Token::Async
                | Token::ForgeKw
                | Token::Catch
                | Token::For
                | Token::Each
        ) || matches!(self.tok(i + 1), Token::Colon)
        {
            return true;
        }
        match self.tok(i) {
            // `else` aliases only before a block, `if` or a `when` arrow.
            Token::Otherwise | Token::Nah => !matches!(
                self.tok(i + 1),
                Token::LBrace | Token::If | Token::Arrow | Token::FatArrow
            ),
            // The parser's rule for soft keywords (`parser::soft`).
            _ => crate::parser::soft::keyword_is_name(
                self.toks,
                i,
                i == 0
                    || matches!(
                        prev,
                        Token::Newline
                            | Token::Semicolon
                            | Token::LBrace
                            | Token::RBrace
                            | Token::FatArrow
                    ),
            ),
        }
    }

    fn at_statement_start(&self, i: usize) -> bool {
        i == 0
            || matches!(
                self.tok(i - 1),
                Token::Newline | Token::Semicolon | Token::LBrace | Token::RBrace
            )
    }

    fn run(&mut self) {
        for i in 0..self.toks.len() {
            let t = self.tok(i).clone();
            let keyword = matches!(
                t,
                Token::Set
                    | Token::Change
                    | Token::Define
                    | Token::Otherwise
                    | Token::Nah
                    | Token::ForgeKw
                    | Token::Hold
                    | Token::Emit
                    | Token::Grab
                    | Token::Repeat
                    | Token::Unpack
                    | Token::Safe
                    | Token::Must
                    | Token::Check
                    | Token::Say
                    | Token::Yell
                    | Token::Whisper
            );
            if keyword && self.used_as_name(i) {
                continue;
            }
            let before = (self.edits.len(), self.unfixable.len());
            match t {
                Token::Set => self.fix_set(i),
                Token::Change => self.fix_change(i),
                Token::Define => self.replace_tok(i, "fn"),
                Token::Otherwise | Token::Nah => self.replace_tok(i, "else"),
                Token::Hold => self.replace_tok(i, "await"),
                Token::Emit => self.replace_tok(i, "yield"),
                Token::ForgeKw => match self.tok(i + 1) {
                    Token::Fn | Token::Define => self.replace_tok(i, "async"),
                    Token::Ident(_) => self.replace_tok(i, "async fn"),
                    _ => self.report(i, "cannot rewrite this `forge`: write `async fn name(...)`"),
                },
                Token::Grab => self.fix_grab(i),
                Token::Repeat => self.fix_repeat(i),
                Token::Unpack => self.fix_unpack(i),
                Token::Safe => self.fix_safe(i),
                Token::Must => match self.operand_end(i + 1) {
                    Some(end) => {
                        self.replace_tok_and_space(i, "unwrap(");
                        self.insert_after(end, ")");
                    }
                    None => self.report(i, "cannot rewrite this `must`: write `unwrap(expr)`"),
                },
                Token::Check => self.fix_check(i),
                Token::Say | Token::Yell | Token::Whisper => self.fix_say(i, &t),
                Token::Ident(ref name) if self.removed_global(i) => self.fix_builtin(i, name),
                Token::StringLit(ref s) if s.contains('{') => self.check_interpolation(i, s),
                _ => {}
            }
            if self.edits.len() > before.0 && self.unfixable.len() == before.1 {
                self.rewrites += 1;
            }
        }
    }

    fn removed_global(&self, i: usize) -> bool {
        let t = &self.toks[i];
        self.removed_globals.contains(&(t.line, t.col))
    }

    fn fix_set(&mut self, i: usize) {
        // `set(...)` is the set() constructor; `set` stays a name in 2027.
        if matches!(self.tok(i + 1), Token::LParen) {
            return;
        }
        let name = if matches!(self.tok(i + 1), Token::Mut) {
            i + 2
        } else {
            i + 1
        };
        if matches!(self.tok(name), Token::Ident(_)) && matches!(self.tok(name + 1), Token::To) {
            self.replace_tok(i, "let");
            self.replace_tok(name + 1, "=");
        } else {
            self.report(i, "cannot rewrite this `set`: write `let name = value`");
        }
    }

    fn fix_change(&mut self, i: usize) {
        let to = self
            .expr_end(i + 1, &|t| matches!(t, Token::To))
            .map(|end| end + 1)
            .filter(|&j| matches!(self.tok(j), Token::To));
        match to {
            Some(to) => {
                self.replace_tok_and_space(i, "");
                self.replace_tok(to, "=");
            }
            None => self.report(i, "cannot rewrite this `change`: write `target = value`"),
        }
    }

    fn fix_grab(&mut self, i: usize) {
        let ok =
            matches!(self.tok(i + 1), Token::Ident(_)) && matches!(self.tok(i + 2), Token::From);
        let url_end = if ok {
            self.expr_end(i + 3, &|t| ident_is(t, "with"))
        } else {
            None
        };
        let Some(url_end) = url_end else {
            self.report(
                i,
                "cannot rewrite this `grab`: write `let name = fetch(url)`",
            );
            return;
        };
        let opts = if ident_is(self.tok(url_end + 1), "with") {
            match self.expr_end(url_end + 2, &|_| false) {
                Some(end) => Some(end),
                None => {
                    self.report(i, "cannot rewrite this `grab ... with`: write `let name = fetch(url, options)`");
                    return;
                }
            }
        } else {
            None
        };
        self.replace_tok(i, "let");
        let (from_start, url_start) = (self.start(i + 2), self.start(i + 3));
        self.replace(from_start, url_start, "= fetch(");
        match opts {
            Some(end) => {
                let (s, e) = (self.end(url_end), self.start(url_end + 2));
                self.replace(s, e, ", ");
                self.insert_after(end, ")");
            }
            None => self.insert_after(url_end, ")"),
        }
    }

    fn fix_repeat(&mut self, i: usize) {
        let end = self.expr_end(i + 1, &|t| matches!(t, Token::Times | Token::LBrace));
        let brace = end.and_then(|end| match self.tok(end + 1) {
            Token::Times if matches!(self.tok(end + 2), Token::LBrace) => Some(end + 2),
            Token::LBrace => Some(end + 1),
            _ => None,
        });
        match (end, brace) {
            (Some(end), Some(brace)) => {
                self.replace_tok_and_space(i, "for _ in range(");
                let (s, e) = (self.end(end), self.start(brace));
                self.replace(s, e, ") ");
            }
            _ => self.report(
                i,
                "cannot rewrite this `repeat`: write `for _ in range(n) { ... }`",
            ),
        }
    }

    fn fix_unpack(&mut self, i: usize) {
        let from = self
            .matching(i + 1)
            .filter(|_| is_open(self.tok(i + 1)))
            .map(|close| close + 1)
            .filter(|&j| matches!(self.tok(j), Token::From));
        match from {
            Some(from) => {
                self.replace_tok(i, "let");
                self.replace_tok(from, "=");
            }
            None => self.report(
                i,
                "cannot rewrite this `unpack`: write `let {a, b} = value`",
            ),
        }
    }

    fn fix_safe(&mut self, i: usize) {
        let close = if matches!(self.tok(i + 1), Token::LBrace) {
            self.matching(i + 1)
        } else {
            None
        };
        match close {
            Some(close) if self.at_statement_start(i) => {
                self.replace_tok(i, "try");
                self.insert_after(close, " catch _ { }");
            }
            _ => self.report(
                i,
                "cannot rewrite `safe { }` used as a value: use `try { ... } catch e { ... }` and assign in both branches",
            ),
        }
    }

    fn fix_check(&mut self, i: usize) {
        let stop = |t: &Token| ident_is(t, "is") || ident_is(t, "between");
        let Some(e_end) = self.expr_end(i + 1, &stop) else {
            self.report(i, "cannot rewrite this `check`: write `assert(condition)`");
            return;
        };
        let next = self.tok(e_end + 1).clone();
        if ident_is(&next, "is") {
            let mut m = e_end + 2;
            if matches!(self.tok(m), Token::Not) || ident_is(self.tok(m), "not") {
                m += 1;
            }
            if ident_is(self.tok(m), "empty") {
                self.replace_tok_and_space(i, "assert(len(");
                let (s, e) = (self.end(e_end), self.end(m));
                self.replace(s, e, ") > 0)");
            } else {
                self.report(
                    i,
                    "cannot rewrite `check ... is ...` (only `is not empty` has a fixed meaning): write `assert(condition)`",
                );
            }
        } else if ident_is(&next, "between") {
            // The subject is repeated, so it must be free of effects.
            let simple = (i + 1..=e_end)
                .all(|j| matches!(self.tok(j), Token::Ident(_) | Token::Dot))
                || (i + 1 == e_end
                    && matches!(
                        self.tok(e_end),
                        Token::Int(_) | Token::Float(_) | Token::StringLit(_)
                    ));
            let lo = e_end + 2;
            let lo_end = self.expr_end(lo, &|t| ident_is(t, "and") || matches!(t, Token::And));
            let and = lo_end
                .map(|l| l + 1)
                .filter(|&a| ident_is(self.tok(a), "and") || matches!(self.tok(a), Token::And));
            let hi_end = and.and_then(|a| self.expr_end(a + 1, &|_| false));
            match (simple, lo_end, and, hi_end) {
                (true, Some(lo_end), Some(and), Some(hi_end)) => {
                    let subject = self.text(i + 1, e_end);
                    let (s, e) = (self.start(i), self.start(lo));
                    self.replace(s, e, format!("assert({subject} >= "));
                    let (s, e) = (self.end(lo_end), self.start(and + 1));
                    self.replace(s, e, format!(" && {subject} <= "));
                    self.insert_after(hi_end, ")");
                }
                _ => self.report(
                    i,
                    "cannot rewrite this `check ... between`: write `assert(x >= lo && x <= hi)`",
                ),
            }
        } else {
            self.replace_tok_and_space(i, "assert(");
            self.insert_after(e_end, ")");
        }
    }

    fn fix_say(&mut self, i: usize, which: &Token) {
        let Some(end) = self.expr_end(i + 1, &|_| false) else {
            self.report(
                i,
                "cannot rewrite this output statement: write `println(value)`",
            );
            return;
        };
        let single_string =
            i + 1 == end && matches!(self.tok(end), Token::StringLit(_) | Token::RawStringLit(_));
        let one_group = matches!(self.tok(i + 1), Token::LParen)
            && self.matching(i + 1) == Some(end)
            && !self.has_top_level_comma(i + 1, end);
        let (open, close) = match which {
            Token::Say if one_group => {
                self.replace_tok_and_space(i, "println");
                return;
            }
            Token::Say => ("println(", ")"),
            Token::Yell if single_string => ("println(upper(", "))"),
            Token::Yell => ("println(upper(str(", ")))"),
            _ if single_string => ("println(lower(", "))"),
            _ => ("println(lower(str(", ")))"),
        };
        self.replace_tok_and_space(i, open);
        self.insert_after(end, close);
    }

    fn has_top_level_comma(&self, open: usize, close: usize) -> bool {
        let mut depth = 0usize;
        for j in open + 1..close {
            let t = self.tok(j);
            if is_open(t) {
                depth += 1;
            } else if is_close(t) {
                depth = depth.saturating_sub(1);
            } else if depth == 0 && matches!(t, Token::Comma) {
                return true;
            }
        }
        false
    }

    fn fix_builtin(&mut self, i: usize, name: &str) {
        if let Some((_, why)) = MANUAL_BUILTINS.iter().find(|(n, _)| *n == name) {
            self.report(i, format!("`{name}` was removed in edition 2027: {why}"));
            return;
        }
        let close = if matches!(self.tok(i + 1), Token::LParen) {
            self.matching(i + 1)
        } else {
            None
        };
        let Some(close) = close else {
            let hint = crate::builtins_registry::removal_hint(name).unwrap_or_default();
            self.report(i, format!("cannot rewrite this use of `{name}`: {hint}"));
            return;
        };
        match name {
            "bet" => self.replace_tok(i, "assert"),
            "no_cap" => self.replace_tok(i, "assert_eq"),
            "bruh" if close == i + 2 => {
                let (s, e) = (self.start(i), self.end(close));
                self.replace(s, e, "assert(false, \"something ain't right\")");
            }
            "bruh" => {
                let (s, e) = (self.start(i), self.start(i + 2));
                self.replace(s, e, "assert(false, ");
            }
            "ick" if close > i + 2 => {
                let first_end = self.expr_end(i + 2, &|_| false);
                match first_end {
                    Some(end) => {
                        let (s, e) = (self.start(i), self.start(i + 2));
                        self.replace(s, e, "assert(!(");
                        self.insert_after(end, ")");
                    }
                    None => self.report(i, "cannot rewrite this `ick`: write `assert(!condition)`"),
                }
            }
            "ghost" if close == i + 3 && matches!(self.tok(i + 2), Token::Ident(_)) => {
                let f = self.text(i + 2, i + 2);
                let (s, e) = (self.start(i), self.end(close));
                self.replace(s, e, format!("{f}()"));
            }
            _ => {
                let hint = crate::builtins_registry::removal_hint(name).unwrap_or_default();
                self.report(i, format!("cannot rewrite this use of `{name}`: {hint}"));
            }
        }
    }

    /// Removed forms inside `"{...}"` interpolations are reported (the
    /// rewriter does not edit inside string literals).
    fn check_interpolation(&mut self, i: usize, s: &str) {
        let mut rest = s;
        while let Some(open) = rest.find('{') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else { break };
            let inner = &after[..close];
            let removed = Lexer::with_edition(inner, Edition::E2026)
                .tokenize()
                .map(|toks| {
                    toks.iter().any(|t| {
                        matches!(
                            t.token,
                            Token::Hold | Token::Must | Token::Safe | Token::Say | Token::Check
                        )
                    })
                })
                .unwrap_or(false);
            if removed {
                self.report(
                    i,
                    format!("cannot rewrite inside a string interpolation: `{{{inner}}}`"),
                );
                return;
            }
            rest = &after[close + 1..];
        }
    }
}

/// Set `edition = "<edition>"` under `[project]` in a forge.toml text.
pub fn set_manifest_edition(manifest: &str, edition: Edition) -> String {
    let line = format!("edition = \"{}\"", edition);
    let mut out: Vec<String> = Vec::new();
    let mut in_project = false;
    let mut done = false;
    let mut project_header = None;
    for l in manifest.lines() {
        let trimmed = l.trim();
        if trimmed.starts_with('[') {
            if in_project && !done {
                // `[project]` ended without an edition key.
                if let Some(at) = project_header {
                    out.insert(at + 1, line.clone());
                    done = true;
                }
            }
            in_project = trimmed == "[project]";
            if in_project {
                project_header = Some(out.len());
            }
        } else if in_project && !done {
            let key = trimmed.split('=').next().unwrap_or("").trim();
            if key == "edition" {
                out.push(line.clone());
                done = true;
                continue;
            }
        }
        out.push(l.to_string());
    }
    if !done {
        match project_header {
            Some(at) => out.insert(at + 1, line),
            None => {
                if out.last().is_some_and(|l| !l.trim().is_empty()) {
                    out.push(String::new());
                }
                out.push("[project]".to_string());
                out.push(line);
            }
        }
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

/// A unified diff of `old` → `new` (3 lines of context).
pub fn unified_diff(path: &str, old: &str, new: &str) -> String {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // Longest common subsequence table (files are small; huge inputs fall
    // back to one hunk replacing everything).
    let ops: Vec<(char, usize, usize)> = if a.len().saturating_mul(b.len()) > 25_000_000 {
        let mut ops: Vec<(char, usize, usize)> = (0..a.len()).map(|i| ('-', i, 0)).collect();
        ops.extend((0..b.len()).map(|j| ('+', a.len(), j)));
        ops
    } else {
        let (n, m) = (a.len(), b.len());
        let mut lcs = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i][j] = if a[i] == b[j] {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        let mut ops = Vec::new();
        while i < n || j < m {
            if i < n && j < m && a[i] == b[j] {
                ops.push((' ', i, j));
                i += 1;
                j += 1;
            } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
                ops.push(('-', i, j));
                i += 1;
            } else {
                ops.push(('+', i, j));
                j += 1;
            }
        }
        ops
    };
    if ops.iter().all(|(k, _, _)| *k == ' ') {
        return String::new();
    }
    let mut out = format!("--- a/{path}\n+++ b/{path}\n");
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, (k, _, _))| *k != ' ')
        .map(|(idx, _)| idx)
        .collect();
    let mut k = 0;
    while k < changed.len() {
        let first = changed[k].saturating_sub(3);
        let mut last = changed[k];
        while k + 1 < changed.len() && changed[k + 1] <= last + 7 {
            k += 1;
            last = changed[k];
        }
        let last = (last + 3).min(ops.len() - 1);
        let hunk = &ops[first..=last];
        let old_start = hunk.first().map(|o| o.1).unwrap_or(0);
        let new_start = hunk.first().map(|o| o.2).unwrap_or(0);
        let old_len = hunk.iter().filter(|o| o.0 != '+').count();
        let new_len = hunk.iter().filter(|o| o.0 != '-').count();
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            old_start + usize::from(old_len > 0),
            old_len,
            new_start + usize::from(new_len > 0),
            new_len
        ));
        for (kind, i, j) in hunk {
            let line = if *kind == '+' { b[*j] } else { a[*i] };
            out.push_str(&format!("{kind}{line}\n"));
        }
        k += 1;
    }
    out
}

/// Options of `forge fix`.
pub struct FixOptions {
    /// Do not write; exit 1 when a file would change.
    pub check: bool,
    /// Print a unified diff instead of writing.
    pub diff: bool,
    /// Also set `edition = "2027"` in `forge.toml` (project roots).
    pub set_edition: bool,
}

/// `forge fix --edition 2027 [paths]`. Returns the process exit code: 0
/// when everything is (or was made) edition-2027 code, 1 when `--check`
/// found files to change or some forms could not be rewritten.
pub fn run(paths: &[PathBuf], options: &FixOptions) -> i32 {
    let roots: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.to_vec()
    };
    let mut files = Vec::new();
    for root in &roots {
        if root.is_dir() {
            files.extend(crate::formatter::find_forge_files(&root.to_string_lossy()));
        } else {
            files.push(root.clone());
        }
    }
    let write = !options.check && !options.diff;
    let (mut changed, mut manual, mut failed) = (0usize, 0usize, false);
    for path in &files {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: error: {}", path.display(), e);
                failed = true;
                continue;
            }
        };
        let fixed = match fix_source(&source) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("{}:{}", path.display(), e);
                failed = true;
                continue;
            }
        };
        for u in &fixed.unfixable {
            eprintln!("{}:{}:{}: {}", path.display(), u.line, u.col, u.message);
        }
        manual += fixed.unfixable.len();
        if fixed.output == source {
            continue;
        }
        changed += 1;
        if options.diff {
            print!(
                "{}",
                unified_diff(&path.to_string_lossy(), &source, &fixed.output)
            );
        }
        if options.check && !options.diff {
            println!("  would fix  {}", path.display());
        }
        if write {
            match std::fs::write(path, &fixed.output) {
                Ok(()) => println!(
                    "  fixed  {} ({} rewrite{})",
                    path.display(),
                    fixed.rewrites,
                    if fixed.rewrites == 1 { "" } else { "s" }
                ),
                Err(e) => {
                    eprintln!("{}: error: {}", path.display(), e);
                    failed = true;
                }
            }
        }
    }
    if options.set_edition {
        for root in &roots {
            let dir = if root.is_dir() {
                root.as_path()
            } else {
                continue;
            };
            if let Err(e) = set_project_edition(dir, options) {
                eprintln!("{}", e);
                failed = true;
            }
        }
    }
    if write {
        println!(
            "\n  {} file(s) fixed, {} form(s) left for you to rewrite",
            changed, manual
        );
    }
    if failed || manual > 0 || (options.check && changed > 0) {
        1
    } else {
        0
    }
}

fn set_project_edition(dir: &Path, options: &FixOptions) -> Result<(), String> {
    let path = dir.join("forge.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(()); // not a project root
    };
    let updated = set_manifest_edition(&text, Edition::E2027);
    if updated == text {
        return Ok(());
    }
    if options.diff {
        print!("{}", unified_diff(&path.to_string_lossy(), &text, &updated));
    }
    if !options.check && !options.diff {
        std::fs::write(&path, updated).map_err(|e| format!("{}: error: {}", path.display(), e))?;
        println!("  set edition = \"2027\" in {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(src: &str) -> String {
        let fixed = fix_source(src).expect("fixes");
        assert!(fixed.unfixable.is_empty(), "{:?}", fixed.unfixable);
        fixed.output
    }

    #[test]
    fn rewrites_each_removed_form() {
        let cases = [
            ("set x to 5", "let x = 5"),
            ("set mut x to 5", "let mut x = 5"),
            ("let mut x = 1\nchange x to 2", "let mut x = 1\nx = 2"),
            ("define f(a) { return a }", "fn f(a) { return a }"),
            ("forge f() { return 1 }", "async fn f() { return 1 }"),
            ("forge fn f() { return 1 }", "async fn f() { return 1 }"),
            (
                "if true { say 1 } otherwise { say 2 }",
                "if true { println(1) } else { println(2) }",
            ),
            ("if true { } nah { }", "if true { } else { }"),
            ("let r = hold f()", "let r = await f()"),
            ("emit 5", "yield 5"),
            ("grab r from \"http://x\"", "let r = fetch(\"http://x\")"),
            (
                "grab r from url with { method: \"POST\" }",
                "let r = fetch(url, { method: \"POST\" })",
            ),
            (
                "repeat 3 times { say \"hi\" }",
                "for _ in range(3) { println(\"hi\") }",
            ),
            ("repeat n + 1 { }", "for _ in range(n + 1) { }"),
            ("unpack {a, b} from obj", "let {a, b} = obj"),
            ("unpack [x, ...rest] from arr", "let [x, ...rest] = arr"),
            ("safe { risky() }", "try { risky() } catch _ { }"),
            (
                "let v = must parse(s).value",
                "let v = unwrap(parse(s).value)",
            ),
            ("let v = must f() + 1", "let v = unwrap(f()) + 1"),
            ("check x > 0", "assert(x > 0)"),
            ("check name is not empty", "assert(len(name) > 0)"),
            (
                "check age between 1 and 10",
                "assert(age >= 1 && age <= 10)",
            ),
            ("say \"hi\"", "println(\"hi\")"),
            ("say(\"hi\")", "println(\"hi\")"),
            ("say (1, 2)", "println((1, 2))"),
            ("yell \"hi\"", "println(upper(\"hi\"))"),
            ("whisper x", "println(lower(str(x)))"),
            ("bet(x > 1, \"m\")", "assert(x > 1, \"m\")"),
            ("no_cap(a, b)", "assert_eq(a, b)"),
            ("bruh(\"stop\")", "assert(false, \"stop\")"),
            ("bruh()", "assert(false, \"something ain't right\")"),
            ("ick(x == 1)", "assert(!(x == 1))"),
            (
                "fn g() { return 1 }\nlet v = ghost(g)",
                "fn g() { return 1 }\nlet v = g()",
            ),
            (
                "when x { < 1 -> \"a\", otherwise -> \"b\" }",
                "when x { < 1 -> \"a\", else -> \"b\" }",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(fix(input), expected, "input: {input}");
        }
    }

    #[test]
    fn keeps_comments_layout_and_names() {
        let src = "// greet\nset name to \"Ada\"   // who\n\n/* block */\nsay \"hi {name}\"\nlet o = json.parse(\"\"\"{\"check\": 1}\"\"\")\nprintln(o.check)\nlet s = set([1])\n";
        let out = fix(src);
        assert_eq!(
            out,
            "// greet\nlet name = \"Ada\"   // who\n\n/* block */\nprintln(\"hi {name}\")\nlet o = json.parse(\"\"\"{\"check\": 1}\"\"\")\nprintln(o.check)\nlet s = set([1])\n"
        );
    }

    #[test]
    fn nested_forms_compose() {
        assert_eq!(fix("say must f()"), "println(unwrap(f()))");
        assert_eq!(fix("set v to hold g()"), "let v = await g()");
    }

    #[test]
    fn shadowed_builtins_are_left_alone() {
        let src = "fn bet(x) { return x }\nprintln(bet(1))\n";
        assert_eq!(fix(src), src);
    }

    #[test]
    fn reports_what_it_cannot_rewrite() {
        let fixed = fix_source("let v = safe { 1 }\nsus(2)\n").expect("fixes");
        let lines: Vec<usize> = fixed.unfixable.iter().map(|u| u.line).collect();
        assert_eq!(lines, vec![1, 2], "{:?}", fixed.unfixable);
        assert!(fixed.output.contains("safe { 1 }"));
        assert!(fixed.output.contains("sus(2)"));
    }

    #[test]
    fn is_idempotent() {
        let once = fix("set x to 1\nrepeat 2 times { say x }\n");
        assert_eq!(fix(&once), once);
    }

    #[test]
    fn manifest_edition_is_set() {
        assert_eq!(
            set_manifest_edition("[project]\nname = \"a\"\n", Edition::E2027),
            "[project]\nedition = \"2027\"\nname = \"a\"\n"
        );
        assert_eq!(
            set_manifest_edition(
                "[project]\nedition = \"2026\"\n[test]\ndirectory = \"t\"\n",
                Edition::E2027
            ),
            "[project]\nedition = \"2027\"\n[test]\ndirectory = \"t\"\n"
        );
        assert_eq!(
            set_manifest_edition("[test]\nx = 1\n", Edition::E2027),
            "[test]\nx = 1\n\n[project]\nedition = \"2027\"\n"
        );
    }

    #[test]
    fn diff_shows_changed_lines() {
        let d = unified_diff("a.fg", "a\nb\nc\n", "a\nB\nc\n");
        assert!(d.contains("-b\n+B\n"), "{d}");
        assert!(
            d.starts_with("--- a/a.fg\n+++ b/a.fg\n@@ -1,3 +1,3 @@"),
            "{d}"
        );
        assert_eq!(unified_diff("a.fg", "x\n", "x\n"), "");
    }
}
