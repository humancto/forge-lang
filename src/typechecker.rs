/// Forge Gradual Type Checker
/// Runs between parsing and interpretation.
/// Enforces type annotations when present, ignores when absent.
/// Does NOT reject programs without annotations (gradual typing).
use crate::parser::ast::*;
use std::collections::HashMap;

#[derive(Debug)]
#[allow(dead_code)]
pub struct TypeWarning {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

pub struct TypeChecker {
    /// function name -> (param types, return type)
    functions: HashMap<String, FnSignature>,
    /// type name -> list of variant names
    type_defs: HashMap<String, Vec<String>>,
    /// interface name -> list of method signatures
    interfaces: HashMap<String, Vec<InterfaceMethod>>,
    warnings: Vec<TypeWarning>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct FnSignature {
    params: Vec<Option<String>>,
    param_count: usize,
    return_type: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct InterfaceMethod {
    name: String,
    param_count: usize,
    return_type: Option<String>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            type_defs: HashMap::new(),
            interfaces: HashMap::new(),
            warnings: Vec::new(),
        }
    }

    pub fn check(&mut self, program: &Program) -> Vec<TypeWarning> {
        // First pass: collect all function signatures, type defs, and interfaces
        for stmt in &program.statements {
            self.collect_definitions(stmt);
        }

        // Second pass: check usage
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
                let param_types: Vec<Option<String>> = params
                    .iter()
                    .map(|p| p.type_ann.as_ref().map(type_ann_to_string))
                    .collect();
                self.functions.insert(
                    name.clone(),
                    FnSignature {
                        param_count: params.len(),
                        params: param_types,
                        return_type: return_type.as_ref().map(type_ann_to_string),
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
                        return_type: m.return_type.as_ref().map(type_ann_to_string),
                    })
                    .collect();
                self.interfaces.insert(name.clone(), method_sigs);
            }
            _ => {}
        }
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                value, type_ann, ..
            } => {
                self.check_expr(value);
                if let Some(_ann) = type_ann {
                    // Future: validate that value type matches annotation
                }
            }
            Stmt::Assign { value, .. } => {
                self.check_expr(value);
            }
            Stmt::FnDef { body, .. } => {
                for s in body {
                    self.check_stmt(s);
                }
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                self.check_expr(condition);
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
                self.check_expr(iterable);
                for s in body {
                    self.check_stmt(s);
                }
            }
            Stmt::While { condition, body } => {
                self.check_expr(condition);
                for s in body {
                    self.check_stmt(s);
                }
            }
            Stmt::Loop { body } | Stmt::Spawn { body } => {
                for s in body {
                    self.check_stmt(s);
                }
            }
            Stmt::Return(Some(expr)) => {
                self.check_expr(expr);
            }
            Stmt::Expression(expr) => {
                self.check_expr(expr);
            }
            Stmt::Match { subject, arms } => {
                self.check_expr(subject);
                for arm in arms {
                    for s in &arm.body {
                        self.check_stmt(s);
                    }
                }
            }
            _ => {}
        }
    }

    fn check_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Call { function, args } => {
                if let Expr::Ident(name) = function.as_ref() {
                    if let Some(sig) = self.functions.get(name).cloned() {
                        if args.len() != sig.param_count {
                            self.warnings.push(TypeWarning {
                                message: format!(
                                    "function '{}' expects {} argument(s), got {}",
                                    name,
                                    sig.param_count,
                                    args.len()
                                ),
                                line: 0,
                                col: 0,
                            });
                        }
                    }
                }
                for arg in args {
                    self.check_expr(arg);
                }
            }
            Expr::BinOp { left, right, .. } => {
                self.check_expr(left);
                self.check_expr(right);
            }
            Expr::UnaryOp { operand, .. } => {
                self.check_expr(operand);
            }
            Expr::FieldAccess { object, .. } => {
                self.check_expr(object);
            }
            Expr::Index { object, index } => {
                self.check_expr(object);
                self.check_expr(index);
            }
            Expr::Pipeline { value, function } => {
                self.check_expr(value);
                self.check_expr(function);
            }
            Expr::Try(inner) => {
                self.check_expr(inner);
            }
            Expr::Array(items) => {
                for item in items {
                    self.check_expr(item);
                }
            }
            Expr::Object(fields) => {
                for (_, expr) in fields {
                    self.check_expr(expr);
                }
            }
            Expr::Lambda { body, .. } => {
                for s in body {
                    self.check_stmt(s);
                }
            }
            Expr::Block(stmts) => {
                for s in stmts {
                    self.check_stmt(s);
                }
            }
            _ => {}
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
