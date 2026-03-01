use std::path::{Path, PathBuf};

use crate::lexer::Lexer;
use crate::parser::ast::{self, *};
use crate::parser::Parser;

pub fn generate_docs(paths: &[PathBuf]) {
    let files = if paths.is_empty() {
        collect_fg_files(Path::new("."))
    } else {
        let mut all = Vec::new();
        for path in paths {
            if path.is_dir() {
                all.extend(collect_fg_files(path));
            } else if path.extension().map(|e| e == "fg").unwrap_or(false) {
                all.push(path.clone());
            }
        }
        all
    };

    if files.is_empty() {
        println!("  No .fg files found.");
        return;
    }

    println!();
    println!("  \x1B[1mForge Documentation\x1B[0m");
    println!("  {}", "=".repeat(50));

    for file in &files {
        document_file(file);
    }

    println!();
    println!(
        "  \x1B[90mGenerated from {} file{}\x1B[0m",
        files.len(),
        if files.len() == 1 { "" } else { "s" }
    );
}

fn collect_fg_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "fg").unwrap_or(false) {
                files.push(path);
            } else if path.is_dir() {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if !name.starts_with('.') && name != "forge_modules" && name != "target" {
                    files.extend(collect_fg_files(&path));
                }
            }
        }
    }
    files.sort();
    files
}

fn document_file(file: &Path) {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(_) => return,
    };

    let mut lexer = Lexer::new(&source);
    let tokens = match lexer.tokenize() {
        Ok(t) => t,
        Err(_) => return,
    };

    let mut parser = Parser::new(tokens);
    let program = match parser.parse_program() {
        Ok(p) => p,
        Err(_) => return,
    };

    let lines: Vec<&str> = source.lines().collect();
    let entries = extract_docs(&program, &lines);

    if entries.is_empty() {
        return;
    }

    println!();
    println!("  \x1B[1;34m## {}\x1B[0m", file.display());

    for entry in &entries {
        println!();
        match &entry.kind {
            DocKind::Function {
                name,
                params,
                return_type,
            } => {
                let params_str = params
                    .iter()
                    .map(|p| {
                        if p.type_ann.is_empty() {
                            p.name.clone()
                        } else {
                            format!("{}: {}", p.name, p.type_ann)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let ret = if return_type.is_empty() {
                    String::new()
                } else {
                    format!(" -> {}", return_type)
                };
                println!(
                    "  \x1B[33mfn\x1B[0m \x1B[1m{}\x1B[0m({}){}",
                    name, params_str, ret
                );
            }
            DocKind::Variable { name, mutable } => {
                let prefix = if *mutable { "let mut" } else { "let" };
                println!("  \x1B[33m{}\x1B[0m \x1B[1m{}\x1B[0m", prefix, name);
            }
            DocKind::Struct { name, fields } => {
                println!(
                    "  \x1B[33mstruct\x1B[0m \x1B[1m{}\x1B[0m {{ {} }}",
                    name,
                    fields.join(", ")
                );
            }
        }

        for comment in &entry.comments {
            println!("    \x1B[90m{}\x1B[0m", comment);
        }

        if !entry.decorators.is_empty() {
            for dec in &entry.decorators {
                println!("    \x1B[36m@{}\x1B[0m", dec);
            }
        }
    }
}

struct DocEntry {
    kind: DocKind,
    comments: Vec<String>,
    decorators: Vec<String>,
}

enum DocKind {
    Function {
        name: String,
        params: Vec<ParamDoc>,
        return_type: String,
    },
    Variable {
        name: String,
        mutable: bool,
    },
    Struct {
        name: String,
        fields: Vec<String>,
    },
}

struct ParamDoc {
    name: String,
    type_ann: String,
}

fn extract_docs(program: &Program, lines: &[&str]) -> Vec<DocEntry> {
    let mut entries = Vec::new();

    for stmt in &program.statements {
        match stmt {
            Stmt::FnDef {
                name,
                params,
                decorators,
                return_type,
                ..
            } => {
                let comments = extract_preceding_comments(lines, stmt);
                let param_docs: Vec<ParamDoc> = params
                    .iter()
                    .map(|p| ParamDoc {
                        name: p.name.clone(),
                        type_ann: p.type_ann.as_ref().map(format_type_ann).unwrap_or_default(),
                    })
                    .collect();
                let ret = return_type
                    .as_ref()
                    .map(format_type_ann)
                    .unwrap_or_default();
                let dec_names: Vec<String> = decorators
                    .iter()
                    .map(|d| {
                        let args_str = d
                            .args
                            .iter()
                            .map(|a| match a {
                                DecoratorArg::Positional(e) => format_expr(e),
                                DecoratorArg::Named(k, v) => format!("{}: {}", k, format_expr(v)),
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        if args_str.is_empty() {
                            d.name.clone()
                        } else {
                            format!("{}({})", d.name, args_str)
                        }
                    })
                    .collect();

                entries.push(DocEntry {
                    kind: DocKind::Function {
                        name: name.clone(),
                        params: param_docs,
                        return_type: ret,
                    },
                    comments,
                    decorators: dec_names,
                });
            }
            Stmt::StructDef { name, fields, .. } => {
                let comments = extract_preceding_comments(lines, stmt);
                let field_names: Vec<String> = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, format_type_ann(&f.type_ann)))
                    .collect();
                entries.push(DocEntry {
                    kind: DocKind::Struct {
                        name: name.clone(),
                        fields: field_names,
                    },
                    comments,
                    decorators: Vec::new(),
                });
            }
            _ => {}
        }
    }

    entries
}

fn extract_preceding_comments(_lines: &[&str], _stmt: &Stmt) -> Vec<String> {
    // AST doesn't carry line info for statements yet — return empty
    Vec::new()
}

fn format_type_ann(t: &TypeAnn) -> String {
    match t {
        TypeAnn::Simple(s) => s.clone(),
        TypeAnn::Array(inner) => format!("Array<{}>", format_type_ann(inner)),
        TypeAnn::Generic(name, args) => {
            let args_str = args
                .iter()
                .map(format_type_ann)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", name, args_str)
        }
        TypeAnn::Function(params, ret) => {
            let params_str = params
                .iter()
                .map(format_type_ann)
                .collect::<Vec<_>>()
                .join(", ");
            format!("fn({}) -> {}", params_str, format_type_ann(ret))
        }
        _ => format!("{:?}", t),
    }
}

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::StringLit(s) => format!("\"{}\"", s),
        Expr::Int(n) => n.to_string(),
        Expr::Float(f) => f.to_string(),
        Expr::Bool(b) => b.to_string(),
        Expr::Ident(s) => s.clone(),
        _ => "<expr>".to_string(),
    }
}
