/// Forge Lexer
/// Hand-rolled for full control over string interpolation and error reporting.
/// Will migrate to `logos` in Phase 3 for performance.
use super::token::{Spanned, Token};

/// A comment found while lexing. The parser never sees comments; tools that
/// must preserve them (the formatter) ask for them with
/// [`Lexer::tokenize_with_comments`].
#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    /// The full comment text including its delimiters (`// ...` or `/* ... */`).
    pub text: String,
    /// `true` for `/* ... */`, `false` for `// ...`.
    pub block: bool,
    pub line: usize,
    pub col: usize,
    /// Character offset of the comment's first character.
    pub offset: usize,
    /// Length in characters.
    pub len: usize,
}

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
    /// Collected comments, when the caller asked for them.
    comments: Option<Vec<Comment>>,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
            comments: None,
        }
    }

    /// Like [`Lexer::tokenize`], but also returns every comment in source
    /// order. The token stream is identical to `tokenize`'s.
    pub fn tokenize_with_comments(&mut self) -> Result<(Vec<Spanned>, Vec<Comment>), LexError> {
        self.comments = Some(Vec::new());
        let tokens = self.tokenize()?;
        Ok((tokens, self.comments.take().unwrap_or_default()))
    }

    pub fn tokenize(&mut self) -> Result<Vec<Spanned>, LexError> {
        let mut tokens = Vec::new();

        while self.pos < self.source.len() {
            self.skip_whitespace_except_newline();

            if self.pos >= self.source.len() {
                break;
            }

            let ch = self.current();

            // Skip comments
            if ch == '/' && self.peek() == Some('/') {
                self.skip_line_comment();
                continue;
            }
            if ch == '/' && self.peek() == Some('*') {
                let (line, col, offset) = (self.line, self.col, self.pos);
                self.skip_block_comment()?;
                // A block comment that spans lines separates statements the
                // same way the newlines inside it would have (as in Go), so
                // `let a = 1 /* ... \n ... */ let b = 2` stays two statements.
                if self.line > line {
                    tokens.push(Spanned::new(Token::Newline, line, col, offset, 0));
                }
                continue;
            }

            let start_line = self.line;
            let start_col = self.col;
            let start_offset = self.pos;

            let token = match ch {
                '\n' => {
                    self.advance();
                    Token::Newline
                }

                // Numbers
                '0'..='9' => self.lex_number()?,

                // Strings (triple-quoted or single-quoted)
                '"' => {
                    if self.peek() == Some('"') && self.peek_at(2) == Some('"') {
                        self.lex_triple_string()?
                    } else {
                        self.lex_string()?
                    }
                }

                // Identifiers and keywords
                'a'..='z' | 'A'..='Z' | '_' => self.lex_ident(),

                // Operators and delimiters
                '+' => {
                    self.advance();
                    if self.current_matches('=') {
                        self.advance();
                        Token::PlusEq
                    } else {
                        Token::Plus
                    }
                }
                '-' => {
                    self.advance();
                    if self.current_matches('>') {
                        self.advance();
                        Token::Arrow
                    } else if self.current_matches('=') {
                        self.advance();
                        Token::MinusEq
                    } else {
                        Token::Minus
                    }
                }
                '*' => {
                    self.advance();
                    if self.current_matches('=') {
                        self.advance();
                        Token::StarEq
                    } else {
                        Token::Star
                    }
                }
                '/' => {
                    self.advance();
                    if self.current_matches('=') {
                        self.advance();
                        Token::SlashEq
                    } else {
                        Token::Slash
                    }
                }
                '%' => {
                    self.advance();
                    if self.current_matches('=') {
                        self.advance();
                        Token::PercentEq
                    } else {
                        Token::Percent
                    }
                }
                '=' => {
                    self.advance();
                    if self.current_matches('=') {
                        self.advance();
                        Token::EqEq
                    } else if self.current_matches('>') {
                        self.advance();
                        Token::FatArrow
                    } else {
                        Token::Eq
                    }
                }
                '!' => {
                    self.advance();
                    if self.current_matches('=') {
                        self.advance();
                        Token::NotEq
                    } else {
                        Token::Not
                    }
                }
                '<' => {
                    self.advance();
                    if self.current_matches('=') {
                        self.advance();
                        Token::LtEq
                    } else {
                        Token::Lt
                    }
                }
                '>' => {
                    self.advance();
                    if self.current_matches('=') {
                        self.advance();
                        Token::GtEq
                    } else if self.current_matches('>') {
                        self.advance();
                        Token::PipeRight
                    } else {
                        Token::Gt
                    }
                }
                '&' => {
                    self.advance();
                    if self.current_matches('&') {
                        self.advance();
                        Token::And
                    } else {
                        Token::Ampersand
                    }
                }
                '|' => {
                    self.advance();
                    if self.current_matches('|') {
                        self.advance();
                        Token::Or
                    } else if self.current_matches('>') {
                        self.advance();
                        Token::Pipe
                    } else {
                        Token::Bar
                    }
                }
                '?' => {
                    self.advance();
                    Token::Question
                }
                '.' => {
                    self.advance();
                    if self.current_matches('.') {
                        self.advance();
                        if self.current_matches('.') {
                            self.advance();
                            Token::DotDotDot
                        } else {
                            Token::DotDot
                        }
                    } else {
                        Token::Dot
                    }
                }
                '@' => {
                    self.advance();
                    Token::At
                }

                // Delimiters
                '(' => {
                    self.advance();
                    Token::LParen
                }
                ')' => {
                    self.advance();
                    Token::RParen
                }
                '{' => {
                    self.advance();
                    Token::LBrace
                }
                '}' => {
                    self.advance();
                    Token::RBrace
                }
                '[' => {
                    self.advance();
                    Token::LBracket
                }
                ']' => {
                    self.advance();
                    Token::RBracket
                }
                ',' => {
                    self.advance();
                    Token::Comma
                }
                ':' => {
                    self.advance();
                    Token::Colon
                }
                ';' => {
                    self.advance();
                    Token::Semicolon
                }

                _ => return Err(self.error(&format!("unexpected character: '{}'", ch))),
            };

            let len = self.pos - start_offset;
            tokens.push(Spanned::new(
                token,
                start_line,
                start_col,
                start_offset,
                len,
            ));
        }

        tokens.push(Spanned::new(Token::Eof, self.line, self.col, self.pos, 0));
        Ok(tokens)
    }

    // --- Lexing helpers ---

    fn lex_number(&mut self) -> Result<Token, LexError> {
        let mut num_str = String::new();
        let mut is_float = false;

        while self.pos < self.source.len() {
            let ch = self.current();
            if ch.is_ascii_digit() {
                num_str.push(ch);
                self.advance();
            } else if ch == '.' && !is_float && self.peek().is_some_and(|c| c.is_ascii_digit()) {
                is_float = true;
                num_str.push(ch);
                self.advance();
            } else if ch == '_' {
                // Allow 1_000_000 style
                self.advance();
            } else {
                break;
            }
        }

        if is_float {
            num_str
                .parse::<f64>()
                .map(Token::Float)
                .map_err(|_| self.error("invalid float literal"))
        } else {
            num_str
                .parse::<i64>()
                .map(Token::Int)
                .map_err(|_| self.error("invalid integer literal"))
        }
    }

    fn lex_string(&mut self) -> Result<Token, LexError> {
        self.advance(); // skip opening "
        let mut result = String::new();

        while self.pos < self.source.len() {
            let ch = self.current();
            match ch {
                '"' => {
                    self.advance();
                    return Ok(Token::StringLit(result));
                }
                '\\' => {
                    self.advance();
                    if self.pos >= self.source.len() {
                        return Err(self.error("unexpected end of string"));
                    }
                    match self.current() {
                        'n' => {
                            result.push('\n');
                            self.advance();
                        }
                        't' => {
                            result.push('\t');
                            self.advance();
                        }
                        'r' => {
                            result.push('\r');
                            self.advance();
                        }
                        '\\' => {
                            result.push('\\');
                            self.advance();
                        }
                        '"' => {
                            result.push('"');
                            self.advance();
                        }
                        '{' => {
                            result.push('{');
                            self.advance();
                        }
                        '}' => {
                            result.push('}');
                            self.advance();
                        }
                        _ => {
                            return Err(self.error(&format!("unknown escape: \\{}", self.current())))
                        }
                    }
                }
                '\n' => return Err(self.error("unterminated string (newline in string literal)")),
                _ => {
                    // String interpolation: {expr} is kept as-is in the string for now
                    // The interpreter handles interpolation at runtime
                    result.push(ch);
                    self.advance();
                }
            }
        }

        Err(self.error("unterminated string"))
    }

    fn lex_triple_string(&mut self) -> Result<Token, LexError> {
        // Skip opening """
        self.advance(); // "
        self.advance(); // "
        self.advance(); // "

        // Skip leading newline if present
        if self.pos < self.source.len() && self.source[self.pos] == '\n' {
            self.advance();
        }

        let mut result = String::new();
        while self.pos < self.source.len() {
            if self.current() == '"' && self.peek() == Some('"') && self.peek_at(2) == Some('"') {
                self.advance(); // "
                self.advance(); // "
                self.advance(); // "
                return Ok(Token::RawStringLit(result));
            }
            result.push(self.current());
            self.advance();
        }
        Err(self.error("unterminated triple-quoted string"))
    }

    fn lex_ident(&mut self) -> Token {
        let start = self.pos;
        while self.pos < self.source.len()
            && (self.current().is_alphanumeric() || self.current() == '_')
        {
            self.advance();
        }

        let word: String = self.source[start..self.pos].iter().collect();

        // Check for keywords
        Token::keyword_from_str(&word).unwrap_or(Token::Ident(word))
    }

    // --- Navigation helpers ---

    fn current(&self) -> char {
        self.source[self.pos]
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.source.get(self.pos + offset).copied()
    }

    fn current_matches(&self, ch: char) -> bool {
        self.pos < self.source.len() && self.source[self.pos] == ch
    }

    fn advance(&mut self) {
        if self.pos < self.source.len() {
            if self.source[self.pos] == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            self.pos += 1;
        }
    }

    fn skip_whitespace_except_newline(&mut self) {
        while self.pos < self.source.len() {
            let ch = self.source[self.pos];
            if ch == ' ' || ch == '\t' || ch == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        let (line, col, start) = (self.line, self.col, self.pos);
        while self.pos < self.source.len() && self.source[self.pos] != '\n' {
            self.advance();
        }
        self.record_comment(false, line, col, start);
    }

    /// Skip a `/* ... */` comment. Block comments do not nest: the first
    /// `*/` closes the comment (see the spec, lexical-structure/comments).
    /// Line numbers stay correct because `advance` counts the newlines.
    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        let (line, col, start) = (self.line, self.col, self.pos);
        self.advance(); // '/'
        self.advance(); // '*'
        loop {
            if self.pos >= self.source.len() {
                return Err(LexError {
                    message: "unterminated block comment (missing `*/`)".to_string(),
                    line,
                    col,
                });
            }
            if self.current() == '*' && self.peek() == Some('/') {
                self.advance();
                self.advance();
                break;
            }
            self.advance();
        }
        self.record_comment(true, line, col, start);
        Ok(())
    }

    fn record_comment(&mut self, block: bool, line: usize, col: usize, start: usize) {
        if let Some(comments) = self.comments.as_mut() {
            comments.push(Comment {
                text: self.source[start..self.pos].iter().collect(),
                block,
                line,
                col,
                offset: start,
                len: self.pos - start,
            });
        }
    }

    fn error(&self, msg: &str) -> LexError {
        LexError {
            message: msg.to_string(),
            line: self.line,
            col: self.col,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}:{}] Lex error: {}",
            self.line, self.col, self.message
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(input: &str) -> Vec<Token> {
        Lexer::new(input)
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|s| s.token)
            .filter(|t| !matches!(t, Token::Newline | Token::Eof))
            .collect()
    }

    #[test]
    fn test_numbers() {
        assert_eq!(lex("42"), vec![Token::Int(42)]);
        assert_eq!(lex("3.14"), vec![Token::Float(3.14)]);
        assert_eq!(lex("1_000_000"), vec![Token::Int(1000000)]);
    }

    #[test]
    fn test_strings() {
        assert_eq!(lex(r#""hello""#), vec![Token::StringLit("hello".into())]);
        assert_eq!(
            lex(r#""line\nbreak""#),
            vec![Token::StringLit("line\nbreak".into())]
        );
    }

    #[test]
    fn test_keywords() {
        assert_eq!(
            lex("let fn return if else"),
            vec![Token::Let, Token::Fn, Token::Return, Token::If, Token::Else,]
        );
    }

    #[test]
    fn test_operators() {
        assert_eq!(
            lex("== != <= >= -> => |>"),
            vec![
                Token::EqEq,
                Token::NotEq,
                Token::LtEq,
                Token::GtEq,
                Token::Arrow,
                Token::FatArrow,
                Token::Pipe,
            ]
        );
    }

    #[test]
    fn test_simple_function() {
        let tokens = lex("fn greet(name: String) -> String { return \"hello\" }");
        assert!(tokens.contains(&Token::Fn));
        assert!(tokens.contains(&Token::Ident("greet".into())));
        assert!(tokens.contains(&Token::Arrow));
        assert!(tokens.contains(&Token::StringType));
    }

    #[test]
    fn test_comments_skipped() {
        assert_eq!(lex("42 // this is a comment"), vec![Token::Int(42)]);
    }

    fn lex_spanned(input: &str) -> Vec<Spanned> {
        Lexer::new(input).tokenize().unwrap()
    }

    #[test]
    fn block_comment_inline_is_skipped() {
        assert_eq!(
            lex("let x = /* the answer */ 42"),
            vec![
                Token::Let,
                Token::Ident("x".into()),
                Token::Eq,
                Token::Int(42)
            ]
        );
    }

    #[test]
    fn block_comment_multiline_keeps_line_numbers() {
        let tokens = lex_spanned("/* a\nb\nc */ let x = 1\nlet y = 2");
        let y = tokens
            .iter()
            .find(|t| t.token == Token::Ident("y".into()))
            .unwrap();
        assert_eq!((y.line, y.col), (4, 5));
        let x = tokens
            .iter()
            .find(|t| t.token == Token::Ident("x".into()))
            .unwrap();
        assert_eq!((x.line, x.col), (3, 10));
    }

    #[test]
    fn multiline_block_comment_separates_statements() {
        let tokens: Vec<Token> = lex_spanned("let a = 1 /* x\n y */ let b = 2")
            .into_iter()
            .map(|t| t.token)
            .collect();
        let newline_at = tokens.iter().position(|t| *t == Token::Newline).unwrap();
        assert_eq!(tokens[newline_at - 1], Token::Int(1));
        assert_eq!(tokens[newline_at + 1], Token::Let);
        // A single-line block comment does not introduce a newline.
        assert!(!lex_spanned("let a = /* x */ 1")
            .iter()
            .any(|t| t.token == Token::Newline));
    }

    #[test]
    fn block_comments_do_not_nest() {
        // The first `*/` closes the comment, so `c` is code.
        assert_eq!(lex("/* a /* b */ c"), vec![Token::Ident("c".into())]);
    }

    #[test]
    fn unterminated_block_comment_reports_its_start() {
        let err = Lexer::new("let x = 1\n  /* never closed\nlet y = 2")
            .tokenize()
            .unwrap_err();
        assert!(err.message.contains("unterminated block comment"));
        assert_eq!((err.line, err.col), (2, 3));
    }

    #[test]
    fn comment_markers_inside_strings_are_text() {
        assert_eq!(
            lex(r#""a /* b */ c // d""#),
            vec![Token::StringLit("a /* b */ c // d".into())]
        );
    }

    #[test]
    fn slash_star_operators_still_lex() {
        assert_eq!(
            lex("a / b * c"),
            vec![
                Token::Ident("a".into()),
                Token::Slash,
                Token::Ident("b".into()),
                Token::Star,
                Token::Ident("c".into()),
            ]
        );
    }

    #[test]
    fn tokenize_with_comments_collects_both_kinds() {
        let src = "let a = 1 // one\n/* two\n */ a";
        let (tokens, comments) = Lexer::new(src).tokenize_with_comments().unwrap();
        let plain = Lexer::new(src).tokenize().unwrap();
        let key = |t: &Spanned| (t.token.clone(), t.line, t.col, t.offset, t.len);
        assert_eq!(
            tokens.iter().map(key).collect::<Vec<_>>(),
            plain.iter().map(key).collect::<Vec<_>>()
        );
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].text, "// one");
        assert!(!comments[0].block);
        assert_eq!((comments[0].line, comments[0].col), (1, 11));
        assert_eq!(comments[1].text, "/* two\n */");
        assert!(comments[1].block);
        assert_eq!((comments[1].line, comments[1].col), (2, 1));
    }

    #[test]
    fn test_decorator() {
        let tokens = lex("@get");
        assert_eq!(tokens, vec![Token::At, Token::Ident("get".into())]);
    }

    #[test]
    fn test_triple_quoted_string() {
        let tokens = lex(r#""""hello world""""#);
        assert_eq!(tokens, vec![Token::RawStringLit("hello world".into())]);
    }

    #[test]
    fn test_compound_operators() {
        assert_eq!(lex("+="), vec![Token::PlusEq]);
        assert_eq!(lex("-="), vec![Token::MinusEq]);
        assert_eq!(lex("*="), vec![Token::StarEq]);
        assert_eq!(lex("/="), vec![Token::SlashEq]);
        assert_eq!(lex("%="), vec![Token::PercentEq]);
    }

    #[test]
    fn test_spread_operator() {
        assert_eq!(lex("..."), vec![Token::DotDotDot]);
    }

    #[test]
    fn test_pipe_right() {
        assert_eq!(lex(">>"), vec![Token::PipeRight]);
    }

    #[test]
    fn test_bar_operator() {
        assert_eq!(lex("| "), vec![Token::Bar]);
    }

    #[test]
    fn test_natural_keywords() {
        assert_eq!(lex("set"), vec![Token::Set]);
        assert_eq!(lex("to"), vec![Token::To]);
        assert_eq!(lex("change"), vec![Token::Change]);
        assert_eq!(lex("define"), vec![Token::Define]);
        assert_eq!(lex("say"), vec![Token::Say]);
        assert_eq!(lex("yell"), vec![Token::Yell]);
        assert_eq!(lex("whisper"), vec![Token::Whisper]);
        assert_eq!(lex("otherwise"), vec![Token::Otherwise]);
        assert_eq!(lex("nah"), vec![Token::Nah]);
    }

    #[test]
    fn test_innovation_keywords() {
        assert_eq!(lex("when"), vec![Token::When]);
        assert_eq!(lex("must"), vec![Token::Must]);
        assert_eq!(lex("safe"), vec![Token::Safe]);
        assert_eq!(lex("check"), vec![Token::Check]);
        assert_eq!(lex("retry"), vec![Token::Retry]);
        assert_eq!(lex("timeout"), vec![Token::Timeout]);
        assert_eq!(lex("freeze"), vec![Token::Freeze]);
        assert_eq!(lex("unless"), vec![Token::Unless]);
    }

    #[test]
    fn test_forge_vocabulary() {
        assert_eq!(lex("forge"), vec![Token::ForgeKw]);
        assert_eq!(lex("hold"), vec![Token::Hold]);
        assert_eq!(lex("emit"), vec![Token::Emit]);
        assert_eq!(lex("unpack"), vec![Token::Unpack]);
    }

    #[test]
    fn test_escape_sequences() {
        assert_eq!(lex(r#""\n""#), vec![Token::StringLit("\n".into())]);
        assert_eq!(lex(r#""\t""#), vec![Token::StringLit("\t".into())]);
        assert_eq!(lex(r#""\\""#), vec![Token::StringLit("\\".into())]);
        assert_eq!(lex(r#""\{""#), vec![Token::StringLit("{".into())]);
        assert_eq!(lex(r#""\}""#), vec![Token::StringLit("}".into())]);
    }
}
