mod lexer;
pub mod token;

pub(crate) use lexer::{escape_source_len, literal_braces};
#[allow(unused_imports)] // `Comment` is part of the library API
pub use lexer::{Comment, LexError, Lexer};
