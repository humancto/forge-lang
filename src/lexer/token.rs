/// Forge Token Definitions
/// Every atom of the Forge language.

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum Token {
    // === Literals ===
    Int(i64),
    Float(f64),
    StringLit(String),    // "hello, {name}" — supports interpolation
    RawStringLit(String), // """raw""" — no interpolation
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
    NullLit,

    // Natural-language keywords
    Set,       // set name to value
    To,        // set name to value / change name to value
    Change,    // change name to value
    Define,    // define fn (alias for fn)
    Otherwise, // else alias
    Nah,       // else alias (fun mode)
    Each,      // for each x in items
    Repeat,    // repeat N times { }
    Times,     // repeat N times { }
    Grab,      // grab resp from "url"
    From,      // grab resp from "url"
    Wait,      // wait N seconds
    Seconds,   // wait N seconds
    Say,       // say "hello" (println)
    Yell,      // yell "HELLO" (uppercase println)
    Whisper,   // whisper "hello" (lowercase println)

    // Error handling
    TryKw, // try { }
    Catch, // catch err { }

    // Forge-unique vocabulary
    ForgeKw, // forge fn (async)
    Hold,    // hold expr (await)
    Emit,    // emit value (yield)
    Unpack,  // unpack {a,b} from obj

    // Classic equivalents
    Async, // async fn
    Await, // await expr
    Yield, // yield value

    // Advanced
    DotDotDot, // ... (spread)

    // Innovation tokens
    When,      // when guards
    Unless,    // postfix unless
    Until,     // postfix until
    Must,      // must expr (crash on error)
    Check,     // check validation
    Safe,      // safe { } blocks
    Where,     // collection where filter
    Timeout,   // timeout N seconds { }
    Retry,     // retry N times { }
    Schedule,  // schedule every N { }
    Every,     // every x in items
    Any,       // any x in items
    Ask,       // ask "prompt"
    Prompt,    // prompt name() { }
    Transform, // transform data { }
    Table,     // table [...]
    Select,    // from X select Y
    Order,     // order by
    By,        // sort by / order by
    Limit,     // limit N
    Keep,      // keep where
    Take,      // take N
    Freeze,    // freeze expr
    Watch,     // watch "file" { }
    PipeRight, // >> pipe operator
    Download,  // download url to path
    Crawl,     // crawl url
    PlusEq,    // +=
    MinusEq,   // -=
    StarEq,    // *=
    SlashEq,   // /=

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
    Bar,       // | (single bar, for ADT variants)
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
            "null" => Some(Token::NullLit),
            "set" => Some(Token::Set),
            "to" => Some(Token::To),
            "change" => Some(Token::Change),
            "define" => Some(Token::Define),
            "otherwise" => Some(Token::Otherwise),
            "nah" => Some(Token::Nah),
            "each" => Some(Token::Each),
            "repeat" => Some(Token::Repeat),
            "times" => Some(Token::Times),
            "grab" => Some(Token::Grab),
            "from" => Some(Token::From),
            "wait" => Some(Token::Wait),
            "seconds" => Some(Token::Seconds),
            "say" => Some(Token::Say),
            "yell" => Some(Token::Yell),
            "whisper" => Some(Token::Whisper),
            "try" => Some(Token::TryKw),
            "catch" => Some(Token::Catch),
            "forge" => Some(Token::ForgeKw),
            "hold" => Some(Token::Hold),
            "emit" => Some(Token::Emit),
            "unpack" => Some(Token::Unpack),
            "async" => Some(Token::Async),
            "await" => Some(Token::Await),
            "yield" => Some(Token::Yield),
            "when" => Some(Token::When),
            "unless" => Some(Token::Unless),
            "until" => Some(Token::Until),
            "must" => Some(Token::Must),
            "check" => Some(Token::Check),
            "safe" => Some(Token::Safe),
            "where" => Some(Token::Where),
            "timeout" => Some(Token::Timeout),
            "retry" => Some(Token::Retry),
            "schedule" => Some(Token::Schedule),
            "every" => Some(Token::Every),
            "any" => Some(Token::Any),
            "ask" => Some(Token::Ask),
            "prompt" => Some(Token::Prompt),
            "transform" => Some(Token::Transform),
            "table" => Some(Token::Table),
            "select" => Some(Token::Select),
            "order" => Some(Token::Order),
            "by" => Some(Token::By),
            "limit" => Some(Token::Limit),
            "keep" => Some(Token::Keep),
            "take" => Some(Token::Take),
            "freeze" => Some(Token::Freeze),
            "watch" => Some(Token::Watch),
            "download" => Some(Token::Download),
            "crawl" => Some(Token::Crawl),
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
#[allow(dead_code)]
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
