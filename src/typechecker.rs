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
#[allow(dead_code)]
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
    structs: HashMap<String, StructInfo>,
    type_aliases: HashMap<String, InferredType>,
    variables: HashMap<String, InferredType>,
    current_fn_return: Option<InferredType>,
    current_line: usize,
    strict: bool,
    warnings: Vec<TypeWarning>,
}

#[derive(Debug, Clone)]
struct FnSignature {
    type_params: Vec<String>,
    params: Vec<(String, Option<InferredType>)>,
    param_count: usize,
    return_type: Option<InferredType>,
}

#[derive(Debug, Clone)]
struct StructInfo {
    type_params: Vec<String>,
    fields: Vec<(String, InferredType)>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct InterfaceMethod {
    name: String,
    param_count: usize,
    return_type: Option<InferredType>,
}

#[derive(Debug, Clone, PartialEq)]
enum NarrowingFact {
    NonNull,
    IsNull,
    IsOk,
    IsErr,
}

impl NarrowingFact {
    fn invert(&self) -> NarrowingFact {
        match self {
            NarrowingFact::NonNull => NarrowingFact::IsNull,
            NarrowingFact::IsNull => NarrowingFact::NonNull,
            NarrowingFact::IsOk => NarrowingFact::IsErr,
            NarrowingFact::IsErr => NarrowingFact::IsOk,
        }
    }
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
    Union(Vec<InferredType>),
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
            InferredType::Union(variants) => {
                let vs: Vec<std::string::String> =
                    variants.iter().map(|v| format!("{}", v)).collect();
                write!(f, "{}", vs.join(" | "))
            }
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

fn is_null_expr(expr: &Expr) -> bool {
    matches!(expr, Expr::Ident(name) if name == "null" || name == "None")
}

fn types_compatible(expected: &InferredType, actual: &InferredType) -> bool {
    if *expected == InferredType::Unknown || *actual == InferredType::Unknown {
        return true;
    }
    if expected == actual {
        return true;
    }
    // Array types: compare element types recursively
    if let (InferredType::Array(expected_elem), InferredType::Array(actual_elem)) =
        (expected, actual)
    {
        return types_compatible(expected_elem, actual_elem);
    }
    // Union types: actual is compatible with expected union if it matches any variant
    if let InferredType::Union(variants) = expected {
        return variants.iter().any(|v| types_compatible(v, actual));
    }
    // Union actual: compatible if all variants match expected
    if let InferredType::Union(variants) = actual {
        return variants.iter().all(|v| types_compatible(expected, v));
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

#[allow(dead_code)]
impl TypeChecker {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            type_defs: HashMap::new(),
            interfaces: HashMap::new(),
            structs: HashMap::new(),
            type_aliases: HashMap::new(),
            variables: HashMap::new(),
            current_fn_return: None,
            current_line: 0,
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
            type_aliases: HashMap::new(),
            variables: HashMap::new(),
            current_fn_return: None,
            current_line: 0,
            strict,
            warnings: Vec::new(),
        }
    }

    fn emit(&mut self, msg: impl Into<String>) {
        let mut warning = if self.strict {
            TypeWarning::error(msg)
        } else {
            TypeWarning::warn(msg)
        };
        warning.line = self.current_line;
        self.warnings.push(warning);
    }

    /// Resolve a type through type aliases. If it's a Named type that matches
    /// a type alias, return the aliased type. Otherwise return as-is.
    fn resolve_alias(&self, ty: &InferredType) -> InferredType {
        if let InferredType::Named(name) = ty {
            if let Some(aliased) = self.type_aliases.get(name) {
                return aliased.clone();
            }
        }
        ty.clone()
    }

    pub fn check(&mut self, program: &Program) -> Vec<TypeWarning> {
        // Pass 1: collect all function/type/struct/interface definitions
        for spanned in &program.statements {
            self.collect_definitions(&spanned.stmt);
        }
        // Pass 1.5: infer return types for unannotated functions
        self.infer_all_return_types(&program.statements);
        // Pass 2: full type checking
        for spanned in &program.statements {
            self.current_line = spanned.line;
            self.check_stmt(&spanned.stmt);
        }
        std::mem::take(&mut self.warnings)
    }

    /// Pass 1.5: For each function without an explicit return type annotation,
    /// walk the body to collect return types and infer a unified return type.
    fn infer_all_return_types(&mut self, stmts: &[SpannedStmt]) {
        for spanned in stmts {
            match &spanned.stmt {
                Stmt::FnDef {
                    name,
                    params,
                    body,
                    return_type: None,
                    ..
                } => {
                    // Temporarily register param types so expr inference works
                    let saved_vars: Vec<_> = params
                        .iter()
                        .filter_map(|p| {
                            let old = self.variables.get(&p.name).cloned();
                            if let Some(ref ann) = p.type_ann {
                                self.variables
                                    .insert(p.name.clone(), type_ann_to_inferred(ann));
                            }
                            Some((p.name.clone(), old))
                        })
                        .collect();

                    let inferred = self.infer_body_return_type(body);

                    // Restore previous variable state
                    for (name, old_val) in saved_vars {
                        match old_val {
                            Some(v) => {
                                self.variables.insert(name, v);
                            }
                            None => {
                                self.variables.remove(&name);
                            }
                        }
                    }

                    if inferred != InferredType::Unknown {
                        if let Some(sig) = self.functions.get_mut(name) {
                            sig.return_type = Some(inferred);
                        }
                    }
                }
                Stmt::ImplBlock {
                    type_name, methods, ..
                } => {
                    for method_spanned in methods {
                        if let Stmt::FnDef {
                            name: method_name,
                            params,
                            body,
                            return_type: None,
                            ..
                        } = &method_spanned.stmt
                        {
                            let qualified = format!("{}::{}", type_name, method_name);

                            let saved_vars: Vec<_> = params
                                .iter()
                                .filter_map(|p| {
                                    let old = self.variables.get(&p.name).cloned();
                                    if let Some(ref ann) = p.type_ann {
                                        self.variables
                                            .insert(p.name.clone(), type_ann_to_inferred(ann));
                                    }
                                    Some((p.name.clone(), old))
                                })
                                .collect();

                            let inferred = self.infer_body_return_type(body);

                            for (name, old_val) in saved_vars {
                                match old_val {
                                    Some(v) => {
                                        self.variables.insert(name, v);
                                    }
                                    None => {
                                        self.variables.remove(&name);
                                    }
                                }
                            }

                            if inferred != InferredType::Unknown {
                                if let Some(sig) = self.functions.get_mut(&qualified) {
                                    sig.return_type = Some(inferred);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Infer the return type of a function body by collecting all return types
    /// (explicit returns + implicit last expression) and unifying them.
    fn infer_body_return_type(&mut self, body: &[SpannedStmt]) -> InferredType {
        let mut return_types = Vec::new();
        self.collect_return_types(body, &mut return_types);

        // Also check the implicit last expression (if the last statement is an expression)
        if let Some(last) = body.last() {
            if let Stmt::Expression(expr) = &last.stmt {
                let t = self.infer_expr(expr);
                if t != InferredType::Unknown {
                    return_types.push(t);
                }
            }
        }

        if return_types.is_empty() {
            return InferredType::Null;
        }

        self.unify_types(&return_types)
    }

    /// Recursively walk statements to find all explicit return types.
    fn collect_return_types(&mut self, stmts: &[SpannedStmt], out: &mut Vec<InferredType>) {
        for spanned in stmts {
            match &spanned.stmt {
                Stmt::Return(Some(expr)) => {
                    let t = self.infer_expr(expr);
                    if t != InferredType::Unknown {
                        out.push(t);
                    }
                }
                Stmt::Return(None) => {
                    out.push(InferredType::Null);
                }
                Stmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    self.collect_return_types(then_body, out);
                    if let Some(else_b) = else_body {
                        self.collect_return_types(else_b, out);
                    }
                }
                Stmt::For { body, .. }
                | Stmt::While { body, .. }
                | Stmt::Loop { body }
                | Stmt::Spawn { body } => {
                    self.collect_return_types(body, out);
                }
                Stmt::Match { arms, .. } => {
                    for arm in arms {
                        self.collect_return_types(&arm.body, out);
                    }
                }
                Stmt::TryCatch {
                    try_body,
                    catch_body,
                    ..
                } => {
                    self.collect_return_types(try_body, out);
                    self.collect_return_types(catch_body, out);
                }
                // Don't recurse into nested function definitions
                Stmt::FnDef { .. } => {}
                _ => {}
            }
        }
    }

    /// Unify a list of inferred types into a single type.
    /// If all agree, returns that type. Int+Float promotes to Float.
    /// Otherwise returns Unknown.
    fn unify_types(&self, types: &[InferredType]) -> InferredType {
        if types.is_empty() {
            return InferredType::Unknown;
        }

        let mut unified = types[0].clone();
        for t in &types[1..] {
            if *t == unified {
                continue;
            }
            // Numeric promotion: Int + Float → Float
            if matches!(
                (&unified, t),
                (InferredType::Int, InferredType::Float) | (InferredType::Float, InferredType::Int)
            ) {
                unified = InferredType::Float;
                continue;
            }
            // Incompatible types
            return InferredType::Unknown;
        }

        unified
    }

    fn collect_definitions(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FnDef {
                name,
                type_params,
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
                        type_params: type_params.clone(),
                        param_count: params.len(),
                        params: param_types,
                        return_type: return_type.as_ref().map(type_ann_to_inferred),
                    },
                );
            }
            Stmt::TypeDef { name, variants } => {
                let variant_names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
                self.type_defs.insert(name.clone(), variant_names.clone());

                // Detect union types: all variants have no fields and are type names
                let is_union = !variants.is_empty() && variants.iter().all(|v| v.fields.is_empty());
                if is_union {
                    let types: Vec<InferredType> = variant_names
                        .iter()
                        .map(|n| type_ann_to_inferred(&TypeAnn::Simple(n.clone())))
                        .collect();
                    if types.len() == 1 {
                        // Single-variant: simple alias
                        self.type_aliases
                            .insert(name.clone(), types.into_iter().next().unwrap());
                    } else {
                        self.type_aliases
                            .insert(name.clone(), InferredType::Union(types));
                    }
                }
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
            Stmt::StructDef {
                name,
                type_params,
                fields,
                ..
            } => {
                let field_info: Vec<(String, InferredType)> = fields
                    .iter()
                    .map(|f| (f.name.clone(), type_ann_to_inferred(&f.type_ann)))
                    .collect();
                self.structs.insert(
                    name.clone(),
                    StructInfo {
                        type_params: type_params.clone(),
                        fields: field_info,
                    },
                );
            }
            Stmt::ImplBlock {
                type_name, methods, ..
            } => {
                for spanned_method in methods {
                    if let Stmt::FnDef {
                        name: method_name,
                        params,
                        return_type,
                        ..
                    } = &spanned_method.stmt
                    {
                        let qualified = format!("{}::{}", type_name, method_name);
                        let param_info: Vec<(String, Option<InferredType>)> = params
                            .iter()
                            .map(|p| {
                                (
                                    p.name.clone(),
                                    p.type_ann.as_ref().map(type_ann_to_inferred),
                                )
                            })
                            .collect();
                        let ret = return_type.as_ref().map(type_ann_to_inferred);
                        let method_type_params =
                            if let Stmt::FnDef { type_params, .. } = &spanned_method.stmt {
                                type_params.clone()
                            } else {
                                vec![]
                            };
                        self.functions.insert(
                            qualified,
                            FnSignature {
                                type_params: method_type_params,
                                param_count: params.len(),
                                params: param_info,
                                return_type: ret,
                            },
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn check_interface_satisfaction(&mut self, struct_name: &str, interface_name: &str) {
        let struct_info = match self.structs.get(struct_name) {
            Some(info) => info.clone(),
            None => return,
        };
        let methods = match self.interfaces.get(interface_name) {
            Some(m) => m.clone(),
            None => return,
        };

        for method in &methods {
            let has_field = struct_info
                .fields
                .iter()
                .any(|(name, _)| *name == method.name);

            // Look up the function: try impl block (::), legacy (_), or bare name
            let fn_sig = self
                .functions
                .get(&format!("{}::{}", struct_name, method.name))
                .or_else(|| {
                    self.functions
                        .get(&format!("{}_{}", struct_name, method.name))
                })
                .or_else(|| self.functions.get(&method.name))
                .cloned();

            if !has_field && fn_sig.is_none() {
                self.emit(format!(
                    "struct '{}' does not satisfy interface '{}': missing '{}'",
                    struct_name, interface_name, method.name
                ));
                continue;
            }

            // Validate signature if we found a function (not just a field)
            if let Some(ref sig) = fn_sig {
                // Check param count (interface methods don't include `self`,
                // but impl methods may have `self` as first param)
                let impl_params = if sig.params.first().map(|(n, _)| n.as_str()) == Some("self") {
                    sig.param_count.saturating_sub(1)
                } else {
                    sig.param_count
                };
                if impl_params != method.param_count {
                    self.emit(format!(
                        "method '{}' on '{}' has {} parameter(s) but interface '{}' requires {}",
                        method.name, struct_name, impl_params, interface_name, method.param_count
                    ));
                }

                // Check return type compatibility
                if let (Some(expected_ret), Some(actual_ret)) =
                    (&method.return_type, &sig.return_type)
                {
                    if !types_compatible(expected_ret, actual_ret) {
                        self.emit(format!(
                            "method '{}' on '{}' returns {} but interface '{}' expects {}",
                            method.name, struct_name, actual_ret, interface_name, expected_ret
                        ));
                    }
                }
            }
        }
    }

    /// Extract narrowing facts from a condition expression.
    /// Returns (variable_name, fact) pairs.
    fn extract_narrowing(&self, expr: &Expr) -> Vec<(String, NarrowingFact)> {
        match expr {
            // x != null / x != None → x is non-null
            Expr::BinOp {
                left,
                op: BinOp::NotEq,
                right,
            } => {
                if let (Expr::Ident(name), true) = (left.as_ref(), is_null_expr(right)) {
                    vec![(name.clone(), NarrowingFact::NonNull)]
                } else if let (true, Expr::Ident(name)) = (is_null_expr(left), right.as_ref()) {
                    vec![(name.clone(), NarrowingFact::NonNull)]
                } else {
                    vec![]
                }
            }
            // x == null / x == None → x is null
            Expr::BinOp {
                left,
                op: BinOp::Eq,
                right,
            } => {
                if let (Expr::Ident(name), true) = (left.as_ref(), is_null_expr(right)) {
                    vec![(name.clone(), NarrowingFact::IsNull)]
                } else if let (true, Expr::Ident(name)) = (is_null_expr(left), right.as_ref()) {
                    vec![(name.clone(), NarrowingFact::IsNull)]
                } else {
                    vec![]
                }
            }
            // x && y → collect facts from both sides
            Expr::BinOp {
                left,
                op: BinOp::And,
                right,
            } => {
                let mut facts = self.extract_narrowing(left);
                facts.extend(self.extract_narrowing(right));
                facts
            }
            // !(expr) → invert all facts from inner
            Expr::UnaryOp {
                op: UnaryOp::Not,
                operand,
            } => self
                .extract_narrowing(operand)
                .into_iter()
                .map(|(name, fact)| (name, fact.invert()))
                .collect(),
            // is_some(x) → non-null, is_none(x) → null
            // is_ok(x) → ok, is_err(x) → err
            Expr::Call { function, args } => {
                if let Expr::Ident(fname) = function.as_ref() {
                    if args.len() == 1 {
                        if let Expr::Ident(var_name) = &args[0] {
                            match fname.as_str() {
                                "is_some" => {
                                    return vec![(var_name.clone(), NarrowingFact::NonNull)]
                                }
                                "is_none" => {
                                    return vec![(var_name.clone(), NarrowingFact::IsNull)]
                                }
                                "is_ok" => return vec![(var_name.clone(), NarrowingFact::IsOk)],
                                "is_err" => return vec![(var_name.clone(), NarrowingFact::IsErr)],
                                _ => {}
                            }
                        }
                    }
                }
                vec![]
            }
            _ => vec![],
        }
    }

    /// Apply a narrowing fact to a type.
    fn narrow_type(current: &InferredType, fact: &NarrowingFact) -> InferredType {
        match (current, fact) {
            (InferredType::Option(inner), NarrowingFact::NonNull) => *inner.clone(),
            (InferredType::Option(_), NarrowingFact::IsNull) => InferredType::Null,
            (InferredType::Result(ok, _), NarrowingFact::IsOk) => *ok.clone(),
            (InferredType::Result(_, err), NarrowingFact::IsErr) => *err.clone(),
            (InferredType::Unknown, _) => InferredType::Unknown,
            (t, NarrowingFact::NonNull) => t.clone(), // already non-null
            (_, NarrowingFact::IsNull) => InferredType::Null,
            _ => current.clone(),
        }
    }

    /// Check if the last statement in a body is a guaranteed exit (return/break).
    fn body_always_returns(body: &[SpannedStmt]) -> bool {
        body.last()
            .map_or(false, |s| matches!(s.stmt, Stmt::Return(_)))
    }

    /// Check if a match expression covers all variants of a known type.
    fn check_match_exhaustiveness(&mut self, subject_type: &InferredType, arms: &[MatchArm]) {
        // Check if any arm is a wildcard/catch-all (makes match exhaustive)
        for arm in arms {
            match &arm.pattern {
                Pattern::Wildcard => return,
                Pattern::Binding(_) => return, // any binding is a catch-all
                _ => {}
            }
        }

        // Determine required variants and check coverage
        match subject_type {
            InferredType::Option(_) => {
                let mut has_some = false;
                let mut has_none = false;
                for arm in arms {
                    match &arm.pattern {
                        Pattern::Constructor { name, .. } if name == "Some" => has_some = true,
                        Pattern::Literal(expr) if is_null_expr(expr) => has_none = true,
                        _ => {}
                    }
                }
                let mut missing = Vec::new();
                if !has_some {
                    missing.push("Some");
                }
                if !has_none {
                    missing.push("None");
                }
                if !missing.is_empty() {
                    self.emit(format!(
                        "non-exhaustive match on {} — missing: {}",
                        subject_type,
                        missing.join(", ")
                    ));
                }
            }
            InferredType::Result(_, _) => {
                let mut has_ok = false;
                let mut has_err = false;
                for arm in arms {
                    if let Pattern::Constructor { name, .. } = &arm.pattern {
                        match name.as_str() {
                            "Ok" => has_ok = true,
                            "Err" => has_err = true,
                            _ => {}
                        }
                    }
                }
                let mut missing = Vec::new();
                if !has_ok {
                    missing.push("Ok");
                }
                if !has_err {
                    missing.push("Err");
                }
                if !missing.is_empty() {
                    self.emit(format!(
                        "non-exhaustive match on {} — missing: {}",
                        subject_type,
                        missing.join(", ")
                    ));
                }
            }
            InferredType::Bool => {
                let mut has_true = false;
                let mut has_false = false;
                for arm in arms {
                    if let Pattern::Literal(Expr::Bool(v)) = &arm.pattern {
                        if *v {
                            has_true = true;
                        } else {
                            has_false = true;
                        }
                    }
                }
                let mut missing = Vec::new();
                if !has_true {
                    missing.push("true");
                }
                if !has_false {
                    missing.push("false");
                }
                if !missing.is_empty() {
                    self.emit(format!(
                        "non-exhaustive match on Bool — missing: {}",
                        missing.join(", ")
                    ));
                }
            }
            _ => {} // Unknown, Int, String, etc. — cannot check exhaustiveness
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
                    let expected = self.resolve_alias(&type_ann_to_inferred(ann));
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
                    self.current_line = s.line;
                    self.check_stmt(&s.stmt);
                }

                // Collect definitions for nested functions
                for s in body {
                    self.collect_definitions(&s.stmt);
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

                let facts = self.extract_narrowing(condition);

                // Check then-body with positive narrowing
                let saved = self.variables.clone();
                for (var, fact) in &facts {
                    if let Some(current) = saved.get(var) {
                        let narrowed = Self::narrow_type(current, fact);
                        self.variables.insert(var.clone(), narrowed);
                    }
                }
                for s in then_body {
                    self.current_line = s.line;
                    self.check_stmt(&s.stmt);
                }
                self.variables = saved.clone();

                // Check else-body with inverted narrowing
                if let Some(else_b) = else_body {
                    for (var, fact) in &facts {
                        if let Some(current) = saved.get(var) {
                            let narrowed = Self::narrow_type(current, &fact.invert());
                            self.variables.insert(var.clone(), narrowed);
                        }
                    }
                    for s in else_b {
                        self.current_line = s.line;
                        self.check_stmt(&s.stmt);
                    }
                    self.variables = saved;
                } else if !facts.is_empty() && Self::body_always_returns(then_body) {
                    // Early return narrowing: if then-body always returns,
                    // apply inverted facts to the rest of the scope
                    for (var, fact) in &facts {
                        if let Some(current) = saved.get(var) {
                            let narrowed = Self::narrow_type(current, &fact.invert());
                            self.variables.insert(var.clone(), narrowed);
                        }
                    }
                }
            }
            Stmt::For { iterable, body, .. } => {
                self.infer_expr(iterable);
                for s in body {
                    self.current_line = s.line;
                    self.check_stmt(&s.stmt);
                }
            }
            Stmt::While { condition, body } => {
                self.infer_expr(condition);
                for s in body {
                    self.current_line = s.line;
                    self.check_stmt(&s.stmt);
                }
            }
            Stmt::Loop { body } | Stmt::Spawn { body } => {
                for s in body {
                    self.current_line = s.line;
                    self.check_stmt(&s.stmt);
                }
            }
            Stmt::Return(None) => {}
            Stmt::Expression(expr) => {
                self.infer_expr(expr);
            }
            Stmt::Match { subject, arms } => {
                let subject_type = self.infer_expr(subject);
                let subject_name = if let Expr::Ident(name) = subject {
                    Some(name.clone())
                } else {
                    None
                };

                for arm in arms {
                    let saved = self.variables.clone();

                    // Narrow the subject's type based on the match pattern
                    if let Some(ref var) = subject_name {
                        let narrowed = match &arm.pattern {
                            Pattern::Literal(expr) if is_null_expr(expr) => {
                                Some(Self::narrow_type(&subject_type, &NarrowingFact::IsNull))
                            }
                            Pattern::Constructor { name, .. } => match name.as_str() {
                                "Some" => {
                                    Some(Self::narrow_type(&subject_type, &NarrowingFact::NonNull))
                                }
                                "Ok" => {
                                    Some(Self::narrow_type(&subject_type, &NarrowingFact::IsOk))
                                }
                                "Err" => {
                                    Some(Self::narrow_type(&subject_type, &NarrowingFact::IsErr))
                                }
                                _ => None,
                            },
                            _ => None,
                        };
                        if let Some(t) = narrowed {
                            self.variables.insert(var.clone(), t);
                        }
                    }

                    for s in &arm.body {
                        self.current_line = s.line;
                        self.check_stmt(&s.stmt);
                    }
                    self.variables = saved;
                }

                // Exhaustiveness check for known types
                self.check_match_exhaustiveness(&subject_type, arms);
            }
            _ => {}
        }
    }

    /// Substitute generic type parameters with concrete types.
    /// Only replaces Named types that appear in `substitutions`.
    fn resolve_type(
        ty: &InferredType,
        substitutions: &HashMap<String, InferredType>,
    ) -> InferredType {
        match ty {
            InferredType::Named(name) => {
                if let Some(concrete) = substitutions.get(name) {
                    concrete.clone()
                } else {
                    ty.clone()
                }
            }
            InferredType::Array(inner) => {
                InferredType::Array(Box::new(Self::resolve_type(inner, substitutions)))
            }
            InferredType::Option(inner) => {
                InferredType::Option(Box::new(Self::resolve_type(inner, substitutions)))
            }
            InferredType::Result(ok, err) => InferredType::Result(
                Box::new(Self::resolve_type(ok, substitutions)),
                Box::new(Self::resolve_type(err, substitutions)),
            ),
            InferredType::Function(params, ret) => InferredType::Function(
                params
                    .iter()
                    .map(|p| Self::resolve_type(p, substitutions))
                    .collect(),
                Box::new(Self::resolve_type(ret, substitutions)),
            ),
            _ => ty.clone(),
        }
    }

    /// Build a substitution map from type params and argument types.
    fn build_substitutions(
        sig: &FnSignature,
        arg_types: &[InferredType],
    ) -> HashMap<String, InferredType> {
        let mut subs = HashMap::new();
        if sig.type_params.is_empty() {
            return subs;
        }
        for (i, (_, param_type)) in sig.params.iter().enumerate() {
            if let Some(expected) = param_type {
                if let Some(arg_type) = arg_types.get(i) {
                    if *arg_type != InferredType::Unknown {
                        Self::bind_type_params(&sig.type_params, expected, arg_type, &mut subs);
                    }
                }
            }
        }
        subs
    }

    /// Recursively bind type params by matching expected type structure against actual type.
    fn bind_type_params(
        type_params: &[String],
        expected: &InferredType,
        actual: &InferredType,
        subs: &mut HashMap<String, InferredType>,
    ) {
        match expected {
            InferredType::Named(name) if type_params.contains(name) => {
                subs.entry(name.clone()).or_insert_with(|| actual.clone());
            }
            InferredType::Array(inner_expected) => {
                if let InferredType::Array(inner_actual) = actual {
                    Self::bind_type_params(type_params, inner_expected, inner_actual, subs);
                }
            }
            InferredType::Option(inner_expected) => {
                if let InferredType::Option(inner_actual) = actual {
                    Self::bind_type_params(type_params, inner_expected, inner_actual, subs);
                }
            }
            InferredType::Result(ok_exp, err_exp) => {
                if let InferredType::Result(ok_act, err_act) = actual {
                    Self::bind_type_params(type_params, ok_exp, ok_act, subs);
                    Self::bind_type_params(type_params, err_exp, err_act, subs);
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
                if name == "null" {
                    return InferredType::Null;
                }
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
                // Warn when Option<T> is used in arithmetic/comparison (not == or !=)
                if !matches!(op, BinOp::Eq | BinOp::NotEq) {
                    let left_opt = matches!(lt, InferredType::Option(_));
                    let right_opt = matches!(rt, InferredType::Option(_));
                    if left_opt || right_opt {
                        self.emit(
                            "Option type used in operation; check with is_some() first".to_string(),
                        );
                    }
                }
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

                        // Infer all argument types first
                        let arg_types: Vec<InferredType> =
                            args.iter().map(|a| self.infer_expr(a)).collect();

                        // Build generic substitutions from arguments
                        let subs = Self::build_substitutions(&sig, &arg_types);

                        // Argument type check (with generic substitution)
                        for (i, arg_type) in arg_types.iter().enumerate() {
                            if let Some((_, Some(expected))) = sig.params.get(i) {
                                let resolved = if subs.is_empty() {
                                    expected.clone()
                                } else {
                                    Self::resolve_type(expected, &subs)
                                };
                                if *arg_type != InferredType::Unknown
                                    && !types_compatible(&resolved, arg_type)
                                {
                                    // Check interface satisfaction before emitting error
                                    if let (
                                        InferredType::Named(iface_name),
                                        InferredType::Named(struct_name),
                                    ) = (&resolved, arg_type)
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
                                        resolved,
                                        arg_type
                                    ));
                                }
                            }
                        }

                        // Resolve return type with generic substitutions
                        let ret = sig.return_type.unwrap_or(InferredType::Unknown);
                        return if subs.is_empty() {
                            ret
                        } else {
                            Self::resolve_type(&ret, &subs)
                        };
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
                        "unwrap" => {
                            if let Some(arg) = args.first() {
                                let arg_type = self.infer_expr(arg);
                                if let InferredType::Option(inner) = arg_type {
                                    return *inner;
                                }
                                return arg_type;
                            }
                            return InferredType::Unknown;
                        }
                        "unwrap_or" => {
                            if let Some(arg) = args.first() {
                                let arg_type = self.infer_expr(arg);
                                let fallback_type = if args.len() > 1 {
                                    self.infer_expr(&args[1])
                                } else {
                                    InferredType::Unknown
                                };
                                if let InferredType::Option(inner) = arg_type {
                                    if fallback_type != InferredType::Unknown
                                        && !types_compatible(&inner, &fallback_type)
                                    {
                                        self.emit(format!(
                                            "unwrap_or fallback type {} incompatible with Option inner type {}",
                                            fallback_type, inner
                                        ));
                                    }
                                    return *inner;
                                }
                                return arg_type;
                            }
                            return InferredType::Unknown;
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
                    self.current_line = s.line;
                    self.check_stmt(&s.stmt);
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
                    self.current_line = s.line;
                    self.check_stmt(&s.stmt);
                }
                InferredType::Unknown
            }

            Expr::Await(inner) | Expr::Must(inner) | Expr::Freeze(inner) | Expr::Ask(inner) => {
                self.infer_expr(inner)
            }

            Expr::Spawn(body) => {
                for s in body {
                    self.current_line = s.line;
                    self.check_stmt(&s.stmt);
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

    #[test]
    fn interface_impl_block_satisfaction() {
        let w = warnings_for(
            "interface Printable { fn display() -> String }\nstruct User { name: String }\nimpl User { fn display() -> String { return \"User\" } }\nfn show(p: Printable) { println(p) }\nlet u = User { name: \"Alice\" }\nshow(u)",
        );
        let interface_warnings: Vec<_> = w
            .iter()
            .filter(|w| {
                w.message.contains("satisfy")
                    || w.message.contains("parameter")
                    || w.message.contains("returns")
            })
            .collect();
        assert!(
            interface_warnings.is_empty(),
            "impl block method should satisfy interface: {:?}",
            interface_warnings
        );
    }

    #[test]
    fn interface_wrong_param_count() {
        let w = warnings_for(
            "interface Hasher { fn hash(data: String) -> Int }\nstruct MyHash { x: Int }\nimpl MyHash { fn hash() -> Int { return 0 } }\nfn do_hash(h: Hasher) { println(h) }\nlet m = MyHash { x: 1 }\ndo_hash(m)",
        );
        let param_warnings: Vec<_> = w
            .iter()
            .filter(|w| w.message.contains("parameter"))
            .collect();
        assert!(
            !param_warnings.is_empty(),
            "wrong param count should warn: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn interface_wrong_return_type() {
        let w = warnings_for(
            "interface Stringer { fn to_str() -> String }\nstruct Num { val: Int }\nimpl Num { fn to_str() -> Int { return 0 } }\nfn stringify(s: Stringer) { println(s) }\nlet n = Num { val: 1 }\nstringify(n)",
        );
        let ret_warnings: Vec<_> = w.iter().filter(|w| w.message.contains("returns")).collect();
        assert!(
            !ret_warnings.is_empty(),
            "wrong return type should warn: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn interface_multi_method() {
        let w = warnings_for(
            "interface ReadWrite { fn read() -> String\nfn write(data: String) }\nstruct File { path: String }\nfn process(rw: ReadWrite) { println(rw) }\nlet f = File { path: \"test\" }\nprocess(f)",
        );
        let missing_warnings: Vec<_> = w.iter().filter(|w| w.message.contains("missing")).collect();
        assert!(
            missing_warnings.len() >= 2,
            "should warn about both missing methods: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
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

    // ========== 8A.1: Return Type Inference ==========

    #[test]
    fn infers_return_type_from_explicit_return() {
        // add() returns Int (inferred), so assigning to String should warn
        let w = warnings_for("fn add(a: Int, b: Int) { return a + b }\nlet x: String = add(1, 2)");
        assert_eq!(w.len(), 1);
        assert!(
            w[0].message.contains("Int"),
            "expected Int mismatch, got: {}",
            w[0].message
        );
    }

    #[test]
    fn inferred_return_type_no_false_positive() {
        // add() returns Int (inferred), assigned to Int — no warning
        let w = warnings_for("fn add(a: Int, b: Int) { return a + b }\nlet x: Int = add(1, 2)");
        assert!(
            w.is_empty(),
            "should not warn: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn infers_string_return_type() {
        let w = warnings_for(
            "fn greet(name: String) { return \"hello \" + name }\nlet x: Int = greet(\"world\")",
        );
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("String"));
    }

    #[test]
    fn infers_null_for_no_return() {
        // Function with no return statements returns Null
        let w = warnings_for("fn noop() { let x = 1 }\nlet y: Int = noop()");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("Null"));
    }

    #[test]
    fn infers_from_multiple_consistent_returns() {
        let w = warnings_for(
            "fn abs_val(x: Int) {\n  if x > 0 { return x }\n  return 0 - x\n}\nlet y: String = abs_val(5)",
        );
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("Int"));
    }

    #[test]
    fn mixed_int_float_promotes_to_float() {
        let w = warnings_for(
            "fn mixed(x: Int) {\n  if x > 0 { return x }\n  return 1.5\n}\nlet y: String = mixed(5)",
        );
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("Float"));
    }

    #[test]
    fn incompatible_returns_stay_unknown() {
        // Int and String returns → Unknown, so no warning on caller
        let w = warnings_for(
            "fn weird(x: Int) {\n  if x > 0 { return x }\n  return \"negative\"\n}\nlet y: String = weird(5)",
        );
        // Unknown return type — no mismatch warning for caller
        assert!(w.is_empty());
    }

    #[test]
    fn implicit_last_expression_inferred() {
        // Last expression is the return value
        let w = warnings_for("fn double(x: Int) { x * 2 }\nlet y: String = double(5)");
        assert_eq!(w.len(), 1);
        assert!(w[0].message.contains("Int"));
    }

    #[test]
    fn forward_call_inference_works() {
        // caller defined before callee — pass 1.5 runs on all functions
        let w =
            warnings_for("fn caller() { return callee(5) }\nfn callee(x: Int) { return x * 2 }");
        // callee's return type is inferred as Int, so caller's return is also Int
        // No warnings expected (no type annotations to conflict)
        assert!(w.is_empty());
    }

    // ========== 8A.2: Flow-Sensitive Type Narrowing ==========

    #[test]
    fn narrowing_not_null_unwraps_option() {
        // x is ?String, but after `x != null` check it should be String
        let w = warnings_for("fn f(x: ?String) {\n  if x != null {\n    let y: String = x\n  }\n}");
        assert!(
            w.is_empty(),
            "should not warn when Option narrowed to inner type: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn narrowing_eq_null_narrows_to_null() {
        // In the then-branch of `x == null`, x is Null
        // In the else-branch, x should be non-null (String)
        let w = warnings_for(
            "fn f(x: ?String) {\n  if x == null {\n    let y: Null = x\n  } else {\n    let z: String = x\n  }\n}",
        );
        assert!(
            w.is_empty(),
            "should narrow to Null in then, String in else: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn narrowing_does_not_leak_scope() {
        // After the if-block (no early return), narrowing should not persist
        let w = warnings_for(
            "fn f(x: ?String) {\n  if x != null {\n    let y: String = x\n  }\n  let z: String = x\n}",
        );
        // The `let z: String = x` should warn because x is still ?String outside the if
        assert_eq!(
            w.len(),
            1,
            "narrowing should not leak: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn narrowing_negation() {
        // !(x == null) is the same as x != null
        let w =
            warnings_for("fn f(x: ?String) {\n  if !(x == null) {\n    let y: String = x\n  }\n}");
        assert!(
            w.is_empty(),
            "negation should invert narrowing: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn narrowing_and_chain() {
        // x != null && y != null should narrow both
        let w = warnings_for(
            "fn f(x: ?String, y: ?Int) {\n  if x != null && y != null {\n    let a: String = x\n    let b: Int = y\n  }\n}",
        );
        assert!(
            w.is_empty(),
            "AND chain should narrow both vars: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn narrowing_is_some() {
        let w =
            warnings_for("fn f(x: ?String) {\n  if is_some(x) {\n    let y: String = x\n  }\n}");
        assert!(
            w.is_empty(),
            "is_some should narrow Option: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn narrowing_is_ok() {
        let w = warnings_for(
            "fn f(x: Result<Int, String>) {\n  if is_ok(x) {\n    let y: Int = x\n  }\n}",
        );
        assert!(
            w.is_empty(),
            "is_ok should narrow Result to Ok type: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn narrowing_early_return() {
        // if x == null { return } → x is non-null after the if
        let w =
            warnings_for("fn f(x: ?String) {\n  if x == null { return }\n  let y: String = x\n}");
        assert!(
            w.is_empty(),
            "early return should narrow after if: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn narrowing_unknown_not_narrowed() {
        // Unknown types should stay Unknown (no narrowing)
        let w = warnings_for("fn f(x) {\n  if x != null {\n    let y: String = x\n  }\n}");
        // x is Unknown (no annotation), narrowing Unknown stays Unknown,
        // and Unknown is compatible with anything — no warning
        assert!(w.is_empty());
    }

    #[test]
    fn match_some_constructor_narrows() {
        // Match with Some(...) constructor pattern should narrow Option to inner type
        let w = warnings_for(
            "fn f(x: ?String) {\n  match x {\n    Some(v) => { let y: String = x }\n    _ => {}\n  }\n}",
        );
        assert!(
            w.is_empty(),
            "Some constructor should narrow Option: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn match_ok_constructor_narrows() {
        let w = warnings_for(
            "fn f(x: Result<Int, String>) {\n  match x {\n    Ok(v) => { let y: Int = x }\n    Err(e) => { let z: String = x }\n  }\n}",
        );
        assert!(
            w.is_empty(),
            "Ok/Err constructors should narrow Result: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    // ========== 8A.3: Exhaustive Match Checking ==========

    #[test]
    fn exhaustive_option_missing_none() {
        let w = warnings_for("fn f(x: ?String) {\n  match x {\n    Some(v) => { say v }\n  }\n}");
        assert_eq!(
            w.len(),
            1,
            "should warn about missing None: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
        assert!(
            w[0].message.contains("None"),
            "warning should mention None: {}",
            w[0].message
        );
    }

    #[test]
    fn exhaustive_option_complete() {
        let w = warnings_for(
            "fn f(x: ?String) {\n  match x {\n    Some(v) => { say v }\n    _ => { say \"none\" }\n  }\n}",
        );
        assert!(
            w.is_empty(),
            "complete Option match should not warn: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exhaustive_result_missing_err() {
        let w = warnings_for(
            "fn f(x: Result<Int, String>) {\n  match x {\n    Ok(v) => { say v }\n  }\n}",
        );
        assert_eq!(
            w.len(),
            1,
            "should warn about missing Err: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
        assert!(
            w[0].message.contains("Err"),
            "warning should mention Err: {}",
            w[0].message
        );
    }

    #[test]
    fn exhaustive_result_complete() {
        let w = warnings_for(
            "fn f(x: Result<Int, String>) {\n  match x {\n    Ok(v) => { say v }\n    Err(e) => { say e }\n  }\n}",
        );
        assert!(
            w.is_empty(),
            "complete Result match should not warn: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exhaustive_bool_missing_false() {
        let w = warnings_for("fn f(x: Bool) {\n  match x {\n    true => { say \"yes\" }\n  }\n}");
        assert_eq!(
            w.len(),
            1,
            "should warn about missing false: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
        assert!(
            w[0].message.contains("false"),
            "warning should mention false: {}",
            w[0].message
        );
    }

    #[test]
    fn exhaustive_wildcard_covers_all() {
        let w = warnings_for(
            "fn f(x: Result<Int, String>) {\n  match x {\n    _ => { say \"catch all\" }\n  }\n}",
        );
        assert!(
            w.is_empty(),
            "wildcard should make match exhaustive: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exhaustive_binding_covers_all() {
        let w = warnings_for("fn f(x: ?String) {\n  match x {\n    v => { say v }\n  }\n}");
        assert!(
            w.is_empty(),
            "binding should make match exhaustive: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exhaustive_unknown_no_warning() {
        // Unknown type — can't check exhaustiveness
        let w = warnings_for("fn f(x) {\n  match x {\n    1 => { say \"one\" }\n  }\n}");
        assert!(
            w.is_empty(),
            "unknown type should not trigger exhaustiveness: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exhaustive_int_no_warning() {
        // Int — can't check exhaustiveness
        let w = warnings_for("fn f(x: Int) {\n  match x {\n    1 => { say \"one\" }\n  }\n}");
        assert!(
            w.is_empty(),
            "Int should not trigger exhaustiveness: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    // ========== 8B.2: Generic Type Resolution ==========

    #[test]
    fn generic_identity_resolves_return_type() {
        // fn identity<T>(x: T) -> T: called with Int → return Int
        let w =
            warnings_for("fn identity<T>(x: T) -> T { return x }\nlet y: String = identity(42)");
        assert_eq!(
            w.len(),
            1,
            "should warn: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
        assert!(
            w[0].message.contains("Int"),
            "return type should resolve to Int: {}",
            w[0].message
        );
    }

    #[test]
    fn generic_identity_no_false_positive() {
        let w = warnings_for("fn identity<T>(x: T) -> T { return x }\nlet y: Int = identity(42)");
        assert!(
            w.is_empty(),
            "should not warn: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn generic_two_params_resolves() {
        // fn first<T, U>(a: T, b: U) -> T: called with (Int, String) → return Int
        let w = warnings_for(
            "fn first<T, U>(a: T, b: U) -> T { return a }\nlet y: String = first(42, \"hi\")",
        );
        assert_eq!(w.len(), 1);
        assert!(
            w[0].message.contains("Int"),
            "T should resolve to Int: {}",
            w[0].message
        );
    }

    #[test]
    fn generic_array_return_resolves() {
        // fn wrap<T>(x: T) -> [T]: called with Int → return [Int]
        let w = warnings_for("fn wrap<T>(x: T) -> [T] { return [x] }\nlet y: String = wrap(42)");
        assert_eq!(w.len(), 1);
        assert!(
            w[0].message.contains("[Int]"),
            "return should be [Int]: {}",
            w[0].message
        );
    }

    #[test]
    fn non_generic_unchanged() {
        // Non-generic function behavior unchanged
        let w =
            warnings_for("fn add(a: Int, b: Int) -> Int { return a + b }\nlet y: Int = add(1, 2)");
        assert!(w.is_empty());
    }

    // ========== 8B.3: Generic Struct Definitions ==========

    #[test]
    fn generic_struct_stores_type_params() {
        // Generic struct should parse and type-check without errors
        let w = warnings_for("struct Pair<T> {\n  first: T\n  second: T\n}");
        assert!(
            w.is_empty(),
            "generic struct def should not warn: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn non_generic_struct_still_works() {
        let w = warnings_for("struct Point {\n  x: Int\n  y: Int\n}");
        assert!(w.is_empty());
    }

    #[test]
    fn generic_struct_with_multiple_type_params() {
        let w = warnings_for("struct Either<L, R> {\n  left: L\n  right: R\n}");
        assert!(w.is_empty());
    }

    // ========== 8C.1: Union Types ==========

    #[test]
    fn union_type_accepts_member() {
        let w = warnings_for("type StringOrInt = String | Int\nlet x: StringOrInt = 42");
        assert!(
            w.is_empty(),
            "Int should be assignable to String|Int: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn union_type_accepts_other_member() {
        let w = warnings_for("type StringOrInt = String | Int\nlet x: StringOrInt = \"hello\"");
        assert!(
            w.is_empty(),
            "String should be assignable to String|Int: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn union_type_rejects_non_member() {
        let w = warnings_for("type StringOrInt = String | Int\nlet x: StringOrInt = true");
        assert_eq!(
            w.len(),
            1,
            "Bool should not be assignable to String|Int: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn union_type_nullable() {
        let w = warnings_for("type Nullable = String | Null\nlet x: Nullable = null");
        assert!(
            w.is_empty(),
            "null should be assignable to String|Null: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn single_variant_alias() {
        // type ID = Int — simple alias
        let w = warnings_for("type ID = Int\nlet x: ID = 42");
        assert!(
            w.is_empty(),
            "Int should be assignable to ID alias: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn single_variant_alias_rejects_wrong_type() {
        let w = warnings_for("type ID = Int\nlet x: ID = \"hello\"");
        assert_eq!(
            w.len(),
            1,
            "String should not be assignable to Int alias: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    // ========== 8C.3: Typed Collection Literals ==========

    #[test]
    fn typed_array_correct_elements() {
        let w = warnings_for("let xs: [Int] = [1, 2, 3]");
        assert!(
            w.is_empty(),
            "should not warn for correct array type: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn typed_array_wrong_elements() {
        let w = warnings_for("let xs: [Int] = [\"a\", \"b\"]");
        assert_eq!(
            w.len(),
            1,
            "should warn for wrong element type: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn typed_array_string_elements() {
        let w = warnings_for("let xs: [String] = [\"a\", \"b\"]");
        assert!(w.is_empty());
    }

    #[test]
    fn typed_array_empty_compatible() {
        let w = warnings_for("let xs: [Int] = []");
        assert!(
            w.is_empty(),
            "empty array should be compatible with any typed array: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    // ========== Option<T> Enforcement ==========

    #[test]
    fn unwrap_returns_inner_type() {
        let w = warnings_for("let x: ?Int = Some(42)\nlet y: Int = unwrap(x)");
        assert!(
            w.is_empty(),
            "unwrap(?Int) should return Int: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn unwrap_mismatch_warns() {
        let w = warnings_for("let x: ?Int = Some(42)\nlet y: String = unwrap(x)");
        assert!(!w.is_empty(), "unwrap(?Int) assigned to String should warn");
    }

    #[test]
    fn unwrap_or_returns_inner_type() {
        let w = warnings_for("let x: ?Int = Some(42)\nlet y: Int = unwrap_or(x, 0)");
        assert!(
            w.is_empty(),
            "unwrap_or(?Int, 0) should return Int: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn unwrap_or_fallback_type_mismatch_warns() {
        let w = warnings_for("let x: ?Int = Some(42)\nlet y = unwrap_or(x, \"hello\")");
        assert!(
            !w.is_empty(),
            "unwrap_or(?Int, String) should warn about incompatible fallback"
        );
        assert!(w[0].message.contains("incompatible"));
    }

    #[test]
    fn option_in_arithmetic_warns() {
        let w = warnings_for("let x: ?Int = Some(42)\nlet y = x + 1");
        assert!(!w.is_empty(), "Option in arithmetic should warn");
        assert!(w[0].message.contains("Option"));
    }

    #[test]
    fn option_in_equality_no_warn() {
        let w = warnings_for("let x: ?Int = Some(42)\nlet y = x == null");
        assert!(
            w.is_empty(),
            "Option in == should not warn: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn option_in_not_equal_no_warn() {
        let w = warnings_for("let x: ?Int = Some(42)\nlet y = x != None");
        assert!(
            w.is_empty(),
            "Option in != should not warn: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn reassign_none_to_non_option_warns() {
        let w = warnings_for("let mut x: Int = 5\nx = None");
        assert!(!w.is_empty(), "assigning None to Int variable should warn");
    }

    #[test]
    fn reassign_none_to_option_ok() {
        let w = warnings_for("let mut x: ?Int = Some(5)\nx = None");
        assert!(
            w.is_empty(),
            "assigning None to ?Int should be fine: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn option_generic_syntax_parses() {
        let w = warnings_for("let x: Option<Int> = Some(42)");
        assert!(
            w.is_empty(),
            "Option<Int> syntax should work: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn narrowed_option_no_arithmetic_warn() {
        let w = warnings_for("fn f(x: ?Int) {\n  if is_some(x) {\n    let y = x + 1\n  }\n}");
        assert!(
            w.is_empty(),
            "narrowed Option should not warn in arithmetic: {:?}",
            w.iter().map(|w| &w.message).collect::<Vec<_>>()
        );
    }
}
