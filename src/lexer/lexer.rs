/// Forge Lexer
/// Hand-rolled for full control over string interpolation and error reporting.
/// Will migrate to `logos` in Phase 3 for performance.
use super::token::{Spanned, Token};

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
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
                    Token::Percent
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
        while self.pos < self.source.len() && self.source[self.pos] != '\n' {
            self.advance();
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
