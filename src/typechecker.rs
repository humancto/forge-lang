/// Forge Gradual Type Checker
/// Runs between parsing and interpretation.
/// Enforces type annotations when present, ignores when absent.
/// Does NOT reject programs without annotations (gradual typing).
///
/// With --strict: type mismatches are errors.
/// Without --strict: type mismatches are warnings.
use crate::parser::ast::*;
use std::collections::HashMap;

#[derive(Debug)]
pub struct TypeWarning {
    pub message: String,
    pub line: usize,
    pub col: usize,
    pub is_error: bool,
}

impl TypeWarning {
    fn warn(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            line: 0,
            col: 0,
            is_error: false,
        }
    }

    fn error(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            line: 0,
            col: 0,
            is_error: true,
        }
    }
}

pub struct TypeChecker {
    functions: HashMap<String, FnSignature>,
    type_defs: HashMap<String, Vec<String>>,
    interfaces: HashMap<String, Vec<InterfaceMethod>>,
    structs: HashMap<String, Vec<String>>,
    variables: HashMap<String, InferredType>,
    current_fn_return: Option<InferredType>,
    strict: bool,
    warnings: Vec<TypeWarning>,
}

#[derive(Debug, Clone)]
struct FnSignature {
    params: Vec<(String, Option<InferredType>)>,
    param_count: usize,
    return_type: Option<InferredType>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct InterfaceMethod {
    name: String,
    param_count: usize,
    return_type: Option<InferredType>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InferredType {
    Int,
    Float,
    String,
    Bool,
    Null,
    Array(Box<InferredType>),
    Object,
    Function(Vec<InferredType>, Box<InferredType>),
    Option(Box<InferredType>),
    Result(Box<InferredType>, Box<InferredType>),
    Named(std::string::String),
    Unknown,
}

impl std::fmt::Display for InferredType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferredType::Int => write!(f, "Int"),
            InferredType::Float => write!(f, "Float"),
            InferredType::String => write!(f, "String"),
            InferredType::Bool => write!(f, "Bool"),
            InferredType::Null => write!(f, "Null"),
            InferredType::Array(inner) => write!(f, "[{}]", inner),
            InferredType::Object => write!(f, "Object"),
            InferredType::Function(params, ret) => {
                let ps: Vec<std::string::String> =
                    params.iter().map(|p| format!("{}", p)).collect();
                write!(f, "fn({}) -> {}", ps.join(", "), ret)
            }
            InferredType::Option(inner) => write!(f, "?{}", inner),
            InferredType::Result(ok, err) => write!(f, "Result<{}, {}>", ok, err),
            InferredType::Named(n) => write!(f, "{}", n),
            InferredType::Unknown => write!(f, "Unknown"),
        }
    }
}

fn type_ann_to_inferred(ann: &TypeAnn) -> InferredType {
    match ann {
        TypeAnn::Simple(name) => match name.to_lowercase().as_str() {
            "int" | "i64" | "integer" => InferredType::Int,
            "float" | "f64" | "number" => InferredType::Float,
            "string" | "str" => InferredType::String,
            "bool" | "boolean" => InferredType::Bool,
            "null" | "void" => InferredType::Null,
            "object" | "json" => InferredType::Object,
            _ => InferredType::Named(name.clone()),
        },
        TypeAnn::Array(inner) => InferredType::Array(Box::new(type_ann_to_inferred(inner))),
        TypeAnn::Generic(name, args) => match name.as_str() {
            "Option" if args.len() == 1 => {
                InferredType::Option(Box::new(type_ann_to_inferred(&args[0])))
            }
            "Result" if args.len() == 2 => InferredType::Result(
                Box::new(type_ann_to_inferred(&args[0])),
                Box::new(type_ann_to_inferred(&args[1])),
            ),
            _ => InferredType::Named(name.clone()),
        },
        TypeAnn::Function(params, ret) => {
            let param_types: Vec<InferredType> = params.iter().map(type_ann_to_inferred).collect();
            InferredType::Function(param_types, Box::new(type_ann_to_inferred(ret)))
        }
        TypeAnn::Optional(inner) => InferredType::Option(Box::new(type_ann_to_inferred(inner))),
    }
}

fn types_compatible(expected: &InferredType, actual: &InferredType) -> bool {
    if *expected == InferredType::Unknown || *actual == InferredType::Unknown {
        return true;
    }
    if expected == actual {
        return true;
    }
    // Int and Float are compatible (numeric promotion)
    if matches!(
        (expected, actual),
        (InferredType::Int, InferredType::Float) | (InferredType::Float, InferredType::Int)
    ) {
        return true;
    }
    // Named types: same name matches; different names don't (interface check done separately)
    if let (InferredType::Named(a), InferredType::Named(b)) = (expected, actual) {
        return a == b;
    }
    // Named type matches Unknown
    if matches!(expected, InferredType::Named(_)) || matches!(actual, InferredType::Named(_)) {
        return false;
    }
    // Object matches any named type or Json
    if matches!(
        (expected, actual),
        (InferredType::Object, InferredType::Named(_))
            | (InferredType::Named(_), InferredType::Object)
    ) {
        return true;
    }
    // Option<T> accepts T, Null, or Option<U> where U is compatible with T
    if let InferredType::Option(inner) = expected {
        if *actual == InferredType::Null {
            return true;
        }
        if let InferredType::Option(actual_inner) = actual {
            return types_compatible(inner, actual_inner);
        }
        return types_compatible(inner, actual);
    }
    // Actual is Option<T>, expected is not — incompatible (except Unknown handled above)
    false
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            type_defs: HashMap::new(),
            interfaces: HashMap::new(),
            structs: HashMap::new(),
            variables: HashMap::new(),
            current_fn_return: None,
            strict: false,
            warnings: Vec::new(),
        }
    }

    pub fn with_strict(strict: bool) -> Self {
        Self {
            functions: HashMap::new(),
            type_defs: HashMap::new(),
            interfaces: HashMap::new(),
            structs: HashMap::new(),
            variables: HashMap::new(),
            current_fn_return: None,
            strict,
            warnings: Vec::new(),
        }
    }

    fn emit(&mut self, msg: impl Into<String>) {
        if self.strict {
            self.warnings.push(TypeWarning::error(msg));
        } else {
            self.warnings.push(TypeWarning::warn(msg));
        }
    }

    pub fn check(&mut self, program: &Program) -> Vec<TypeWarning> {
        for stmt in &program.statements {
            self.collect_definitions(stmt);
        }
        for stmt in &program.statements {
            self.check_stmt(stmt);
        }
        std::mem::take(&mut self.warnings)
    }

    fn collect_definitions(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FnDef {
                name,
                params,
                return_type,
                ..
            } => {
                let param_types: Vec<(String, Option<InferredType>)> = params
                    .iter()
                    .map(|p| {
                        (
                            p.name.clone(),
                            p.type_ann.as_ref().map(type_ann_to_inferred),
                        )
                    })
                    .collect();
                self.functions.insert(
                    name.clone(),
                    FnSignature {
                        param_count: params.len(),
                        params: param_types,
                        return_type: return_type.as_ref().map(type_ann_to_inferred),
                    },
                );
            }
            Stmt::TypeDef { name, variants } => {
                let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
                self.type_defs.insert(name.clone(), variant_names);
            }
            Stmt::InterfaceDef { name, methods } => {
                let method_sigs: Vec<InterfaceMethod> = methods
                    .iter()
                    .map(|m| InterfaceMethod {
                        name: m.name.clone(),
                        param_count: m.params.len(),
                        return_type: m.return_type.as_ref().map(type_ann_to_inferred),
                    })
                    .collect();
                self.interfaces.insert(name.clone(), method_sigs);
            }
            Stmt::StructDef { name, fields } => {
                let field_names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                self.structs.insert(name.clone(), field_names);
            }
            _ => {}
        }
    }

    fn check_interface_satisfaction(&mut self, struct_name: &str, interface_name: &str) {
        let struct_fields = match self.structs.get(struct_name) {
            Some(f) => f.clone(),
            None => return,
        };
        let methods = match self.interfaces.get(interface_name) {
            Some(m) => m.clone(),
            None => return,
        };

        for method in &methods {
            let has_field = struct_fields.iter().any(|f| *f == method.name);
            let has_fn = self
                .functions
                .get(&format!("{}_{}", struct_name, method.name))
                .is_some()
                || self.functions.get(&method.name).is_some();
            if !has_field && !has_fn {
                self.emit(format!(
                    "struct '{}' does not satisfy interface '{}': missing '{}'",
                    struct_name, interface_name, method.name
                ));
            }
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                name,
                value,
                type_ann,
                ..
            } => {
                let inferred = self.infer_expr(value);
                if let Some(ann) = type_ann {
                    let expected = type_ann_to_inferred(ann);
                    if inferred != InferredType::Unknown && !types_compatible(&expected, &inferred)
                    {
                        self.emit(format!(
                            "type mismatch: '{}' declared as {} but assigned {}",
                            name, expected, inferred
                        ));
                    }
                    self.variables.insert(name.clone(), expected);
                } else {
                    self.variables.insert(name.clone(), inferred);
                }
            }
            Stmt::Assign { target, value } => {
                let val_type = self.infer_expr(value);
                if let Expr::Ident(name) = target {
                    if let Some(var_type) = self.variables.get(name).cloned() {
                        if var_type != InferredType::Unknown
                            && val_type != InferredType::Unknown
                            && !types_compatible(&var_type, &val_type)
                        {
                            self.emit(format!(
                                "type mismatch: '{}' is {} but assigned {}",
                                name, var_type, val_type
                            ));
                        }
                    }
                }
            }
            Stmt::FnDef {
                name,
                params,
                body,
                return_type,
                ..
            } => {
                let prev_return = self.current_fn_return.take();
                self.current_fn_return = return_type.as_ref().map(type_ann_to_inferred);

                for param in params {
                    if let Some(ref ann) = param.type_ann {
                        self.variables
                            .insert(param.name.clone(), type_ann_to_inferred(ann));
                    }
                }

                for s in body {
                    self.check_stmt(s);
                }

                // Collect definitions for nested functions
                for s in body {
                    self.collect_definitions(s);
                }

                self.current_fn_return = prev_return;
                let _ = name;
            }
            Stmt::Return(Some(expr)) => {
                let returned = self.infer_expr(expr);
                if let Some(ref expected) = self.current_fn_return {
                    if returned != InferredType::Unknown && !types_compatible(expected, &returned) {
                        self.emit(format!(
                            "return type mismatch: expected {} but returning {}",
                            expected, returned
                        ));
                    }
                }
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                let cond_type = self.infer_expr(condition);
                if cond_type != InferredType::Unknown && cond_type != InferredType::Bool {
                    // Not an error in a dynamic language, just informational
                }
                for s in then_body {
                    self.check_stmt(s);
                }
                if let Some(else_b) = else_body {
                    for s in else_b {
                        self.check_stmt(s);
                    }
                }
            }
            Stmt::For { iterable, body, .. } => {
                self.infer_expr(iterable);
                for s in body {
                    self.check_stmt(s);
                }
            }
            Stmt::While { condition, body } => {
                self.infer_expr(condition);
                for s in body {
                    self.check_stmt(s);
                }
            }
            Stmt::Loop { body } | Stmt::Spawn { body } => {
                for s in body {
                    self.check_stmt(s);
                }
            }
            Stmt::Return(None) => {}
            Stmt::Expression(expr) => {
                self.infer_expr(expr);
            }
            Stmt::Match { subject, arms } => {
                self.infer_expr(subject);
                for arm in arms {
                    for s in &arm.body {
                        self.check_stmt(s);
                    }
                }
            }
            _ => {}
        }
    }

    fn infer_expr(&mut self, expr: &Expr) -> InferredType {
        match expr {
            Expr::Int(_) => InferredType::Int,
            Expr::Float(_) => InferredType::Float,
            Expr::StringLit(_) => InferredType::String,
            Expr::Bool(_) => InferredType::Bool,

            Expr::Ident(name) => {
                if name == "None" {
                    return InferredType::Option(Box::new(InferredType::Unknown));
                }
                if let Some(t) = self.variables.get(name) {
                    return t.clone();
                }
                if self.functions.contains_key(name) {
                    return InferredType::Function(vec![], Box::new(InferredType::Unknown));
                }
                InferredType::Unknown
            }

            Expr::StringInterp(_) => InferredType::String,

            Expr::BinOp { left, op, right } => {
                let lt = self.infer_expr(left);
                let rt = self.infer_expr(right);
                match op {
                    BinOp::Eq
                    | BinOp::NotEq
                    | BinOp::Lt
                    | BinOp::Gt
                    | BinOp::LtEq
                    | BinOp::GtEq
                    | BinOp::And
                    | BinOp::Or => InferredType::Bool,
                    BinOp::Add => {
                        if lt == InferredType::String || rt == InferredType::String {
                            InferredType::String
                        } else if lt == InferredType::Float || rt == InferredType::Float {
                            InferredType::Float
                        } else if lt == InferredType::Int && rt == InferredType::Int {
                            InferredType::Int
                        } else {
                            InferredType::Unknown
                        }
                    }
                    BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        if lt == InferredType::Float || rt == InferredType::Float {
                            InferredType::Float
                        } else if lt == InferredType::Int && rt == InferredType::Int {
                            InferredType::Int
                        } else {
                            InferredType::Unknown
                        }
                    }
                }
            }

            Expr::UnaryOp { op, operand } => {
                let t = self.infer_expr(operand);
                match op {
                    UnaryOp::Neg => t,
                    UnaryOp::Not => InferredType::Bool,
                }
            }

            Expr::Call { function, args } => {
                if let Expr::Ident(name) = function.as_ref() {
                    if let Some(sig) = self.functions.get(name).cloned() {
                        // Arity check
                        if args.len() != sig.param_count
                            && !args.iter().any(|a| matches!(a, Expr::Spread(_)))
                        {
                            self.emit(format!(
                                "function '{}' expects {} argument(s), got {}",
                                name,
                                sig.param_count,
                                args.len()
                            ));
                        }

                        // Argument type check
                        for (i, arg) in args.iter().enumerate() {
                            let arg_type = self.infer_expr(arg);
                            if let Some((_, Some(expected))) = sig.params.get(i) {
                                if arg_type != InferredType::Unknown
                                    && !types_compatible(expected, &arg_type)
                                {
                                    // Check interface satisfaction before emitting error
                                    if let (
                                        InferredType::Named(iface_name),
                                        InferredType::Named(struct_name),
                                    ) = (expected, &arg_type)
                                    {
                                        if self.interfaces.contains_key(iface_name) {
                                            self.check_interface_satisfaction(
                                                struct_name,
                                                iface_name,
                                            );
                                            continue;
                                        }
                                    }
                                    self.emit(format!(
                                        "argument {} of '{}': expected {} but got {}",
                                        i + 1,
                                        name,
                                        expected,
                                        arg_type
                                    ));
                                }
                            }
                        }

                        return sig.return_type.unwrap_or(InferredType::Unknown);
                    }

                    match name.as_str() {
                        "len" => return InferredType::Int,
                        "str" | "type" | "typeof" | "uuid" | "cwd" | "sh" => {
                            return InferredType::String
                        }
                        "int" => return InferredType::Int,
                        "float" => return InferredType::Float,
                        "Ok" | "ok" => {
                            return InferredType::Result(
                                Box::new(InferredType::Unknown),
                                Box::new(InferredType::Unknown),
                            )
                        }
                        "Err" | "err" => {
                            return InferredType::Result(
                                Box::new(InferredType::Unknown),
                                Box::new(InferredType::Unknown),
                            )
                        }
                        "Some" => {
                            let inner = args
                                .first()
                                .map(|a| self.infer_expr(a))
                                .unwrap_or(InferredType::Unknown);
                            return InferredType::Option(Box::new(inner));
                        }
                        "is_ok" | "is_err" | "is_some" | "is_none" | "contains" | "starts_with"
                        | "ends_with" | "sh_ok" | "satisfies" => return InferredType::Bool,
                        "range" | "map" | "filter" | "sort" | "reverse" | "keys" | "values"
                        | "split" | "sh_lines" | "entries" => {
                            return InferredType::Array(Box::new(InferredType::Unknown))
                        }
                        "fetch" | "shell" | "sh_json" | "merge" => return InferredType::Object,
                        _ => {}
                    }
                }

                for arg in args {
                    self.infer_expr(arg);
                }
                InferredType::Unknown
            }

            Expr::Array(items) => {
                let mut elem_type = InferredType::Unknown;
                for item in items {
                    let t = self.infer_expr(item);
                    if elem_type == InferredType::Unknown {
                        elem_type = t;
                    }
                }
                InferredType::Array(Box::new(elem_type))
            }

            Expr::Object(fields) => {
                for (_, val) in fields {
                    self.infer_expr(val);
                }
                InferredType::Object
            }

            Expr::FieldAccess { object, .. } => {
                self.infer_expr(object);
                InferredType::Unknown
            }

            Expr::Index { object, index } => {
                let obj_type = self.infer_expr(object);
                self.infer_expr(index);
                if let InferredType::Array(inner) = obj_type {
                    return *inner;
                }
                InferredType::Unknown
            }

            Expr::Pipeline { value, function } => {
                self.infer_expr(value);
                self.infer_expr(function);
                InferredType::Unknown
            }

            Expr::Lambda { params, body, .. } => {
                for p in params {
                    if let Some(ref ann) = p.type_ann {
                        self.variables
                            .insert(p.name.clone(), type_ann_to_inferred(ann));
                    }
                }
                for s in body {
                    self.check_stmt(s);
                }
                InferredType::Function(vec![], Box::new(InferredType::Unknown))
            }

            Expr::Try(inner) => {
                self.infer_expr(inner);
                InferredType::Unknown
            }

            Expr::StructInit { name, fields } => {
                for (_, val) in fields {
                    self.infer_expr(val);
                }
                InferredType::Named(name.clone())
            }

            Expr::MethodCall { object, args, .. } => {
                self.infer_expr(object);
                for arg in args {
                    self.infer_expr(arg);
                }
                InferredType::Unknown
            }

            Expr::Block(stmts) => {
                for s in stmts {
                    self.check_stmt(s);
                }
                InferredType::Unknown
            }

            Expr::Await(inner) | Expr::Must(inner) | Expr::Freeze(inner) | Expr::Ask(inner) => {
                self.infer_expr(inner)
            }

            Expr::Spawn(body) => {
                for stmt in body {
                    self.check_stmt(stmt);
                }
                InferredType::Unknown // TaskHandle type
            }

            Expr::Spread(inner) => self.infer_expr(inner),

            Expr::WhereFilter { source, .. } => {
                self.infer_expr(source);
                InferredType::Array(Box::new(InferredType::Unknown))
            }

            Expr::PipeChain { source, .. } => {
                self.infer_expr(source);
                InferredType::Unknown
            }
        }
    }
}

fn type_ann_to_string(ann: &TypeAnn) -> String {
    match ann {
        TypeAnn::Simple(name) => name.clone(),
        TypeAnn::Array(inner) => format!("[{}]", type_ann_to_string(inner)),
        TypeAnn::Generic(name, args) => {
            let arg_strs: Vec<String> = args.iter().map(type_ann_to_string).collect();
            format!("{}<{}>", name, arg_strs.join(", "))
        }
        TypeAnn::Function(params, ret) => {
            let param_strs: Vec<String> = params.iter().map(type_ann_to_string).collect();
            format!("({}) -> {}", param_strs.join(", "), type_ann_to_string(ret))
        }
        TypeAnn::Optional(inner) => format!("?{}", type_ann_to_string(inner)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn check_source(source: &str, strict: bool) -> Vec<TypeWarning> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let mut checker = TypeChecker::with_strict(strict);
        checker.check(&program)
    }

    fn warnings_for(source: &str) -> Vec<TypeWarning> {
        check_source(source, false)
    }

    fn errors_for(source: &str) -> Vec<TypeWarning> {
        check_source(source, true)
    }

    #[test]
    fn no_warnings_for_unannotated_code() {
        let w = warnings_for("let x = 42\nlet y = x + 1\nprintln(y)");
        assert!(w.is_empty());
    }

    #[test]
    fn no_warnings_for_correct_annotations() {
        let w = warnings_for("let x: Int = 42");
        assert!(w.is_empty());
    }

    #[test]
    fn warns_on_let_type_mismatch() {
        let w = warnings_for("let x: Int = \"hello\"");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("type mismatch"));
        assert!(w[0].message.contains("Int"));
        assert!(w[0].message.contains("String"));
        assert!(!w[0].is_error);
    }

    #[test]
    fn strict_mode_produces_errors() {
        let w = errors_for("let x: Int = \"hello\"");
        assert_eq!(w.len(), 1);
        assert!(w[0].is_error);
    }

    #[test]
    fn warns_on_return_type_mismatch() {
        let w = warnings_for("fn add(a: Int, b: Int) -> Int { return \"oops\" }");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("return type mismatch"));
    }

    #[test]
    fn no_warning_for_correct_return() {
        let w = warnings_for("fn add(a: Int, b: Int) -> Int { return a + b }");
        assert!(w.is_empty());
    }

    #[test]
    fn warns_on_arity_mismatch() {
        let w = warnings_for("fn add(a, b) { return a + b }\nadd(1, 2, 3)");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("expects 2"));
    }

    #[test]
    fn warns_on_argument_type_mismatch() {
        let w = warnings_for("fn double(x: Int) -> Int { return x * 2 }\ndouble(\"hello\")");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("argument 1"));
        assert!(w[0].message.contains("expected Int"));
        assert!(w[0].message.contains("got String"));
    }

    #[test]
    fn no_warning_for_correct_args() {
        let w = warnings_for("fn double(x: Int) -> Int { return x * 2 }\ndouble(5)");
        assert!(w.is_empty());
    }

    #[test]
    fn infers_string_concatenation() {
        let w = warnings_for("let x: Int = \"a\" + \"b\"");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("String"));
    }

    #[test]
    fn infers_float_promotion() {
        let w = warnings_for("let x: Int = 1 + 2.5");
        // 1 + 2.5 = Float, assigned to Int — but Int/Float are compatible
        assert!(w.is_empty());
    }

    #[test]
    fn infers_comparison_as_bool() {
        let w = warnings_for("let x: Int = 5 > 3");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("Bool"));
    }

    #[test]
    fn infers_array_type() {
        let w = warnings_for("let x: Int = [1, 2, 3]");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("[Int]"));
    }

    #[test]
    fn infers_object_type() {
        let w = warnings_for("let x: Int = { name: \"Odin\" }");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("Object"));
    }

    #[test]
    fn assignment_type_check() {
        let w = warnings_for("let mut x: Int = 5\nx = \"hello\"");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("type mismatch"));
    }

    #[test]
    fn unannotated_code_no_errors_strict() {
        let w = errors_for("let x = 42\nlet y = \"hello\"");
        assert!(w.is_empty());
    }

    #[test]
    fn builtin_return_types_known() {
        let w = warnings_for("let x: String = len([1,2,3])");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("Int"));
    }

    #[test]
    fn string_interp_inferred_as_string() {
        let w = warnings_for("let name = \"world\"\nlet x: Int = \"hello {name}\"");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("String"));
    }

    #[test]
    fn negation_preserves_type() {
        let w = warnings_for("let x: String = -5");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("Int"));
    }

    #[test]
    fn not_always_bool() {
        let w = warnings_for("let x: Int = !true");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("Bool"));
    }

    #[test]
    fn lambda_body_checked() {
        let w = warnings_for(
            "fn takes_int(x: Int) -> Int { return x }\nlet f = fn() { takes_int(\"bad\") }",
        );
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("argument 1"));
    }

    #[test]
    fn multiple_errors() {
        let w = warnings_for(
            "let x: Int = \"hello\"\nlet y: String = 42\nfn f(a, b) { return a }\nf(1)",
        );
        assert_eq!(w.len(), 3);
    }

    #[test]
    fn interface_satisfaction_pass() {
        let w = warnings_for(
            "interface Printable { fn display() -> String }\nstruct User { name: String, display: String }\nfn show(p: Printable) { println(p) }\nlet u = User { name: \"Alice\", display: \"Alice\" }\nshow(u)",
        );
        // User has 'display' field, satisfies Printable — no warning
        let interface_warnings: Vec<_> =
            w.iter().filter(|w| w.message.contains("satisfy")).collect();
        assert!(interface_warnings.is_empty());
    }

    #[test]
    fn interface_satisfaction_fail() {
        let w = warnings_for(
            "interface Serializable { fn serialize() -> String }\nstruct Point { x: Int, y: Int }\nfn save(s: Serializable) { println(s) }\nlet p = Point { x: 1, y: 2 }\nsave(p)",
        );
        let interface_warnings: Vec<_> =
            w.iter().filter(|w| w.message.contains("satisfy")).collect();
        assert!(!interface_warnings.is_empty());
        assert!(interface_warnings[0].message.contains("serialize"));
    }

    // ========== M3.3: Option<T> Type Checking ==========

    #[test]
    fn option_type_annotation_accepts_none() {
        let w = warnings_for("let x: ?Int = None");
        assert!(w.is_empty(), "None should be valid for ?Int");
    }

    #[test]
    fn option_type_annotation_accepts_some() {
        let w = warnings_for("let x: ?Int = Some(42)");
        assert!(w.is_empty(), "Some(42) should be valid for ?Int");
    }

    #[test]
    fn non_optional_rejects_none() {
        let w = warnings_for("let x: Int = None");
        assert!(!w.is_empty(), "None should not be valid for bare Int");
    }

    #[test]
    fn some_inferred_as_option_type() {
        let w = warnings_for("let x: Int = Some(42)");
        assert!(!w.is_empty(), "Some(42) is Option, not Int");
        assert!(w[0].message.contains("Option") || w[0].message.contains("?"));
    }
}
