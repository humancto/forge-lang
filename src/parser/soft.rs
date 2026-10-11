//! Soft (contextual) keywords, in every edition.
//!
//! Forge's natural-syntax and innovation words (`order`, `from`, `to`,
//! `safe`, `emit`, `limit`, `check`, ...) are keywords only where their
//! construct can start. Everywhere else they are ordinary names:
//! `let order = 1`, `fn safe(x) { }`, `println(from)`, `{ limit: 5 }`,
//! `obj.check`. The rule is additive: a word is read as a name only where
//! its construct could not have parsed, so every program that parsed
//! before parses to the same tree.
//!
//! Mechanism: the lexer still produces the keyword token; at an operand
//! position (statement start, unary/primary expression) the parser asks
//! [`keyword_is_name`] and, when it says so, turns the token into
//! `Token::Ident` before parsing on. Binding and field positions accept the
//! words through [`name_word`] (`expect_ident`). Constructs that look for a
//! word after an expression (`x where f > 1`, `>> take 3`, `retry 3 times`,
//! `download u to p`, `for each`) still see the keyword token.
//!
//! Still reserved in every edition (never names): the core language
//! (`let mut fn return if else match for in while loop break continue
//! struct type interface impl pub import spawn squad try catch async await
//! yield true false null`) and the type names (`Int Float String Bool
//! Json`). In edition 2027 the removed natural words are not keywords at
//! all (`lexer::token::REMOVED_IN_2027`).

use crate::lexer::token::{Spanned, Token};

/// The word of a soft keyword token.
pub fn soft_word(t: &Token) -> Option<&'static str> {
    Some(match t {
        Token::Set => "set",
        Token::To => "to",
        Token::Change => "change",
        Token::Define => "define",
        Token::Otherwise => "otherwise",
        Token::Nah => "nah",
        Token::Each => "each",
        Token::Repeat => "repeat",
        Token::Times => "times",
        Token::Grab => "grab",
        Token::From => "from",
        Token::Wait => "wait",
        Token::Seconds => "seconds",
        Token::Say => "say",
        Token::Yell => "yell",
        Token::Whisper => "whisper",
        Token::Thing => "thing",
        Token::Power => "power",
        Token::Give => "give",
        Token::Craft => "craft",
        Token::The => "the",
        Token::ForgeKw => "forge",
        Token::Hold => "hold",
        Token::Emit => "emit",
        Token::Unpack => "unpack",
        Token::When => "when",
        Token::Unless => "unless",
        Token::Until => "until",
        Token::Must => "must",
        Token::Check => "check",
        Token::Safe => "safe",
        Token::Where => "where",
        Token::Timeout => "timeout",
        Token::Retry => "retry",
        Token::Schedule => "schedule",
        Token::Every => "every",
        Token::Any => "any",
        Token::Ask => "ask",
        Token::Prompt => "prompt",
        Token::Agent => "agent",
        Token::Transform => "transform",
        Token::Table => "table",
        Token::Select => "select",
        Token::Order => "order",
        Token::By => "by",
        Token::Limit => "limit",
        Token::Keep => "keep",
        Token::Take => "take",
        Token::Freeze => "freeze",
        Token::Watch => "watch",
        Token::Download => "download",
        Token::Crawl => "crawl",
        _ => return None,
    })
}

/// The name a token spells in a binding or field position: an identifier,
/// a soft keyword, or one of the reserved words that were always accepted
/// there (`obj.type`, `x.match`, `import`, `in`).
pub fn name_word(t: &Token) -> Option<String> {
    match t {
        Token::Ident(name) => Some(name.clone()),
        Token::Type => Some("type".to_string()),
        Token::Match => Some("match".to_string()),
        Token::In => Some("in".to_string()),
        Token::Import => Some("import".to_string()),
        other => soft_word(other).map(str::to_string),
    }
}

/// A token that cannot start an expression (it can only follow one), so a
/// keyword that takes an expression is a name when this follows it.
fn ends_operand(t: &Token) -> bool {
    matches!(
        t,
        Token::Eq
            | Token::PlusEq
            | Token::MinusEq
            | Token::StarEq
            | Token::SlashEq
            | Token::PercentEq
            | Token::Dot
            | Token::Comma
            | Token::RParen
            | Token::RBracket
            | Token::RBrace
            | Token::Colon
            | Token::Semicolon
            | Token::Newline
            | Token::Eof
            | Token::Plus
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
            | Token::Question
            | Token::FatArrow
            | Token::Arrow
            | Token::Bar
            | Token::In
    )
}

/// Whether the soft keyword at `tokens[i]`, at an operand position, is
/// used as a name: its construct cannot start here. `statement`: the
/// position is the start of a statement (statement constructs such as
/// `check x`, `say x`, `define f()` are possible); otherwise it is the
/// start of a unary expression, where only `hold`, `must`, `freeze`, `ask`,
/// `safe { }`, `when` and `craft` are constructs. `false` for any other
/// token.
pub fn keyword_is_name(tokens: &[Spanned], i: usize, statement: bool) -> bool {
    let tok = |k: usize| tokens.get(k).map(|t| &t.token).unwrap_or(&Token::Eof);
    let expression_construct = matches!(
        tok(i),
        Token::Hold
            | Token::Must
            | Token::Freeze
            | Token::Ask
            | Token::Safe
            | Token::When
            | Token::Craft
    );
    if !statement && !expression_construct {
        return soft_word(tok(i)).is_some();
    }
    let ident_like = |k: usize| name_word(tok(k)).is_some();
    let starts_expr = |k: usize| !ends_operand(tok(k));
    let construct = match tok(i) {
        // Connector words: never the start of a construct.
        Token::To
        | Token::From
        | Token::Times
        | Token::Seconds
        | Token::Each
        | Token::Where
        | Token::Order
        | Token::By
        | Token::Limit
        | Token::Keep
        | Token::Take
        | Token::The
        | Token::Unless
        | Token::Until
        | Token::Every
        | Token::Transform
        | Token::Table
        | Token::Otherwise
        | Token::Nah
        | Token::Select
        | Token::Any => false,
        // `set [mut] name to v`
        Token::Set => {
            matches!(tok(i + 1), Token::Mut)
                || (ident_like(i + 1) && matches!(tok(i + 2), Token::To))
        }
        Token::Define | Token::ForgeKw => {
            ident_like(i + 1) || matches!(tok(i + 1), Token::Fn | Token::Define)
        }
        Token::Thing | Token::Power | Token::Give | Token::Prompt | Token::Agent => {
            ident_like(i + 1)
        }
        Token::Craft => ident_like(i + 1) && matches!(tok(i + 2), Token::LBrace),
        Token::Grab => ident_like(i + 1) && matches!(tok(i + 2), Token::From),
        Token::Unpack => {
            matches!(tok(i + 1), Token::LBrace | Token::LBracket | Token::LParen)
                && matching_close(tokens, i + 1)
                    .is_some_and(|close| matches!(tok(close + 1), Token::From))
        }
        // `safe { }` (the block may start on the next line)
        Token::Safe => {
            let mut k = i + 1;
            while matches!(tok(k), Token::Newline) {
                k += 1;
            }
            matches!(tok(k), Token::LBrace)
        }
        Token::Schedule => matches!(tok(i + 1), Token::Every) || starts_expr(i + 1),
        Token::Change
        | Token::Repeat
        | Token::Say
        | Token::Yell
        | Token::Whisper
        | Token::Wait
        | Token::Emit
        | Token::Check
        | Token::Timeout
        | Token::Retry
        | Token::Watch
        | Token::Download
        | Token::Crawl
        | Token::When
        | Token::Hold
        | Token::Must
        | Token::Freeze
        | Token::Ask => starts_expr(i + 1),
        _ => return false,
    };
    !construct
}

fn matching_close(tokens: &[Spanned], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (k, t) in tokens.iter().enumerate().skip(open) {
        match t.token {
            Token::LParen | Token::LBracket | Token::LBrace => depth += 1,
            Token::RParen | Token::RBracket | Token::RBrace => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(k);
                }
            }
            Token::Eof => return None,
            _ => {}
        }
    }
    None
}
