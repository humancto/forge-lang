mod lexer;
pub mod token;

#[allow(unused_imports)] // `Comment` is part of the library API
pub use lexer::{Comment, LexError, Lexer};
