//! `forge fmt`: a token-based source formatter (see [`format_source`]).
use std::path::{Path, PathBuf};

use crate::lexer::token::Token;
use crate::lexer::{LexError, Lexer};

pub fn format_files(files: &[PathBuf], check: bool) {
    let targets = if files.is_empty() {
        find_forge_files(".")
    } else {
        files.to_vec()
    };

    if targets.is_empty() {
        println!("No .fg files found");
        return;
    }

    let mut formatted = 0;
    let mut unformatted = 0;
    for path in &targets {
        match std::fs::read_to_string(path) {
            Ok(source) => {
                let result = format_source(&source);
                if result != source {
                    if check {
                        println!("  would format  {}", path.display());
                        unformatted += 1;
                    } else {
                        if let Err(e) = std::fs::write(path, &result) {
                            eprintln!("  error      {} — {}", path.display(), e);
                            continue;
                        }
                        println!("  formatted  {}", path.display());
                        formatted += 1;
                    }
                } else {
                    println!("  unchanged  {}", path.display());
                }
            }
            Err(e) => {
                eprintln!("  error      {} — {}", path.display(), e);
            }
        }
    }
    println!();
    if check {
        if unformatted > 0 {
            println!("  {} file(s) need formatting", unformatted);
            std::process::exit(1);
        } else {
            println!("  All files formatted correctly");
        }
    } else {
        println!("  {} file(s) formatted", formatted);
    }
}

fn find_forge_files(dir: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_fg_files(Path::new(dir), &mut files);
    files.sort();
    files
}

fn collect_fg_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if !name.starts_with('.') && name != "target" && name != "node_modules" {
                    collect_fg_files(&path, files);
                }
            } else if path.extension().is_some_and(|e| e == "fg") {
                files.push(path);
            }
        }
    }
}

/// Count leading close braces/brackets at the start of a line (before any other content).
fn count_leading_closes(line: &str) -> i32 {
    let mut count = 0i32;
    for c in line.chars() {
        match c {
            '}' | ']' | ')' => count += 1,
            ' ' | '\t' => continue,
            _ => break,
        }
    }
    count
}

/// Count opening/closing delimiters (braces, brackets, parens) in a line,
/// ignoring those inside strings and comments.
#[cfg(test)]
fn count_delimiters(line: &str) -> (i32, i32) {
    let (opens, closes, _) = scan_line(line);
    (opens, closes)
}

/// Scan one line of code (which does NOT start inside a block comment).
/// Returns `(opens, closes, ends_inside_block_comment)`, ignoring delimiters
/// inside strings, `//` line comments and `/* ... */` block comments.
fn scan_line(line: &str) -> (i32, i32, bool) {
    let mut opens = 0i32;
    let mut closes = 0i32;
    let mut in_string = false;
    let mut in_block = false;
    let mut string_char = '"';
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if in_block {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block = false;
            }
            continue;
        }

        // Handle line comments — stop counting
        if !in_string && c == '/' && chars.peek() == Some(&'/') {
            break;
        }

        // Handle block comment start
        if !in_string && c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_block = true;
            continue;
        }

        // Handle string start/end
        if !in_string && (c == '"' || c == '\'') {
            in_string = true;
            string_char = c;
            continue;
        }
        if in_string {
            if c == '\\' {
                // Skip escaped character
                chars.next();
                continue;
            }
            if c == string_char {
                in_string = false;
            }
            continue;
        }

        // Count braces, brackets, and parens outside strings
        if c == '{' || c == '[' || c == '(' {
            opens += 1;
        } else if c == '}' || c == ']' || c == ')' {
            closes += 1;
        }
    }

    (opens, closes, in_block)
}

/// Line-based re-indenter, used only for source the lexer rejects (see
/// [`format_source`]): it tracks bracket depth while skipping strings and
/// comments, and fixes indentation without touching anything else.
fn reindent_lines(source: &str) -> String {
    let mut output = String::new();
    let mut indent_level: i32 = 0;
    let mut prev_blank = false;
    let mut in_block_comment = false;

    for line in source.lines() {
        // Inside a multi-line `/* ... */` comment the body is preserved
        // verbatim (only trailing whitespace is trimmed) so aligned text,
        // ASCII art and nested indentation survive `forge fmt`.
        if in_block_comment {
            prev_blank = false;
            match line.find("*/") {
                None => {
                    output.push_str(line.trim_end());
                    output.push('\n');
                }
                Some(idx) => {
                    let (comment, rest) = line.split_at(idx + 2);
                    in_block_comment = false;
                    output.push_str(comment.trim_end());
                    let rest = rest.trim();
                    if !rest.is_empty() {
                        output.push(' ');
                        output.push_str(rest);
                        let (opens, closes, still_in_block) = scan_line(rest);
                        indent_level = (indent_level + opens - closes).max(0);
                        in_block_comment = still_in_block;
                    }
                    output.push('\n');
                }
            }
            continue;
        }

        let trimmed = line.trim();

        // Collapse multiple blank lines into one
        if trimmed.is_empty() {
            if !prev_blank {
                output.push('\n');
                prev_blank = true;
            }
            continue;
        }
        prev_blank = false;

        let (opens, closes, ends_in_block) = scan_line(trimmed);
        in_block_comment = ends_in_block;
        let leading_closes = count_leading_closes(trimmed);

        // Decrease indent for leading close braces (before writing the line)
        indent_level -= leading_closes;
        if indent_level < 0 {
            indent_level = 0;
        }

        let indent = "    ".repeat(indent_level as usize);
        output.push_str(&indent);
        output.push_str(trimmed);
        output.push('\n');

        // Adjust indent for remaining braces (opens minus non-leading closes)
        let trailing_closes = closes - leading_closes;
        indent_level += opens - trailing_closes;
        if indent_level < 0 {
            indent_level = 0;
        }
    }

    // Ensure trailing newline
    if !output.ends_with('\n') {
        output.push('\n');
    }

    // Remove trailing blank lines
    while output.ends_with("\n\n") {
        output.pop();
    }

    output
}

/// Format Forge source.
///
/// The formatter works on the lexer's token stream (plus the comments the
/// lexer collects on request), never on regexes over text:
///
/// * **Line structure is kept.** Every statement stays on its line; the
///   only lines joined are an `else {` / `else if` / `catch e {` line
///   following a line that is just `}` (the parser skips those newlines,
///   so the program is unchanged).
/// * **Indentation** is four spaces per open bracket *line*: a line that
///   leaves brackets open indents the following lines one level, however
///   many brackets it opened (`f(x, fn() {` indents once), and a line that
///   starts with closers returns to the level of the line that opened them.
/// * **Spacing** between tokens on a line is normalised: one space around
///   binary operators, `=`, `->`, `=>`, after `,` `:` and keywords, inside
///   non-empty `{ }`; none inside `( )` / `[ ]`, before `,` `:` `)` `]`,
///   around `.` `..`, after `@`, `!`, `...` and a unary `-`, and between a
///   callee and its `(` / a value and its `[`. Where a token's role is
///   ambiguous at the token level (`<`/`>` as comparison or generic
///   brackets, `?`, `|`, `&`, `(` after a keyword), the original choice of
///   "space or no space" is kept.
/// * **Comments** are preserved: block comment bodies verbatim (only
///   trailing whitespace is trimmed), and the original gap before a
///   trailing `//` comment is kept so aligned comments stay aligned.
/// * **Literals are copied byte-for-byte from the source**, so string
///   contents, escapes, interpolations and number spellings never change.
/// * Runs of blank lines collapse to one; trailing whitespace and trailing
///   blank lines are removed.
///
/// Because only whitespace between tokens changes (and newlines only where
/// the parser ignores them), the formatted program has the same token
/// stream and the same AST. Formatting is idempotent.
///
/// Source the lexer rejects (e.g. an unterminated string) is only
/// re-indented line by line, so `forge fmt` and the LSP still work while a
/// file is being edited.
pub fn format_source(source: &str) -> String {
    match format_tokens(source) {
        Ok(formatted) => formatted,
        Err(_) => reindent_lines(source),
    }
}

/// One token or comment, with its position in the source (in characters).
#[derive(Clone, Copy)]
struct Item<'t> {
    /// `None` for a comment.
    token: Option<&'t Token>,
    /// For comments: `true` for `/* */`.
    block: bool,
    offset: usize,
    len: usize,
}

impl Item<'_> {
    fn is_comment(&self) -> bool {
        self.token.is_none()
    }
    fn is_line_comment(&self) -> bool {
        self.token.is_none() && !self.block
    }
    fn is(&self, t: &Token) -> bool {
        self.token == Some(t)
    }
}

/// Token-stream formatter; see [`format_source`]. Fails only when the
/// source does not lex.
pub fn format_tokens(source: &str) -> Result<String, LexError> {
    let chars: Vec<char> = source.chars().collect();
    let (tokens, comments) = Lexer::new(source).tokenize_with_comments()?;

    // Merge tokens and comments in source order, one Vec per source line.
    let mut lines: Vec<Vec<Item>> = vec![Vec::new()];
    let mut comments = comments.iter().peekable();
    for tok in &tokens {
        while let Some(c) = comments.next_if(|c| c.offset < tok.offset) {
            lines
                .last_mut()
                .expect("BUG: lines is never empty")
                .push(Item {
                    token: None,
                    block: c.block,
                    offset: c.offset,
                    len: c.len,
                });
        }
        match tok.token {
            Token::Eof => {}
            // A zero-length newline is the lexer's marker for a multi-line
            // block comment; the comment text carries the real line breaks.
            Token::Newline if tok.len == 0 => {}
            Token::Newline => lines.push(Vec::new()),
            ref t => lines
                .last_mut()
                .expect("BUG: lines is never empty")
                .push(Item {
                    token: Some(t),
                    block: false,
                    offset: tok.offset,
                    len: tok.len,
                }),
        }
    }
    for c in comments {
        lines
            .last_mut()
            .expect("BUG: lines is never empty")
            .push(Item {
                token: None,
                block: c.block,
                offset: c.offset,
                len: c.len,
            });
    }

    join_else_lines(&mut lines);

    let mut out = String::new();
    // Indent level of the line that opened each still-open bracket.
    let mut open: Vec<usize> = Vec::new();
    let mut prev_blank = false;
    for line in &lines {
        if line.is_empty() {
            if !prev_blank {
                out.push('\n');
                prev_blank = true;
            }
            continue;
        }
        prev_blank = false;

        let mut level = open.last().map_or(0, |owner| owner + 1);
        for (index, item) in line.iter().enumerate() {
            match item.token {
                Some(Token::LBrace | Token::LBracket | Token::LParen) => open.push(level),
                Some(Token::RBrace | Token::RBracket | Token::RParen) => {
                    let owner = open.pop().unwrap_or(0);
                    if index == 0 {
                        // A line that starts by closing a bracket sits at
                        // the level of the line that opened it.
                        level = owner;
                    }
                }
                _ => {}
            }
        }

        let mut text = "    ".repeat(level);
        let mut prev: Option<(&Item, Option<bool>)> = None;
        for item in line {
            if let Some((p, p_unary)) = prev {
                let gap: String = chars[p.offset + p.len..item.offset].iter().collect();
                text.push_str(&spacing(p, p_unary, item, &gap));
            }
            push_item_text(&mut text, &chars, item);
            let unary = match item.token {
                Some(Token::Minus) => is_unary_position(prev.map(|(p, _)| p)),
                _ => None,
            };
            prev = Some((item, unary));
        }
        out.push_str(text.trim_end());
        out.push('\n');
    }

    while out.ends_with("\n\n") {
        out.pop();
    }
    if out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

/// Join a lone `}` line + newline + `else {` / `else if` / `catch e {`
/// into one line.
/// Only these shapes are joined: in a `when`, `else -> value` arms must stay
/// on their own line.
fn join_else_lines(lines: &mut Vec<Vec<Item>>) {
    let mut i = 1;
    while i < lines.len() {
        let joinable = {
            let line = &lines[i];
            let continues = match (line.first().and_then(|t| t.token), line.get(1)) {
                (Some(Token::Else | Token::Otherwise | Token::Nah), Some(next)) => {
                    next.is(&Token::LBrace) || next.is(&Token::If)
                }
                (Some(Token::Catch), Some(next)) => {
                    matches!(next.token, Some(Token::Ident(_)))
                        && line.get(2).is_some_and(|t| t.is(&Token::LBrace))
                }
                _ => false,
            };
            continues
        };
        if joinable {
            // The closest non-blank line above must end with `}`.
            let mut j = i - 1;
            while j > 0 && lines[j].is_empty() {
                j -= 1;
            }
            // Join only after a multi-line block (its `}` opens the line);
            // one-line `if c { a }` / `else if d { b }` chains stay as written.
            let prev = &lines[j];
            if prev.len() == 1 && prev[0].is(&Token::RBrace) {
                let moved = std::mem::take(&mut lines[i]);
                lines[j].extend(moved);
                lines.drain(j + 1..=i);
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
}

fn push_item_text(out: &mut String, chars: &[char], item: &Item) {
    let raw = chars[item.offset..item.offset + item.len].iter();
    if item.is_comment() && item.block {
        // Comment bodies are kept verbatim except for trailing whitespace
        // on each of their lines.
        let text: String = raw.collect();
        let mut lines = text.split('\n').peekable();
        while let Some(line) = lines.next() {
            out.push_str(line.trim_end());
            if lines.peek().is_some() {
                out.push('\n');
            }
        }
    } else {
        out.extend(raw);
    }
}

/// Tokens after which a `-` is binary (they end an operand).
fn ends_operand(t: &Token) -> bool {
    matches!(
        t,
        Token::Ident(_)
            | Token::Int(_)
            | Token::Float(_)
            | Token::StringLit(_)
            | Token::RawStringLit(_)
            | Token::Bool(_)
            | Token::True
            | Token::False
            | Token::NullLit
            | Token::RParen
            | Token::RBracket
            | Token::RBrace
            | Token::Question
    )
}

/// Punctuation and operators — everything that is not a word or literal.
fn is_punct(t: &Token) -> bool {
    matches!(
        t,
        Token::Plus
            | Token::Minus
            | Token::Star
            | Token::Slash
            | Token::Percent
            | Token::Eq
            | Token::EqEq
            | Token::NotEq
            | Token::Lt
            | Token::Gt
            | Token::LtEq
            | Token::GtEq
            | Token::And
            | Token::Or
            | Token::Not
            | Token::Pipe
            | Token::Bar
            | Token::Question
            | Token::Arrow
            | Token::FatArrow
            | Token::Dot
            | Token::DotDot
            | Token::DotDotDot
            | Token::Ampersand
            | Token::PipeRight
            | Token::PlusEq
            | Token::MinusEq
            | Token::StarEq
            | Token::SlashEq
            | Token::PercentEq
            | Token::LParen
            | Token::RParen
            | Token::LBrace
            | Token::RBrace
            | Token::LBracket
            | Token::RBracket
            | Token::Comma
            | Token::Colon
            | Token::Semicolon
            | Token::At
    )
}

fn is_keyword(t: &Token) -> bool {
    !is_punct(t)
        && !matches!(
            t,
            Token::Ident(_)
                | Token::Int(_)
                | Token::Float(_)
                | Token::StringLit(_)
                | Token::RawStringLit(_)
                | Token::Bool(_)
                | Token::Newline
                | Token::Eof
        )
}

/// Whether a `-` following `prev` is a prefix minus: `Some(true)` after an
/// operator, opening bracket, separator or at the start of a line;
/// `Some(false)` after an operand; `None` (keep the original spacing) when
/// the previous token is a keyword or something ambiguous.
fn is_unary_position(prev: Option<&Item>) -> Option<bool> {
    let Some(prev) = prev else {
        return Some(true);
    };
    let t = prev.token?;
    if ends_operand(t) {
        Some(false)
    } else if is_keyword(t) || matches!(t, Token::Bar | Token::Ampersand) {
        None
    } else {
        Some(true)
    }
}

/// The whitespace to put between two adjacent items on one line. `gap` is
/// the original whitespace between them; `a_unary` says whether `a` (when
/// it is a `-`) is a prefix minus.
fn spacing(a: &Item, a_unary: Option<bool>, b: &Item, gap: &str) -> String {
    let keep = || {
        if gap.is_empty() {
            String::new()
        } else {
            " ".to_string()
        }
    };
    let none = String::new;
    let one = || " ".to_string();

    // A trailing `//` comment keeps its original alignment.
    if b.is_line_comment() {
        return if gap.is_empty() {
            one()
        } else {
            gap.to_string()
        };
    }
    let (Some(ta), Some(tb)) = (a.token, b.token) else {
        // Next to a block comment: keep the original choice.
        return keep();
    };

    // Tokens whose role is ambiguous at the token level (`>>` is both the
    // pipe-chain operator and the end of `Option<Option<Int>>`).
    let ambiguous = |t: &Token| {
        matches!(
            t,
            Token::Lt
                | Token::Gt
                | Token::PipeRight
                | Token::Question
                | Token::Bar
                | Token::Ampersand
        )
    };
    if ambiguous(ta) || ambiguous(tb) {
        return keep();
    }

    // No space after openers and prefix operators.
    if matches!(
        ta,
        Token::LParen
            | Token::LBracket
            | Token::Dot
            | Token::DotDot
            | Token::DotDotDot
            | Token::At
            | Token::Not
    ) {
        return none();
    }
    if matches!(ta, Token::Minus) {
        match a_unary {
            Some(true) => return none(),
            None => return keep(),
            Some(false) => {}
        }
    }
    // No space before closers and separators.
    if matches!(
        tb,
        Token::RParen
            | Token::RBracket
            | Token::Comma
            | Token::Semicolon
            | Token::Colon
            | Token::Dot
            | Token::DotDot
    ) {
        return none();
    }
    // Braces: `{}` when empty, `{ x }` otherwise.
    if matches!(ta, Token::LBrace) {
        return if matches!(tb, Token::RBrace) {
            none()
        } else {
            one()
        };
    }
    // Calls and indexing hug their operand.
    if matches!(tb, Token::LParen) {
        if matches!(
            ta,
            Token::Ident(_) | Token::RParen | Token::RBracket | Token::Fn | Token::Define
        ) {
            return none();
        }
        if is_keyword(ta) {
            // `say (x)` vs `say(x)`, `if (x)`: either reads fine.
            return keep();
        }
    }
    if matches!(tb, Token::LBracket) {
        if matches!(
            ta,
            Token::Ident(_)
                | Token::RParen
                | Token::RBracket
                | Token::StringLit(_)
                | Token::RawStringLit(_)
        ) {
            return none();
        }
        if is_keyword(ta) || matches!(ta, Token::RBrace) {
            return keep();
        }
    }
    one()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_type_annotations_intact() {
        let src = "let f: fn(Int, String) -> Bool = g\nlet o: Option<Option<Int>> = None\nlet t: (Int, String) = (1, \"a\")\n";
        assert_eq!(format_source(src), src);
        assert_eq!(
            format_source("let f:fn(Int,String)->Bool=g\n"),
            "let f: fn(Int, String) -> Bool = g\n"
        );
    }

    #[test]
    fn formats_basic_indentation() {
        let input = "fn greet(name) {\nprintln(name)\n}\n";
        let result = format_source(input);
        assert!(result.contains("    println(name)"));
    }

    #[test]
    fn preserves_correct_indentation() {
        let input = "let x = 42\nlet y = 10\n";
        let result = format_source(input);
        assert_eq!(result, input);
    }

    #[test]
    fn ignores_braces_in_strings() {
        let input = "let s = \"hello { world }\"\nsay s\n";
        let result = format_source(input);
        // Braces inside strings should NOT affect indentation
        assert_eq!(result, input);
    }

    #[test]
    fn ignores_braces_in_comments() {
        let input = "let x = 1 // this { brace\nsay x\n";
        let result = format_source(input);
        assert_eq!(result, input);
    }

    #[test]
    fn handles_else_blocks() {
        let input = "if true {\nsay \"yes\"\n} else {\nsay \"no\"\n}\n";
        let result = format_source(input);
        assert!(result.contains("} else {"));
        assert!(result.contains("    say \"yes\""));
        assert!(result.contains("    say \"no\""));
    }

    #[test]
    fn strips_trailing_whitespace() {
        let input = "let x = 42   \nlet y = 10  \n";
        let result = format_source(input);
        assert_eq!(result, "let x = 42\nlet y = 10\n");
    }

    #[test]
    fn collapses_multiple_blank_lines() {
        let input = "let x = 1\n\n\n\nlet y = 2\n";
        let result = format_source(input);
        assert_eq!(result, "let x = 1\n\nlet y = 2\n");
    }

    #[test]
    fn handles_nested_braces() {
        let input = "fn outer() {\nif true {\nsay \"nested\"\n}\n}\n";
        let result = format_source(input);
        assert!(result.contains("    if true {"));
        assert!(result.contains("        say \"nested\""));
        assert!(result.contains("    }"));
    }

    #[test]
    fn count_delimiters_ignores_strings() {
        assert_eq!(count_delimiters("let x = \"{\""), (0, 0));
        assert_eq!(count_delimiters("if true {"), (1, 0));
        assert_eq!(count_delimiters("}"), (0, 1));
        assert_eq!(count_delimiters("} else {"), (1, 1));
        assert_eq!(count_delimiters("let s = \"} else {\""), (0, 0));
    }

    #[test]
    fn count_delimiters_includes_brackets() {
        assert_eq!(count_delimiters("let a = ["), (1, 0));
        assert_eq!(count_delimiters("]"), (0, 1));
        assert_eq!(count_delimiters("[{"), (2, 0));
        assert_eq!(count_delimiters("}]"), (0, 2));
    }

    #[test]
    fn handles_bracket_indentation() {
        let input = "let a = [\n1,\n2,\n3\n]\n";
        let result = format_source(input);
        assert_eq!(result, "let a = [\n    1,\n    2,\n    3\n]\n");
    }

    #[test]
    fn handles_paren_continuation() {
        let input = "let result = some_function(\narg1,\narg2,\narg3\n)\n";
        let result = format_source(input);
        assert_eq!(
            result,
            "let result = some_function(\n    arg1,\n    arg2,\n    arg3\n)\n"
        );
    }

    #[test]
    fn preserves_block_comment_indentation() {
        let input = "fn main() {\n/*\n  Usage:\n      forge run x.fg\n  * bullet\n*/\nsay 1\n}\n";
        let result = format_source(input);
        assert_eq!(
            result,
            "fn main() {\n    /*\n  Usage:\n      forge run x.fg\n  * bullet\n*/\n    say 1\n}\n"
        );
    }

    #[test]
    fn ignores_braces_in_block_comments() {
        let input = "/* { [ (\n   } */\nlet x = 1 /* { */\nsay x\n";
        let result = format_source(input);
        assert_eq!(result, input);
    }

    #[test]
    fn block_comment_keeps_blank_lines_and_code_after_close() {
        let input = "/*\n\n\n  a\n*/ if true {\nsay 1\n}\n";
        let result = format_source(input);
        assert_eq!(result, "/*\n\n\n  a\n*/ if true {\n    say 1\n}\n");
    }

    #[test]
    fn block_comment_markers_inside_strings_are_ignored() {
        let input = "let s = \"/*\"\nif true {\nsay s\n}\n";
        let result = format_source(input);
        assert_eq!(result, "let s = \"/*\"\nif true {\n    say s\n}\n");
    }

    #[test]
    fn formatting_is_idempotent() {
        let inputs = [
            "fn greet(name) {\nprintln(name)\n}\n",
            "if true {\nsay \"yes\"\n} else {\nsay \"no\"\n}\n",
            "let a = [\n1,\n2\n]\n\n\n\nlet b = f(\nx\n)\n",
            "fn f() {\n/*\n   keep   me\n\t tabbed\n*/\nreturn {\na: 1\n}\n}\n",
            "/* one-line */ let x = 1\n// line { comment\nsay x\n",
        ];
        for input in inputs {
            let once = format_source(input);
            let twice = format_source(&once);
            assert_eq!(once, twice, "formatter not idempotent for {:?}", input);
        }
    }

    #[test]
    fn count_delimiters_includes_parens() {
        assert_eq!(count_delimiters("fn call("), (1, 0));
        assert_eq!(count_delimiters(")"), (0, 1));
        assert_eq!(count_delimiters("fn call(arg) {"), (2, 1));
    }

    fn fmt(input: &str) -> String {
        format_source(input)
    }

    #[test]
    fn normalizes_operator_and_comma_spacing() {
        assert_eq!(fmt("let x=1+2*3\n"), "let x = 1 + 2 * 3\n");
        assert_eq!(fmt("f( a ,b,c )\n"), "f(a, b, c)\n");
        assert_eq!(fmt("let a=[1 ,2,  3]\n"), "let a = [1, 2, 3]\n");
        assert_eq!(fmt("x+=1\ny  ==  z\n"), "x += 1\ny == z\n");
        assert_eq!(fmt("let r = a|>f\n"), "let r = a |> f\n");
        assert_eq!(fmt("let ok = a&&b||!c\n"), "let ok = a && b || !c\n");
    }

    #[test]
    fn normalizes_braces_and_colons() {
        assert_eq!(fmt("let o = {a:1,b : 2}\n"), "let o = { a: 1, b: 2 }\n");
        assert_eq!(fmt("let e = {}\n"), "let e = {}\n");
        assert_eq!(fmt("fn f(x:Int)->Int{x}\n"), "fn f(x: Int) -> Int { x }\n");
        assert_eq!(
            fmt("if x{\nsay 1\n}else{\nsay 2\n}\n"),
            "if x {\n    say 1\n} else {\n    say 2\n}\n"
        );
    }

    #[test]
    fn joins_else_and_catch_onto_the_closing_brace() {
        assert_eq!(
            fmt("if x {\nsay 1\n}\nelse {\nsay 2\n}\n"),
            "if x {\n    say 1\n} else {\n    say 2\n}\n"
        );
        assert_eq!(
            fmt("if x {\n  a()\n}\n\notherwise if y {\n  b()\n}\n"),
            "if x {\n    a()\n} otherwise if y {\n    b()\n}\n"
        );
        assert_eq!(
            fmt("try {\nf()\n}\ncatch e {\nsay e\n}\n"),
            "try {\n    f()\n} catch e {\n    say e\n}\n"
        );
        // One-line chains are left as written.
        let chain = "if a { x() }
otherwise if b { y() }
otherwise { z() }
";
        assert_eq!(fmt(chain), chain);
        // `else ->` arms of a `when` stay on their own line.
        let when = "when x {\n    < 5 -> \"small\",\n    else -> \"big\"\n}\n";
        assert_eq!(fmt(when), when);
    }

    #[test]
    fn unary_minus_and_prefix_operators_hug_their_operand() {
        assert_eq!(fmt("let a = - 1\n"), "let a = -1\n");
        assert_eq!(fmt("f(- x, y - 1)\n"), "f(-x, y - 1)\n");
        assert_eq!(fmt("return -1\n"), "return -1\n");
        assert_eq!(fmt("let b = ! ok\n"), "let b = !ok\n");
        assert_eq!(fmt("let r = [1 .. 5]\n"), "let r = [1..5]\n");
        assert_eq!(fmt("let s = [... a, ...b]\n"), "let s = [...a, ...b]\n");
    }

    #[test]
    fn calls_indexes_fields_and_decorators_are_tight() {
        assert_eq!(fmt("obj . method (1) [0]\n"), "obj.method(1)[0]\n");
        assert_eq!(fmt("let f = fn (x) { x }\n"), "let f = fn(x) { x }\n");
        assert_eq!(fmt("@ get(\"/x\")\n"), "@get(\"/x\")\n");
        assert_eq!(
            fmt("let c = term.table (rows)\n"),
            "let c = term.table (rows)\n"
        );
    }

    #[test]
    fn ambiguous_tokens_keep_their_original_spacing() {
        let src = "let m: Map<String, Int> = x\nlet t = a<b\nlet u = a < b\nlet v = f()?\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn string_contents_and_number_spellings_are_untouched() {
        let src = "let s = \"a  =b ,{x+1}  \"\nlet n = 1_000_000\nlet r = \"\"\"\n  raw  =  text   \n\"\"\"\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn comments_are_preserved() {
        assert_eq!(
            fmt("let x=1   // aligned\nlet yy=2  // aligned\n"),
            "let x = 1   // aligned\nlet yy = 2  // aligned\n"
        );
        assert_eq!(fmt("let x = /*inline*/ 1\n"), "let x = /*inline*/ 1\n");
        assert_eq!(
            fmt("fn f() {\n/* a\n   b   \n*/\nreturn 1\n}\n"),
            "fn f() {\n    /* a\n   b\n*/\n    return 1\n}\n"
        );
    }

    #[test]
    fn one_indent_level_per_line_of_open_brackets() {
        assert_eq!(
            fmt("server.get(\"/\", fn(req) {\nreturn 1\n})\n"),
            "server.get(\"/\", fn(req) {\n    return 1\n})\n"
        );
        assert_eq!(
            fmt("let x = [\nf(\n1\n)]\n"),
            "let x = [\n    f(\n        1\n    )]\n"
        );
    }

    #[test]
    fn unlexable_source_is_only_reindented() {
        let src = "fn f() {\nlet s = \"unterminated\n}\n";
        assert_eq!(fmt(src), reindent_lines(src));
    }

    /// Every `.fg` file under `examples/` and `tests/` must format to a
    /// program with the same tokens and the same AST, and formatting must
    /// be idempotent. (`tests/fmt_corpus.rs` additionally runs the
    /// formatted programs on both engines and compares their output.)
    #[test]
    fn corpus_formats_to_the_same_program() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        collect_fg_files(&root.join("examples"), &mut files);
        collect_fg_files(&root.join("tests"), &mut files);
        files.sort();
        assert!(
            files.len() > 40,
            "corpus not found under {}",
            root.display()
        );
        for path in files {
            let source = std::fs::read_to_string(&path).unwrap();
            let Ok(tokens) = Lexer::new(&source).tokenize() else {
                continue; // deliberately malformed fixture
            };
            let formatted = format_source(&source);
            let name = path.display();
            assert_eq!(
                format_source(&formatted),
                formatted,
                "formatting is not idempotent for {}",
                name
            );
            let new_tokens = Lexer::new(&formatted)
                .tokenize()
                .unwrap_or_else(|e| panic!("{} no longer lexes: {}\n{}", name, e, formatted));
            let significant = |ts: Vec<crate::lexer::token::Spanned>| -> Vec<Token> {
                ts.into_iter()
                    .map(|t| t.token)
                    .filter(|t| !matches!(t, Token::Newline))
                    .collect()
            };
            assert_eq!(
                significant(tokens),
                significant(new_tokens),
                "tokens changed in {}",
                name
            );
            let ast = |src: &str| {
                let tokens = Lexer::new(src).tokenize().unwrap();
                crate::parser::Parser::new(tokens)
                    .parse_program()
                    .map(|p| strip_spans(&format!("{:?}", p)))
                    .map_err(|e| e.to_string())
            };
            let before = ast(&source);
            if before.is_err() {
                continue; // a parse-error fixture
            }
            assert_eq!(before, ast(&formatted), "AST changed in {}", name);
        }
    }

    /// Remove `line: N, col: N` from an AST debug dump.
    fn strip_spans(debug: &str) -> String {
        let mut out = String::with_capacity(debug.len());
        let mut rest = debug;
        while let Some(i) = rest.find(", line: ") {
            out.push_str(&rest[..i]);
            let after = &rest[i + ", line: ".len()..];
            let digits = after.chars().take_while(|c| c.is_ascii_digit()).count();
            let after = &after[digits..];
            let after = after.strip_prefix(", col: ").unwrap_or(after);
            let digits = after.chars().take_while(|c| c.is_ascii_digit()).count();
            rest = &after[digits..];
        }
        out.push_str(rest);
        out
    }
}
