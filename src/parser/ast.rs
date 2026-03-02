/// Forge Abstract Syntax Tree
/// Every valid Forge program is represented as a tree of these nodes.

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Stmt {
    Let {
        name: String,
        mutable: bool,
        type_ann: Option<TypeAnn>,
        value: Expr,
    },
    Assign {
        target: Expr,
        value: Expr,
    },
    FnDef {
        name: String,
        params: Vec<Param>,
        return_type: Option<TypeAnn>,
        body: Vec<Stmt>,
        decorators: Vec<Decorator>,
        is_async: bool,
    },
    Destructure {
        pattern: DestructurePattern,
        value: Expr,
    },
    StructDef {
        name: String,
        fields: Vec<FieldDef>,
    },
    Return(Option<Expr>),
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    Match {
        subject: Expr,
        arms: Vec<MatchArm>,
    },
    For {
        var: String,
        var2: Option<String>,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    Loop {
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    Spawn {
        body: Vec<Stmt>,
    },
    DecoratorStmt(Decorator),
    TypeDef {
        name: String,
        variants: Vec<Variant>,
    },
    InterfaceDef {
        name: String,
        methods: Vec<MethodSig>,
    },
    /// impl/give block: attach methods to a type
    ImplBlock {
        type_name: String,
        ability: Option<String>,
        methods: Vec<Stmt>,
    },
    TryCatch {
        try_body: Vec<Stmt>,
        catch_var: String,
        catch_body: Vec<Stmt>,
    },
    Import {
        path: String,
        names: Option<Vec<String>>,
    },
    YieldStmt(Expr),
    /// when subject { < val -> expr, else -> expr }
    When {
        subject: Expr,
        arms: Vec<WhenArm>,
    },
    /// check expr is/contains/between validation
    CheckStmt {
        expr: Expr,
        check_kind: CheckKind,
    },
    /// safe { body } -- null-safe execution
    SafeBlock {
        body: Vec<Stmt>,
    },
    /// timeout N seconds { body }
    TimeoutBlock {
        duration: Expr,
        body: Vec<Stmt>,
    },
    /// retry N times { body }
    RetryBlock {
        count: Expr,
        body: Vec<Stmt>,
    },
    /// schedule every N seconds/minutes { body }
    ScheduleBlock {
        interval: Expr,
        unit: String,
        body: Vec<Stmt>,
    },
    /// watch "path" { body }
    WatchBlock {
        path: Expr,
        body: Vec<Stmt>,
    },
    /// prompt name(params) { system/user/returns }
    PromptDef {
        name: String,
        params: Vec<Param>,
        system: String,
        user_template: String,
        returns: Option<String>,
    },
    /// agent name(params) { tools, goal, max_steps }
    AgentDef {
        name: String,
        params: Vec<Param>,
        tools: Vec<String>,
        goal: String,
        max_steps: usize,
    },
    Expression(Expr),
}

#[derive(Debug, Clone)]
pub struct WhenArm {
    pub op: Option<BinOp>,
    pub value: Option<Expr>,
    pub result: Expr,
    pub is_else: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CheckKind {
    IsNotEmpty,
    Contains(Expr),
    Between(Expr, Expr),
    IsTrue,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Expr {
    Int(i64),
    Float(f64),
    StringLit(String),
    StringInterp(Vec<StringPart>),
    Bool(bool),
    Object(Vec<(String, Expr)>),
    Array(Vec<Expr>),
    Ident(String),
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    UnaryOp {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    FieldAccess {
        object: Box<Expr>,
        field: String,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    Call {
        function: Box<Expr>,
        args: Vec<Expr>,
    },
    Try(Box<Expr>),
    Pipeline {
        value: Box<Expr>,
        function: Box<Expr>,
    },
    Lambda {
        params: Vec<Param>,
        body: Vec<Stmt>,
    },
    Await(Box<Expr>),
    Spawn(Vec<Stmt>),
    Spread(Box<Expr>),
    Must(Box<Expr>),
    Freeze(Box<Expr>),
    Ask(Box<Expr>),
    WhereFilter {
        source: Box<Expr>,
        field: String,
        op: BinOp,
        value: Box<Expr>,
    },
    PipeChain {
        source: Box<Expr>,
        steps: Vec<PipeStep>,
    },
    MethodCall {
        object: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
    StructInit {
        name: String,
        fields: Vec<(String, Expr)>,
    },
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone)]
pub enum StringPart {
    Literal(String),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Param {
    pub name: String,
    pub type_ann: Option<TypeAnn>,
    pub default: Option<Expr>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FieldDef {
    pub name: String,
    pub type_ann: TypeAnn,
    pub default: Option<Expr>,
    pub embedded: bool,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum TypeAnn {
    Simple(String),
    Array(Box<TypeAnn>),
    Generic(String, Vec<TypeAnn>),
    Function(Vec<TypeAnn>, Box<TypeAnn>),
    Optional(Box<TypeAnn>),
}

#[derive(Debug, Clone)]
pub struct Decorator {
    pub name: String,
    pub args: Vec<DecoratorArg>,
}

#[derive(Debug, Clone)]
pub enum DecoratorArg {
    Positional(Expr),
    Named(String, Expr),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<TypeAnn>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MethodSig {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeAnn>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum PipeStep {
    Keep(Box<Expr>),
    Sort(Option<String>),
    Take(Box<Expr>),
    Apply(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum DestructurePattern {
    Object(Vec<String>),
    Array {
        items: Vec<String>,
        rest: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Wildcard,
    Literal(Expr),
    Binding(String),
    Constructor { name: String, fields: Vec<Pattern> },
}
