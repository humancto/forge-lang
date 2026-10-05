pub mod ast;
pub mod index;
mod parser;

pub use parser::{ParseError, Parser};

/// Parse a standalone type annotation (`[Int]`, `fn(String) -> Bool`,
/// `Result<Int, String>`, ...). The whole input must be one type.
pub fn parse_type_annotation(src: &str) -> Result<ast::TypeAnn, ParseError> {
    let tokens = crate::lexer::Lexer::new(src)
        .tokenize()
        .map_err(|e| ParseError {
            message: e.message,
            line: e.line,
            col: e.col,
        })?;
    Parser::new(tokens).parse_standalone_type()
}
