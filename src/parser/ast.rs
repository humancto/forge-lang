/// Forge Abstract Syntax Tree
/// Every valid Forge program is represented as a tree of these nodes.
/// A complete Forge program
#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

/// Statements — things that DO something
#[derive(Debug, Clone)]
pub enum Stmt {
    /// let name = expr  /  let name: Type = expr
    Let {
        name: String,
        mutable: bool,
        type_ann: Option<TypeAnn>,
        value: Expr,
    },

    /// name = expr  (reassignment)
    Assign {
        target: Expr,
        value: Expr,
    },

    /// fn name(params) -> ReturnType { body }
    FnDef {
        name: String,
        params: Vec<Param>,
        return_type: Option<TypeAnn>,
        body: Vec<Stmt>,
        decorators: Vec<Decorator>,
    },

    /// struct Name { field: Type, ... }
    StructDef {
        name: String,
        fields: Vec<FieldDef>,
    },

    /// return expr
    Return(Option<Expr>),

    /// if condition { body } else { body }
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },

    /// match expr { pattern => body, ... }
    Match {
        subject: Expr,
        arms: Vec<MatchArm>,
    },

    /// for item in iterable { body }
    For {
        var: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },

    /// while condition { body }
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },

    /// loop { body }
    Loop {
        body: Vec<Stmt>,
    },

    Break,
    Continue,

    /// spawn { body }
    Spawn {
        body: Vec<Stmt>,
    },

    /// @decorator(args...)
    /// Standalone decorator (e.g., @server(port: 8080))
    DecoratorStmt(Decorator),

    /// An expression used as a statement
    Expression(Expr),
}

/// Expressions — things that PRODUCE a value
#[derive(Debug, Clone)]
pub enum Expr {
    // === Literals ===
    Int(i64),
    Float(f64),
    StringLit(String),
    /// String with interpolation segments: "Hello, {name}!"
    StringInterp(Vec<StringPart>),
    Bool(bool),

    /// JSON-style object literal: { key: value, ... }
    Object(Vec<(String, Expr)>),
    /// Array literal: [1, 2, 3]
    Array(Vec<Expr>),

    // === References ===
    Ident(String),

    // === Operations ===
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
    },

    /// a.b  (field access)
    FieldAccess {
        object: Box<Expr>,
        field: String,
    },

    /// a[b]  (index)
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },

    /// f(args...)  (function call)
    Call {
        function: Box<Expr>,
        args: Vec<Expr>,
    },

    /// expr?  (error propagation)
    Try(Box<Expr>),

    /// |> pipeline: expr |> fn
    Pipeline {
        value: Box<Expr>,
        function: Box<Expr>,
    },

    /// fn(params) { body }  (closure / lambda)
    Lambda {
        params: Vec<Param>,
        body: Vec<Stmt>,
    },

    /// Struct construction: Name { field: value, ... }
    StructInit {
        name: String,
        fields: Vec<(String, Expr)>,
    },

    /// Block expression: { stmts; final_expr }
    Block(Vec<Stmt>),
}

/// Parts of an interpolated string
#[derive(Debug, Clone)]
pub enum StringPart {
    Literal(String),
    Expr(Expr),
}

/// Binary operators
#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,   // +
    Sub,   // -
    Mul,   // *
    Div,   // /
    Mod,   // %
    Eq,    // ==
    NotEq, // !=
    Lt,    // <
    Gt,    // >
    LtEq,  // <=
    GtEq,  // >=
    And,   // &&
    Or,    // ||
}

/// Unary operators
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg, // -x
    Not, // !x
}

/// Function parameter
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub type_ann: Option<TypeAnn>,
    pub default: Option<Expr>,
}

/// Struct field definition
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub type_ann: TypeAnn,
}

/// Type annotation
#[derive(Debug, Clone, PartialEq)]
pub enum TypeAnn {
    Simple(String),                       // Int, String, Bool, Json
    Array(Box<TypeAnn>),                  // [Int]
    Generic(String, Vec<TypeAnn>),        // Result<T, E>
    Function(Vec<TypeAnn>, Box<TypeAnn>), // (Int, Int) -> Int
    Optional(Box<TypeAnn>),               // ?Int  (sugar for Option<Int>)
}

/// Decorator: @name(args...)
#[derive(Debug, Clone)]
pub struct Decorator {
    pub name: String,
    pub args: Vec<DecoratorArg>,
}

/// Decorator argument (positional or named)
#[derive(Debug, Clone)]
pub enum DecoratorArg {
    Positional(Expr),
    Named(String, Expr),
}

/// Match arm: pattern => body
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<Stmt>,
}

/// Pattern for match expressions
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Wildcard: _
    Wildcard,
    /// Literal value
    Literal(Expr),
    /// Variable binding
    Binding(String),
    /// Constructor: Ok(value), Err(msg)
    Constructor { name: String, fields: Vec<Pattern> },
}
