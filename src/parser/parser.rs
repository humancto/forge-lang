use super::ast::*;
/// Forge Parser — Recursive Descent
/// Converts a token stream into an AST.
/// Expression parsing uses Pratt parsing for correct precedence.
use crate::lexer::token::{Spanned, Token};
use crate::lexer::Lexer;

pub struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Spanned>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut statements = Vec::new();
        self.skip_newlines();

        while !self.is_at_end() {
            let stmt = self.parse_statement()?;
            statements.push(stmt);
            self.skip_newlines();
        }

        Ok(Program { statements })
    }

    // ========== Statement Parsing ==========

    fn parse_statement(&mut self) -> Result<Stmt, ParseError> {
        self.skip_newlines();

        match self.current_token() {
            Token::Let => self.parse_let(),
            Token::Set => self.parse_set(),
            Token::Change => self.parse_change(),
            Token::Fn | Token::Define => self.parse_fn_def(Vec::new()),
            Token::Type => self.parse_type_def(),
            Token::Interface => self.parse_interface_def(),
            Token::Struct => self.parse_struct_def(),
            Token::Return => self.parse_return(),
            Token::If => self.parse_if(),
            Token::Match => self.parse_match(),
            Token::For => self.parse_for(),
            Token::While => self.parse_while(),
            Token::Loop => self.parse_loop(),
            Token::Repeat => self.parse_repeat(),
            Token::Break => {
                self.advance();
                Ok(Stmt::Break)
            }
            Token::Continue => {
                self.advance();
                Ok(Stmt::Continue)
            }
            Token::Spawn => self.parse_spawn(),
            Token::At => self.parse_decorator_or_fn(),
            Token::Say | Token::Yell | Token::Whisper => self.parse_say_yell_whisper(),
            Token::Grab => self.parse_grab(),
            Token::Wait => self.parse_wait(),
            Token::TryKw => self.parse_try_catch(),
            Token::Import => self.parse_import(),
            Token::Async | Token::ForgeKw => self.parse_fn_def(Vec::new()),
            Token::Yield | Token::Emit => self.parse_yield(),
            Token::Unpack => self.parse_unpack(),
            Token::When => self.parse_when(),
            Token::Check => self.parse_check(),
            Token::Safe => self.parse_safe_block(),
            Token::Timeout => self.parse_timeout(),
            Token::Retry => self.parse_retry(),
            Token::Schedule => self.parse_schedule(),
            Token::Watch => self.parse_watch(),
            Token::Prompt => self.parse_prompt_def(),
            Token::Download => self.parse_download(),
            Token::Crawl => self.parse_crawl(),
            _ => self.parse_expr_or_assign(),
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Let)?;

        let mutable = if self.check(&Token::Mut) {
            self.advance();
            true
        } else {
            false
        };

        let name = self.expect_ident()?;

        let type_ann = if self.check(&Token::Colon) {
            self.advance();
            Some(self.parse_type_ann()?)
        } else {
            None
        };

        self.expect(Token::Eq)?;
        let value = self.parse_expr()?;

        Ok(Stmt::Let {
            name,
            mutable,
            type_ann,
            value,
        })
    }

    /// Parses: set [mut] name to expr
    fn parse_set(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Set)?;

        let mutable = if self.check(&Token::Mut) {
            self.advance();
            true
        } else {
            false
        };

        let name = self.expect_ident()?;
        self.expect(Token::To)?;
        let value = self.parse_expr()?;

        Ok(Stmt::Let {
            name,
            mutable,
            type_ann: None,
            value,
        })
    }

    /// Parses: change name to expr
    fn parse_change(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Change)?;
        let target = self.parse_expr()?;
        self.expect(Token::To)?;
        let value = self.parse_expr()?;

        Ok(Stmt::Assign { target, value })
    }

    /// Parses: type Name = Variant(fields) | Variant(fields) | ...
    fn parse_type_def(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Type)?;
        let name = self.expect_ident()?;
        self.expect(Token::Eq)?;
        self.skip_newlines();

        let mut variants = Vec::new();
        loop {
            let variant_name = self.expect_ident()?;
            let fields = if self.check(&Token::LParen) {
                self.advance();
                let mut fields = Vec::new();
                while !self.check(&Token::RParen) {
                    fields.push(self.parse_type_ann()?);
                    if self.check(&Token::Comma) {
                        self.advance();
                    }
                }
                self.expect(Token::RParen)?;
                fields
            } else {
                Vec::new()
            };
            variants.push(Variant {
                name: variant_name,
                fields,
            });
            self.skip_newlines();
            // | separates variants -- we use Ident("|") won't work, need to check for Pipe-like token
            // The lexer produces an error for bare `|`, so we check for `||` split or just use Ident
            // Actually the `|` char in `|>` is handled as Pipe. For ADT we need bare `|`.
            // Let's check if next char after skip is an ident (next variant) preceded by nothing,
            // or if there's no more variants
            if self.check_pipe_separator() {
                self.advance(); // skip the `|` separator
                self.skip_newlines();
            } else {
                break;
            }
        }

        Ok(Stmt::TypeDef { name, variants })
    }

    /// Parses: interface Name { fn method(params) -> Type, ... }
    fn parse_interface_def(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Interface)?;
        let name = self.expect_ident()?;
        self.expect(Token::LBrace)?;
        self.skip_newlines();

        let mut methods = Vec::new();
        while !self.check(&Token::RBrace) {
            if self.check(&Token::Fn) || self.check(&Token::Define) {
                self.advance();
            }
            let method_name = self.expect_ident()?;
            self.expect(Token::LParen)?;
            let params = self.parse_params()?;
            self.expect(Token::RParen)?;
            let return_type = if self.check(&Token::Arrow) {
                self.advance();
                Some(self.parse_type_ann()?)
            } else {
                None
            };
            methods.push(MethodSig {
                name: method_name,
                params,
                return_type,
            });
            self.skip_newlines();
        }
        self.expect(Token::RBrace)?;

        Ok(Stmt::InterfaceDef { name, methods })
    }

    /// Parses: say/yell/whisper expr
    fn parse_say_yell_whisper(&mut self) -> Result<Stmt, ParseError> {
        let builtin_name = match self.current_token() {
            Token::Say => "say",
            Token::Yell => "yell",
            Token::Whisper => "whisper",
            _ => unreachable!(),
        };
        self.advance();
        let arg = self.parse_expr()?;

        Ok(Stmt::Expression(Expr::Call {
            function: Box::new(Expr::Ident(builtin_name.to_string())),
            args: vec![arg],
        }))
    }

    /// Parses: grab name from expr [with expr]
    fn parse_grab(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Grab)?;
        let name = self.expect_ident()?;
        self.expect(Token::From)?;
        let url_expr = self.parse_expr()?;

        let fetch_call = if self.check(&Token::Ident(String::new())) {
            if let Token::Ident(ref kw) = self.current_token() {
                if kw == "with" {
                    self.advance();
                    let opts = self.parse_expr()?;
                    Expr::Call {
                        function: Box::new(Expr::Ident("fetch".to_string())),
                        args: vec![url_expr, opts],
                    }
                } else {
                    Expr::Call {
                        function: Box::new(Expr::Ident("fetch".to_string())),
                        args: vec![url_expr],
                    }
                }
            } else {
                Expr::Call {
                    function: Box::new(Expr::Ident("fetch".to_string())),
                    args: vec![url_expr],
                }
            }
        } else {
            Expr::Call {
                function: Box::new(Expr::Ident("fetch".to_string())),
                args: vec![url_expr],
            }
        };

        Ok(Stmt::Let {
            name,
            mutable: false,
            type_ann: None,
            value: fetch_call,
        })
    }

    /// Parses: wait expr seconds
    fn parse_wait(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Wait)?;
        let duration = self.parse_expr()?;
        if self.check(&Token::Seconds) {
            self.advance();
        }

        Ok(Stmt::Expression(Expr::Call {
            function: Box::new(Expr::Ident("wait".to_string())),
            args: vec![duration],
        }))
    }

    /// Parses: try { body } catch name { handler }
    fn parse_try_catch(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::TryKw)?;
        let try_body = self.parse_block()?;
        self.skip_newlines();
        self.expect(Token::Catch)?;
        let catch_var = self.expect_ident()?;
        let catch_body = self.parse_block()?;
        Ok(Stmt::TryCatch {
            try_body,
            catch_var,
            catch_body,
        })
    }

    /// Parses: import "path" / import { name, name } from "path"
    fn parse_import(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Import)?;

        if self.check(&Token::LBrace) {
            // import { name1, name2 } from "path"
            self.advance();
            let mut names = Vec::new();
            while !self.check(&Token::RBrace) {
                names.push(self.expect_ident()?);
                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(Token::RBrace)?;
            self.expect(Token::From)?;
            let path = match self.current_token() {
                Token::StringLit(s) => {
                    self.advance();
                    s
                }
                _ => return Err(self.error("expected string path after 'from'")),
            };
            Ok(Stmt::Import {
                path,
                names: Some(names),
            })
        } else {
            // import "path" or import name
            let path = match self.current_token() {
                Token::StringLit(s) => {
                    self.advance();
                    s
                }
                Token::Ident(s) => {
                    self.advance();
                    s
                }
                _ => {
                    return Err(self.error(
                        "expected module name or string path after 'import'. Examples: import \"utils.fg\" or import math",
                    ));
                }
            };
            Ok(Stmt::Import { path, names: None })
        }
    }

    fn parse_yield(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // skip yield or emit
        let expr = self.parse_expr()?;
        Ok(Stmt::YieldStmt(expr))
    }

    /// Parses: unpack { a, b } from expr  /  unpack [ a, ...rest ] from expr
    fn parse_unpack(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Unpack)?;

        let pattern = if self.check(&Token::LBrace) {
            self.advance();
            let mut names = Vec::new();
            while !self.check(&Token::RBrace) {
                names.push(self.expect_ident()?);
                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(Token::RBrace)?;
            DestructurePattern::Object(names)
        } else if self.check(&Token::LBracket) {
            self.advance();
            let mut items = Vec::new();
            let mut rest = None;
            while !self.check(&Token::RBracket) {
                if self.check(&Token::DotDotDot) {
                    self.advance();
                    rest = Some(self.expect_ident()?);
                } else {
                    items.push(self.expect_ident()?);
                }
                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(Token::RBracket)?;
            DestructurePattern::Array { items, rest }
        } else {
            return Err(self.error("expected { or [ after 'unpack'"));
        };

        self.expect(Token::From)?;
        let value = self.parse_expr()?;

        Ok(Stmt::Destructure { pattern, value })
    }

    fn parse_when(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::When)?;
        let subject = self.parse_expr()?;
        self.expect(Token::LBrace)?;
        self.skip_newlines();
        let mut arms = Vec::new();
        while !self.check(&Token::RBrace) {
            if self.check(&Token::Else) || self.check(&Token::Otherwise) || self.check(&Token::Nah)
            {
                self.advance();
                if self.check(&Token::Arrow) || self.check(&Token::FatArrow) {
                    self.advance();
                }
                let result = self.parse_expr()?;
                arms.push(WhenArm {
                    op: None,
                    value: None,
                    result,
                    is_else: true,
                });
            } else {
                let op = match self.current_token() {
                    Token::Lt => {
                        self.advance();
                        Some(BinOp::Lt)
                    }
                    Token::Gt => {
                        self.advance();
                        Some(BinOp::Gt)
                    }
                    Token::LtEq => {
                        self.advance();
                        Some(BinOp::LtEq)
                    }
                    Token::GtEq => {
                        self.advance();
                        Some(BinOp::GtEq)
                    }
                    Token::EqEq => {
                        self.advance();
                        Some(BinOp::Eq)
                    }
                    Token::NotEq => {
                        self.advance();
                        Some(BinOp::NotEq)
                    }
                    _ => None,
                };
                let value = Some(self.parse_expr()?);
                if self.check(&Token::Arrow) || self.check(&Token::FatArrow) {
                    self.advance();
                }
                let result = self.parse_expr()?;
                arms.push(WhenArm {
                    op,
                    value,
                    result,
                    is_else: false,
                });
            }
            self.skip_newlines();
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }
        self.expect(Token::RBrace)?;
        Ok(Stmt::When { subject, arms })
    }

    fn parse_check(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Check)?;
        let expr = self.parse_expr()?;
        let check_kind = if self.check(&Token::Ident(String::new())) {
            match self.current_token() {
                Token::Ident(ref s) if s == "is" => {
                    self.advance();
                    if self.check(&Token::Not) {
                        self.advance();
                    } else if matches!(self.current_token(), Token::Ident(ref s) if s == "not") {
                        self.advance();
                    }
                    if let Token::Ident(ref w) = self.current_token() {
                        if w == "empty" {
                            self.advance();
                            CheckKind::IsNotEmpty
                        } else {
                            CheckKind::IsTrue
                        }
                    } else {
                        CheckKind::IsTrue
                    }
                }
                Token::Ident(ref s) if s == "between" => {
                    self.advance();
                    let lo = self.parse_expr()?;
                    if let Token::Ident(ref w) = self.current_token() {
                        if w == "and" {
                            self.advance();
                        }
                    }
                    let hi = self.parse_expr()?;
                    CheckKind::Between(lo, hi)
                }
                _ => CheckKind::IsTrue,
            }
        } else if self.check(&Token::Ident(String::new())) {
            CheckKind::IsTrue
        } else {
            CheckKind::IsTrue
        };
        Ok(Stmt::CheckStmt { expr, check_kind })
    }

    fn parse_safe_block(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Safe)?;
        let body = self.parse_block()?;
        Ok(Stmt::SafeBlock { body })
    }

    fn parse_timeout(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Timeout)?;
        let duration = self.parse_expr()?;
        if self.check(&Token::Seconds) {
            self.advance();
        }
        let body = self.parse_block()?;
        Ok(Stmt::TimeoutBlock { duration, body })
    }

    fn parse_retry(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Retry)?;
        let count = self.parse_expr()?;
        if self.check(&Token::Times) {
            self.advance();
        }
        let body = self.parse_block()?;
        Ok(Stmt::RetryBlock { count, body })
    }

    fn parse_schedule(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Schedule)?;
        if self.check(&Token::Every) {
            self.advance();
        }
        let interval = self.parse_expr()?;
        let unit = match self.current_token() {
            Token::Seconds => {
                self.advance();
                "seconds".to_string()
            }
            Token::Ident(ref s) if s == "minutes" => {
                self.advance();
                "minutes".to_string()
            }
            Token::Ident(ref s) if s == "hours" => {
                self.advance();
                "hours".to_string()
            }
            _ => "seconds".to_string(),
        };
        let body = self.parse_block()?;
        Ok(Stmt::ScheduleBlock {
            interval,
            unit,
            body,
        })
    }

    fn parse_watch(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Watch)?;
        let path = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::WatchBlock { path, body })
    }

    /// Parses: download url to filepath
    fn parse_download(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Download)?;
        let url = self.parse_expr()?;
        let dest = if self.check(&Token::To) {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        // Desugar to: let _dl = http.download(url, dest)
        let mut call_args = vec![url];
        if let Some(d) = dest {
            call_args.push(d);
        }
        Ok(Stmt::Expression(Expr::Call {
            function: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("http".to_string())),
                field: "download".to_string(),
            }),
            args: call_args,
        }))
    }

    /// Parses: crawl url -> let result = http.crawl(url)
    fn parse_crawl(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Crawl)?;
        let url = self.parse_expr()?;
        Ok(Stmt::Expression(Expr::Call {
            function: Box::new(Expr::FieldAccess {
                object: Box::new(Expr::Ident("http".to_string())),
                field: "crawl".to_string(),
            }),
            args: vec![url],
        }))
    }

    fn parse_prompt_def(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Prompt)?;
        let name = self.expect_ident()?;
        self.expect(Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(Token::RParen)?;
        self.expect(Token::LBrace)?;
        self.skip_newlines();
        let mut system = String::new();
        let mut user_template = String::new();
        let mut returns = None;
        while !self.check(&Token::RBrace) {
            if let Token::Ident(ref key) = self.current_token() {
                let key = key.clone();
                self.advance();
                self.expect(Token::Colon)?;
                if let Token::StringLit(ref s) | Token::RawStringLit(ref s) = self.current_token() {
                    let val = s.clone();
                    self.advance();
                    match key.as_str() {
                        "system" => system = val,
                        "user" => user_template = val,
                        "returns" => returns = Some(val),
                        _ => {}
                    }
                }
            }
            self.skip_newlines();
        }
        self.expect(Token::RBrace)?;
        Ok(Stmt::PromptDef {
            name,
            params,
            system,
            user_template,
            returns,
        })
    }

    /// Parses: repeat expr times { body }
    fn parse_repeat(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Repeat)?;
        let count = self.parse_expr()?;
        if self.check(&Token::Times) {
            self.advance();
        }
        let body = self.parse_block()?;

        Ok(Stmt::For {
            var: "_".to_string(),
            var2: None,
            iterable: Expr::Call {
                function: Box::new(Expr::Ident("range".to_string())),
                args: vec![count],
            },
            body,
        })
    }

    fn parse_fn_def(&mut self, decorators: Vec<Decorator>) -> Result<Stmt, ParseError> {
        let is_async = if self.check(&Token::Async) || self.check(&Token::ForgeKw) {
            self.advance();
            true
        } else {
            false
        };

        // After forge/async, fn/define is optional
        if self.check(&Token::Define) {
            self.advance();
        } else if self.check(&Token::Fn) {
            self.advance();
        } else if !is_async {
            return Err(self.error("expected 'fn' or 'define'"));
        }
        let name = self.expect_ident()?;

        self.expect(Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(Token::RParen)?;

        let return_type = if self.check(&Token::Arrow) {
            self.advance();
            Some(self.parse_type_ann()?)
        } else {
            None
        };

        let body = self.parse_block()?;

        Ok(Stmt::FnDef {
            name,
            params,
            return_type,
            body,
            decorators,
            is_async,
        })
    }

    fn parse_struct_def(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Struct)?;
        let name = self.expect_ident()?;

        self.expect(Token::LBrace)?;
        self.skip_newlines();

        let mut fields = Vec::new();
        while !self.check(&Token::RBrace) {
            let field_name = self.expect_ident()?;
            self.expect(Token::Colon)?;
            let type_ann = self.parse_type_ann()?;
            fields.push(FieldDef {
                name: field_name,
                type_ann,
            });
            self.skip_newlines();
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }

        self.expect(Token::RBrace)?;
        Ok(Stmt::StructDef { name, fields })
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Return)?;

        // Check if there's an expression after return
        if self.is_at_end() || self.check(&Token::RBrace) || self.check(&Token::Newline) {
            return Ok(Stmt::Return(None));
        }

        let expr = self.parse_expr()?;
        Ok(Stmt::Return(Some(expr)))
    }

    fn check_else_keyword(&self) -> bool {
        self.check(&Token::Else) || self.check(&Token::Otherwise) || self.check(&Token::Nah)
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::If)?;
        let condition = self.parse_expr()?;
        let then_body = self.parse_block()?;

        self.skip_newlines();
        let else_body = if self.check_else_keyword() {
            self.advance();
            if self.check(&Token::If) {
                let elif = self.parse_if()?;
                Some(vec![elif])
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };

        Ok(Stmt::If {
            condition,
            then_body,
            else_body,
        })
    }

    fn parse_match(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Match)?;
        let subject = self.parse_expr()?;

        self.expect(Token::LBrace)?;
        self.skip_newlines();

        let mut arms = Vec::new();
        while !self.check(&Token::RBrace) {
            let pattern = self.parse_pattern()?;
            self.expect(Token::FatArrow)?;

            let body = if self.check(&Token::LBrace) {
                self.parse_block()?
            } else {
                let stmt = self.parse_statement()?;
                vec![stmt]
            };

            arms.push(MatchArm { pattern, body });
            self.skip_newlines();
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }

        self.expect(Token::RBrace)?;
        Ok(Stmt::Match { subject, arms })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        match self.current_token() {
            Token::Ident(ref name) if name == "_" => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            Token::Ident(ref name) => {
                let name = name.clone();
                self.advance();

                // Check for constructor pattern: Name(fields...)
                if self.check(&Token::LParen) {
                    self.advance();
                    let mut fields = Vec::new();
                    while !self.check(&Token::RParen) {
                        fields.push(self.parse_pattern()?);
                        if self.check(&Token::Comma) {
                            self.advance();
                        }
                    }
                    self.expect(Token::RParen)?;
                    Ok(Pattern::Constructor { name, fields })
                } else {
                    Ok(Pattern::Binding(name))
                }
            }
            Token::Int(n) => {
                let n = n;
                self.advance();
                Ok(Pattern::Literal(Expr::Int(n)))
            }
            Token::Float(n) => {
                let n = n;
                self.advance();
                Ok(Pattern::Literal(Expr::Float(n)))
            }
            Token::StringLit(ref s) => {
                let s = s.clone();
                self.advance();
                Ok(Pattern::Literal(Expr::StringLit(s)))
            }
            Token::True => {
                self.advance();
                Ok(Pattern::Literal(Expr::Bool(true)))
            }
            Token::False => {
                self.advance();
                Ok(Pattern::Literal(Expr::Bool(false)))
            }
            _ => Err(self.error("expected pattern")),
        }
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::For)?;
        if self.check(&Token::Each) {
            self.advance();
        }
        let var = self.expect_ident()?;
        // Check for key, value syntax
        let var2 = if self.check(&Token::Comma) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };
        self.expect(Token::In)?;
        let iterable = self.parse_expr()?;
        let body = self.parse_block()?;

        Ok(Stmt::For {
            var,
            var2,
            iterable,
            body,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::While)?;
        let condition = self.parse_expr()?;
        let body = self.parse_block()?;

        Ok(Stmt::While { condition, body })
    }

    fn parse_loop(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Loop)?;
        let body = self.parse_block()?;
        Ok(Stmt::Loop { body })
    }

    fn parse_spawn(&mut self) -> Result<Stmt, ParseError> {
        self.expect(Token::Spawn)?;
        let body = self.parse_block()?;
        Ok(Stmt::Spawn { body })
    }

    fn parse_decorator_or_fn(&mut self) -> Result<Stmt, ParseError> {
        let decorator = self.parse_decorator()?;
        self.skip_newlines();

        // @server is always a standalone config decorator
        if decorator.name == "server" {
            return Ok(Stmt::DecoratorStmt(decorator));
        }

        if self.check(&Token::Fn) || self.check(&Token::Define) {
            self.parse_fn_def(vec![decorator])
        } else if self.check(&Token::At) {
            let mut decorators = vec![decorator];
            while self.check(&Token::At) {
                decorators.push(self.parse_decorator()?);
                self.skip_newlines();
            }
            if self.check(&Token::Fn) || self.check(&Token::Define) {
                self.parse_fn_def(decorators)
            } else {
                Ok(Stmt::DecoratorStmt(decorators.pop().unwrap()))
            }
        } else {
            // Standalone decorator
            Ok(Stmt::DecoratorStmt(decorator))
        }
    }

    fn parse_decorator(&mut self) -> Result<Decorator, ParseError> {
        self.expect(Token::At)?;
        let name = self.expect_ident()?;

        let args = if self.check(&Token::LParen) {
            self.advance();
            let mut args = Vec::new();
            while !self.check(&Token::RParen) {
                // Try named arg: key: value
                if let Token::Ident(ref key) = self.current_token() {
                    let key = key.clone();
                    let saved_pos = self.pos;
                    self.advance();
                    if self.check(&Token::Colon) {
                        self.advance();
                        let value = self.parse_expr()?;
                        args.push(DecoratorArg::Named(key, value));
                    } else {
                        // Not a named arg, backtrack
                        self.pos = saved_pos;
                        let expr = self.parse_expr()?;
                        args.push(DecoratorArg::Positional(expr));
                    }
                } else {
                    let expr = self.parse_expr()?;
                    args.push(DecoratorArg::Positional(expr));
                }

                if self.check(&Token::Comma) {
                    self.advance();
                }
            }
            self.expect(Token::RParen)?;
            args
        } else {
            Vec::new()
        };

        Ok(Decorator { name, args })
    }

    fn parse_expr_or_assign(&mut self) -> Result<Stmt, ParseError> {
        let expr = self.parse_expr()?;

        if self.check(&Token::Eq) {
            self.advance();
            let value = self.parse_expr()?;
            Ok(Stmt::Assign {
                target: expr,
                value,
            })
        } else if self.check(&Token::PlusEq)
            || self.check(&Token::MinusEq)
            || self.check(&Token::StarEq)
            || self.check(&Token::SlashEq)
        {
            let op = match self.current_token() {
                Token::PlusEq => BinOp::Add,
                Token::MinusEq => BinOp::Sub,
                Token::StarEq => BinOp::Mul,
                Token::SlashEq => BinOp::Div,
                _ => unreachable!(),
            };
            self.advance();
            let rhs = self.parse_expr()?;
            Ok(Stmt::Assign {
                target: expr.clone(),
                value: Expr::BinOp {
                    left: Box::new(expr),
                    op,
                    right: Box::new(rhs),
                },
            })
        } else {
            Ok(Stmt::Expression(expr))
        }
    }

    // ========== Expression Parsing (Pratt) ==========

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_pipeline()
    }

    /// Pipeline: expr |> expr |> expr
    fn parse_pipeline(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_or()?;

        while self.check(&Token::Pipe) {
            self.advance();
            let func = self.parse_or()?;
            expr = Expr::Pipeline {
                value: Box::new(expr),
                function: Box::new(func),
            };
        }

        Ok(expr)
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;
        while self.check(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_equality()?;
        while self.check(&Token::And) {
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.current_token() {
                Token::EqEq => BinOp::Eq,
                Token::NotEq => BinOp::NotEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_addition()?;
        loop {
            let op = match self.current_token() {
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::LtEq => BinOp::LtEq,
                Token::GtEq => BinOp::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_addition()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_addition(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.current_token() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplication()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.current_token() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.current_token() {
            Token::Await | Token::Hold => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Await(Box::new(operand)))
            }
            Token::Must => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Must(Box::new(operand)))
            }
            Token::Freeze => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Freeze(Box::new(operand)))
            }
            Token::Ask => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Ask(Box::new(operand)))
            }
            Token::DotDotDot => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Spread(Box::new(operand)))
            }
            Token::Minus => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                })
            }
            Token::Not => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    /// Postfix: calls, field access, indexing, try (?)
    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.current_token() {
                Token::LParen => {
                    self.advance();
                    let args = self.parse_call_args()?;
                    self.expect(Token::RParen)?;
                    expr = Expr::Call {
                        function: Box::new(expr),
                        args,
                    };
                }
                Token::Dot => {
                    self.advance();
                    let field = self.expect_ident()?;
                    expr = Expr::FieldAccess {
                        object: Box::new(expr),
                        field,
                    };
                }
                Token::LBracket => {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(Token::RBracket)?;
                    expr = Expr::Index {
                        object: Box::new(expr),
                        index: Box::new(index),
                    };
                }
                Token::Question => {
                    self.advance();
                    expr = Expr::Try(Box::new(expr));
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.current_token() {
            Token::Int(n) => {
                let n = n;
                self.advance();
                Ok(Expr::Int(n))
            }
            Token::Float(n) => {
                let n = n;
                self.advance();
                Ok(Expr::Float(n))
            }
            Token::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            Token::NullLit => {
                self.advance();
                Ok(Expr::Ident("null".to_string()))
            }

            Token::Spawn => {
                self.advance();
                let body = self.parse_block()?;
                Ok(Expr::Spawn(body))
            }

            Token::StringLit(ref s) => {
                let s = s.clone();
                self.advance();
                if s.contains('{') && s.contains('}') {
                    self.parse_string_interpolation(&s)
                } else {
                    Ok(Expr::StringLit(s))
                }
            }

            Token::RawStringLit(ref s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::StringLit(s))
            }

            Token::Ident(ref name) => {
                let name = name.clone();
                self.advance();

                // Check for struct init: Name { field: value }
                if name.chars().next().is_some_and(|c| c.is_uppercase())
                    && self.check(&Token::LBrace)
                {
                    self.advance();
                    self.skip_newlines();
                    let mut fields = Vec::new();
                    while !self.check(&Token::RBrace) {
                        let field_name = self.expect_ident()?;
                        self.expect(Token::Colon)?;
                        let value = self.parse_expr()?;
                        fields.push((field_name, value));
                        self.skip_newlines();
                        if self.check(&Token::Comma) {
                            self.advance();
                        }
                        self.skip_newlines();
                    }
                    self.expect(Token::RBrace)?;
                    Ok(Expr::StructInit { name, fields })
                } else {
                    Ok(Expr::Ident(name))
                }
            }

            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(expr)
            }

            Token::LBrace => {
                // Object literal or block — disambiguate
                self.parse_object_or_block()
            }

            Token::LBracket => {
                // Array literal
                self.advance();
                let mut elements = Vec::new();
                self.skip_newlines();
                while !self.check(&Token::RBracket) {
                    elements.push(self.parse_expr()?);
                    self.skip_newlines();
                    if self.check(&Token::Comma) {
                        self.advance();
                    }
                    self.skip_newlines();
                }
                self.expect(Token::RBracket)?;
                Ok(Expr::Array(elements))
            }

            Token::If => {
                // if-expression: if cond { expr } else { expr }
                self.advance();
                let cond = self.parse_expr()?;
                let then_body = self.parse_block()?;
                self.skip_newlines();
                let else_body = if self.check_else_keyword() {
                    self.advance();
                    Some(self.parse_block()?)
                } else {
                    None
                };
                // Wrap as block expression
                Ok(Expr::Block(vec![Stmt::If {
                    condition: cond,
                    then_body,
                    else_body,
                }]))
            }

            Token::Type => {
                self.advance();
                Ok(Expr::Ident("type".to_string()))
            }

            Token::Fn => {
                self.advance();
                self.expect(Token::LParen)?;
                let params = self.parse_params()?;
                self.expect(Token::RParen)?;
                let body = self.parse_block()?;
                Ok(Expr::Lambda { params, body })
            }

            Token::When => {
                let when_stmt = self.parse_when()?;
                Ok(Expr::Block(vec![when_stmt]))
            }

            Token::Safe => {
                let safe_stmt = self.parse_safe_block()?;
                Ok(Expr::Block(vec![safe_stmt]))
            }

            _ => Err(self.error(&format!("unexpected token: {:?}", self.current_token()))),
        }
    }

    fn parse_string_interpolation(&self, s: &str) -> Result<Expr, ParseError> {
        let mut parts = Vec::new();
        let mut chars = s.chars().peekable();
        let mut current = String::new();

        while let Some(ch) = chars.next() {
            if ch == '{' {
                if !current.is_empty() {
                    parts.push(StringPart::Literal(std::mem::take(&mut current)));
                }
                let mut expr_str = String::new();
                let mut depth = 1;
                for inner in chars.by_ref() {
                    if inner == '{' {
                        depth += 1;
                    }
                    if inner == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    expr_str.push(inner);
                }
                if depth != 0 {
                    return Err(self.error("unterminated interpolation expression"));
                }

                let expr_str = expr_str.trim();
                if expr_str.is_empty() {
                    return Err(self.error("empty interpolation expression"));
                }

                let parsed = self.parse_interpolation_expr(expr_str)?;
                parts.push(StringPart::Expr(parsed));
            } else if ch == '}' {
                return Err(self.error("unexpected '}' in string literal"));
            } else {
                current.push(ch);
            }
        }

        if !current.is_empty() {
            parts.push(StringPart::Literal(current));
        }

        if parts.len() == 1 {
            if let StringPart::Literal(s) = &parts[0] {
                return Ok(Expr::StringLit(s.clone()));
            }
        }

        Ok(Expr::StringInterp(parts))
    }

    fn parse_interpolation_expr(&self, expr_source: &str) -> Result<Expr, ParseError> {
        let mut lexer = Lexer::new(expr_source);
        let tokens = lexer.tokenize().map_err(|e| {
            self.error(&format!(
                "invalid interpolation expression '{{{}}}': {}",
                expr_source, e.message
            ))
        })?;

        let mut parser = Parser::new(tokens);
        let expr = parser.parse_expr().map_err(|e| {
            self.error(&format!(
                "invalid interpolation expression '{{{}}}': {}",
                expr_source, e.message
            ))
        })?;
        parser.skip_newlines();
        if !parser.is_at_end() {
            return Err(self.error(&format!(
                "invalid interpolation expression '{{{}}}': trailing tokens",
                expr_source
            )));
        }

        Ok(expr)
    }

    fn parse_object_or_block(&mut self) -> Result<Expr, ParseError> {
        self.expect(Token::LBrace)?;
        self.skip_newlines();

        // Empty braces = empty object
        if self.check(&Token::RBrace) {
            self.advance();
            return Ok(Expr::Object(Vec::new()));
        }

        // Peek ahead: if we see `ident:` or `"string":` it's an object literal
        if matches!(self.current_token(), Token::Ident(_) | Token::StringLit(_)) {
            let saved = self.pos;
            self.advance();
            if self.check(&Token::Colon) {
                self.pos = saved;
                return self.parse_object_fields();
            }
            self.pos = saved;
        }

        // Otherwise it's a block
        let mut stmts = Vec::new();
        while !self.check(&Token::RBrace) {
            stmts.push(self.parse_statement()?);
            self.skip_newlines();
        }
        self.expect(Token::RBrace)?;
        Ok(Expr::Block(stmts))
    }

    fn parse_object_fields(&mut self) -> Result<Expr, ParseError> {
        let mut fields = Vec::new();

        while !self.check(&Token::RBrace) {
            let key = match self.current_token() {
                Token::StringLit(s) => {
                    let s = s;
                    self.advance();
                    s
                }
                _ => self.expect_ident()?,
            };
            self.expect(Token::Colon)?;
            let value = self.parse_expr()?;
            fields.push((key, value));
            self.skip_newlines();
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }

        self.expect(Token::RBrace)?;
        Ok(Expr::Object(fields))
    }

    // ========== Helpers ==========

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.skip_newlines();
        self.expect(Token::LBrace)?;
        self.skip_newlines();

        let mut stmts = Vec::new();
        while !self.check(&Token::RBrace) {
            stmts.push(self.parse_statement()?);
            self.skip_newlines();
        }

        self.expect(Token::RBrace)?;
        Ok(stmts)
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        while !self.check(&Token::RParen) {
            let name = self.expect_ident()?;

            let type_ann = if self.check(&Token::Colon) {
                self.advance();
                Some(self.parse_type_ann()?)
            } else {
                None
            };

            let default = if self.check(&Token::Eq) {
                self.advance();
                Some(self.parse_expr()?)
            } else {
                None
            };

            params.push(Param {
                name,
                type_ann,
                default,
            });

            if self.check(&Token::Comma) {
                self.advance();
            }
        }
        Ok(params)
    }

    fn parse_call_args(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        self.skip_newlines();
        while !self.check(&Token::RParen) {
            args.push(self.parse_expr()?);
            self.skip_newlines();
            if self.check(&Token::Comma) {
                self.advance();
            }
            self.skip_newlines();
        }
        Ok(args)
    }

    fn parse_type_ann(&mut self) -> Result<TypeAnn, ParseError> {
        match self.current_token() {
            Token::LBracket => {
                self.advance();
                let inner = self.parse_type_ann()?;
                self.expect(Token::RBracket)?;
                Ok(TypeAnn::Array(Box::new(inner)))
            }
            Token::Question => {
                self.advance();
                let inner = self.parse_type_ann()?;
                Ok(TypeAnn::Optional(Box::new(inner)))
            }
            Token::Ident(ref name) if matches!(self.current_token(), Token::Ident(_)) => {
                let name = name.clone();
                self.advance();
                if self.check(&Token::Lt) {
                    self.advance();
                    let mut type_args = Vec::new();
                    while !self.check(&Token::Gt) {
                        type_args.push(self.parse_type_ann()?);
                        if self.check(&Token::Comma) {
                            self.advance();
                        }
                    }
                    self.expect(Token::Gt)?;
                    Ok(TypeAnn::Generic(name, type_args))
                } else {
                    Ok(TypeAnn::Simple(name))
                }
            }
            Token::IntType => {
                self.advance();
                Ok(TypeAnn::Simple("Int".into()))
            }
            Token::FloatType => {
                self.advance();
                Ok(TypeAnn::Simple("Float".into()))
            }
            Token::StringType => {
                self.advance();
                Ok(TypeAnn::Simple("String".into()))
            }
            Token::BoolType => {
                self.advance();
                Ok(TypeAnn::Simple("Bool".into()))
            }
            Token::JsonType => {
                self.advance();
                Ok(TypeAnn::Simple("Json".into()))
            }
            _ => Err(self.error(&format!("expected type, got {:?}", self.current_token()))),
        }
    }

    // ========== Token Navigation ==========

    fn current_token(&self) -> Token {
        self.tokens
            .get(self.pos)
            .map(|s| s.token.clone())
            .unwrap_or(Token::Eof)
    }

    fn check(&self, expected: &Token) -> bool {
        std::mem::discriminant(&self.current_token()) == std::mem::discriminant(expected)
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), ParseError> {
        if self.check(&expected) {
            self.advance();
            Ok(())
        } else {
            Err(self.error(&format!(
                "expected {:?}, got {:?}",
                expected,
                self.current_token()
            )))
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        let name = match self.current_token() {
            Token::Ident(name) => Some(name),
            // Allow keywords as field/method names
            Token::Set => Some("set".to_string()),
            Token::Type => Some("type".to_string()),
            Token::Match => Some("match".to_string()),
            Token::From => Some("from".to_string()),
            Token::To => Some("to".to_string()),
            Token::In => Some("in".to_string()),
            Token::Import => Some("import".to_string()),
            Token::Table => Some("table".to_string()),
            Token::Check => Some("check".to_string()),
            Token::Where => Some("where".to_string()),
            Token::Watch => Some("watch".to_string()),
            Token::Transform => Some("transform".to_string()),
            Token::Select => Some("select".to_string()),
            Token::Order => Some("order".to_string()),
            Token::Limit => Some("limit".to_string()),
            Token::Crawl => Some("crawl".to_string()),
            Token::Download => Some("download".to_string()),
            Token::Ask => Some("ask".to_string()),
            Token::Prompt => Some("prompt".to_string()),
            Token::Schedule => Some("schedule".to_string()),
            Token::Retry => Some("retry".to_string()),
            Token::Timeout => Some("timeout".to_string()),
            Token::Safe => Some("safe".to_string()),
            Token::Must => Some("must".to_string()),
            Token::Freeze => Some("freeze".to_string()),
            Token::Keep => Some("keep".to_string()),
            Token::Take => Some("take".to_string()),
            Token::Every => Some("every".to_string()),
            Token::Any => Some("any".to_string()),
            Token::By => Some("by".to_string()),
            _ => None,
        };
        match name {
            Some(n) => {
                self.advance();
                Ok(n)
            }
            None => Err(self.error(&format!(
                "expected identifier, got {:?}",
                self.current_token()
            ))),
        }
    }

    fn skip_newlines(&mut self) {
        while self.check(&Token::Newline) {
            self.advance();
        }
    }

    fn check_pipe_separator(&self) -> bool {
        self.check(&Token::Bar)
    }

    fn is_at_end(&self) -> bool {
        self.check(&Token::Eof)
    }

    fn error(&self, msg: &str) -> ParseError {
        let (line, col) = self
            .tokens
            .get(self.pos)
            .map(|s| (s.line, s.col))
            .unwrap_or((0, 0));
        ParseError {
            message: msg.to_string(),
            line,
            col,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}:{}] Parse error: {}",
            self.line, self.col, self.message
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_program(input: &str) -> Program {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize().expect("lexing should succeed");
        let mut parser = Parser::new(tokens);
        parser.parse_program().expect("parsing should succeed")
    }

    #[test]
    fn parses_expression_interpolation() {
        let program = parse_program(r#"let msg = "sum = {a + b}""#);
        let stmt = program.statements.first().expect("expected one statement");

        match stmt {
            Stmt::Let { value, .. } => match value {
                Expr::StringInterp(parts) => {
                    assert_eq!(parts.len(), 2);
                    match &parts[0] {
                        StringPart::Literal(s) => assert_eq!(s, "sum = "),
                        _ => panic!("first part should be literal"),
                    }
                    match &parts[1] {
                        StringPart::Expr(Expr::BinOp { op, .. }) => assert_eq!(*op, BinOp::Add),
                        other => panic!("expected binary expression, got {:?}", other),
                    }
                }
                other => panic!("expected interpolated string, got {:?}", other),
            },
            _ => panic!("expected let statement"),
        }
    }

    #[test]
    fn parses_field_access_interpolation() {
        let program = parse_program(r#"let msg = "name = {user.name}""#);
        let stmt = program.statements.first().expect("expected one statement");

        match stmt {
            Stmt::Let { value, .. } => match value {
                Expr::StringInterp(parts) => {
                    assert_eq!(parts.len(), 2);
                    match &parts[1] {
                        StringPart::Expr(Expr::FieldAccess { field, .. }) => {
                            assert_eq!(field, "name")
                        }
                        other => panic!("expected field access expression, got {:?}", other),
                    }
                }
                other => panic!("expected interpolated string, got {:?}", other),
            },
            _ => panic!("expected let statement"),
        }
    }

    #[test]
    fn rejects_invalid_interpolation_expression() {
        let mut lexer = Lexer::new(r#"let msg = "{a + }""#);
        let tokens = lexer.tokenize().expect("lexing should succeed");
        let mut parser = Parser::new(tokens);
        let err = parser.parse_program().expect_err("parsing should fail");
        assert!(err.message.contains("invalid interpolation expression"));
    }
}
