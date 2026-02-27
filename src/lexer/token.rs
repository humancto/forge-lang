/// Forge Token Definitions
/// Every atom of the Forge language.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // === Literals ===
    Int(i64),
    Float(f64),
    StringLit(String), // "hello, {name}"
    Bool(bool),

    // === Identifiers & Keywords ===
    Ident(String),

    // Keywords
    Let,
    Mut,
    Fn,
    Return,
    If,
    Else,
    Match,
    For,
    In,
    While,
    Loop,
    Break,
    Continue,
    Struct,
    Type,
    Interface,
    Impl,
    Pub,
    Import,
    Spawn,
    True,
    False,

    // Built-in type names
    IntType,    // Int
    FloatType,  // Float
    StringType, // String
    BoolType,   // Bool
    JsonType,   // Json

    // === Operators ===
    Plus,      // +
    Minus,     // -
    Star,      // *
    Slash,     // /
    Percent,   // %
    Eq,        // =
    EqEq,      // ==
    NotEq,     // !=
    Lt,        // <
    Gt,        // >
    LtEq,      // <=
    GtEq,      // >=
    And,       // &&
    Or,        // ||
    Not,       // !
    Pipe,      // |>
    Question,  // ?
    Arrow,     // ->
    FatArrow,  // =>
    Dot,       // .
    DotDot,    // ..
    Ampersand, // &

    // === Delimiters ===
    LParen,    // (
    RParen,    // )
    LBrace,    // {
    RBrace,    // }
    LBracket,  // [
    RBracket,  // ]
    Comma,     // ,
    Colon,     // :
    Semicolon, // ;

    // === Decorators ===
    At, // @

    // === Special ===
    Newline,
    Eof,
}

impl Token {
    /// Check if this token is a keyword
    pub fn keyword_from_str(s: &str) -> Option<Token> {
        match s {
            "let" => Some(Token::Let),
            "mut" => Some(Token::Mut),
            "fn" => Some(Token::Fn),
            "return" => Some(Token::Return),
            "if" => Some(Token::If),
            "else" => Some(Token::Else),
            "match" => Some(Token::Match),
            "for" => Some(Token::For),
            "in" => Some(Token::In),
            "while" => Some(Token::While),
            "loop" => Some(Token::Loop),
            "break" => Some(Token::Break),
            "continue" => Some(Token::Continue),
            "struct" => Some(Token::Struct),
            "type" => Some(Token::Type),
            "interface" => Some(Token::Interface),
            "impl" => Some(Token::Impl),
            "pub" => Some(Token::Pub),
            "import" => Some(Token::Import),
            "spawn" => Some(Token::Spawn),
            "true" => Some(Token::True),
            "false" => Some(Token::False),
            "Int" => Some(Token::IntType),
            "Float" => Some(Token::FloatType),
            "String" => Some(Token::StringType),
            "Bool" => Some(Token::BoolType),
            "Json" => Some(Token::JsonType),
            _ => None,
        }
    }
}

/// A token with its position in source code
#[derive(Debug, Clone)]
pub struct Spanned {
    pub token: Token,
    pub line: usize,
    pub col: usize,
    pub offset: usize,
    pub len: usize,
}

impl Spanned {
    pub fn new(token: Token, line: usize, col: usize, offset: usize, len: usize) -> Self {
        Self {
            token,
            line,
            col,
            offset,
            len,
        }
    }
}
