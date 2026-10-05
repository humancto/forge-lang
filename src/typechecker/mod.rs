//! Forge's static type checker.
//!
//! Runs between parsing and execution. It is *gradual*: unannotated code is
//! inferred where possible and never rejected for lack of annotations, and
//! everything it cannot determine is `Any`. What it reports:
//!
//! * mismatches against declared types (variables, parameters, returns,
//!   struct fields), with generics solved per call;
//! * errors the engines would raise at run time and that are certain from
//!   the types alone: invalid operators (`"a" - 1`, via the same
//!   `semantics::binary` rules), wrong argument counts, unknown struct
//!   fields and module members, calls of non-functions, assignments to
//!   immutable bindings;
//! * unknown names and types (with "did you mean" fixes), names used
//!   before their definition runs, unreachable code, missing returns and
//!   non-exhaustive matches.
//!
//! Every diagnostic has a stable code (`T0001`...; see [`Code`]). By
//! default all of them are warnings; `--strict` makes them errors.
//!
//! Entry point: [`analyze`] (source text → full [`Analysis`]), used by the
//! CLI, the LSP and `forge mcp`.

pub mod builtins;
pub mod diagnostics;
pub mod enforce;
mod infer;
pub mod resolve;
pub mod suggest;
pub mod types;

#[cfg(test)]
mod corpus_tests;
#[cfg(test)]
mod tests;

pub use diagnostics::{Code, Diagnostic, Fix, Severity};
pub use infer::TypeFacts;

use crate::parser::ast::{Program, Stmt};
use crate::parser::index::{DefKind, Role, Span, SyntaxIndex};
use resolve::{ImportedNames, Resolution, Resolved};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct CheckOptions {
    /// Report every diagnostic as an error.
    pub strict: bool,
    /// The file being checked; imports are resolved relative to it.
    pub file: Option<PathBuf>,
}

/// A failure before type checking could start.
#[derive(Debug, Clone)]
pub enum FrontendError {
    Lex {
        line: usize,
        col: usize,
        message: String,
    },
    Parse {
        line: usize,
        col: usize,
        message: String,
    },
}

/// One `import` statement and what it resolved to.
#[derive(Debug, Clone)]
struct ImportInfo {
    path: String,
    /// `Some` for `import { a, b } from "path"`.
    names: Option<Vec<String>>,
    resolved: Option<PathBuf>,
}

/// Everything known about one source file after checking.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub program: Program,
    pub index: SyntaxIndex,
    pub resolution: Resolution,
    /// Wildcard imports, in the order of `Resolved::Imported` indices.
    pub wildcard_imports: Vec<ImportedNames>,
    pub diagnostics: Vec<Diagnostic>,
    pub facts: TypeFacts,
}

/// Lex, parse (with a syntax index) and check `source`.
pub fn analyze(source: &str, options: &CheckOptions) -> Result<Analysis, FrontendError> {
    let tokens = crate::lexer::Lexer::new(source)
        .tokenize()
        .map_err(|e| FrontendError::Lex {
            line: e.line,
            col: e.col,
            message: e.message,
        })?;
    let mut parser = crate::parser::Parser::with_index(tokens, source);
    let program = parser.parse_program().map_err(|e| FrontendError::Parse {
        line: e.line,
        col: e.col,
        message: e.message,
    })?;
    let index = parser.take_index().unwrap_or_default();
    Ok(analyze_parsed(program, index, options))
}

/// Check a program parsed with a syntax index.
pub fn analyze_parsed(program: Program, index: SyntaxIndex, options: &CheckOptions) -> Analysis {
    let base_dir = options
        .file
        .as_deref()
        .and_then(Path::parent)
        .map(Path::to_path_buf);
    let imports = collect_imports(&program, base_dir.as_deref());
    let loaded: Vec<Option<Program>> = imports
        .iter()
        .map(|i| i.resolved.as_deref().and_then(load_module))
        .collect();

    let wildcard_imports: Vec<ImportedNames> = imports
        .iter()
        .zip(&loaded)
        .filter(|(i, _)| i.names.is_none() && !is_builtin_module(&i.path))
        .map(|(i, program)| ImportedNames {
            path: i.path.clone(),
            names: program.as_ref().map(exported_names),
        })
        .collect();

    let resolution = resolve::resolve(&index, &wildcard_imports);

    let mut checker = infer::Checker::new(options.strict, Some(&index), Some(&resolution));
    for (info, program) in imports.iter().zip(&loaded) {
        if let Some(program) = program {
            checker.import_declarations(program, info.names.as_deref());
        }
    }
    checker.check_program(&program);
    let (mut diagnostics, facts, suppressed) = checker.into_results();

    diagnostics.extend(name_diagnostics(
        &index,
        &resolution,
        &wildcard_imports,
        options.strict,
    ));
    diagnostics.retain(|d| !suppressed.iter().any(|r| within(d.span, *r)));
    sort_diagnostics(&mut diagnostics);

    Analysis {
        program,
        index,
        resolution,
        wildcard_imports,
        diagnostics,
        facts,
    }
}

fn within(span: Span, region: Span) -> bool {
    span.start.line > 0 && region.start <= span.start && span.start <= region.end
}

fn sort_diagnostics(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|a, b| {
        (a.span.start, a.code, &a.message).cmp(&(b.span.start, b.code, &b.message))
    });
}

fn is_builtin_module(path: &str) -> bool {
    crate::semantics::BUILTIN_MODULES.contains(&path)
}

fn collect_imports(program: &Program, base_dir: Option<&Path>) -> Vec<ImportInfo> {
    program
        .statements
        .iter()
        .filter_map(|s| match &s.stmt {
            Stmt::Import { path, names } => Some(ImportInfo {
                path: path.clone(),
                names: names.clone(),
                resolved: if is_builtin_module(path) {
                    None
                } else {
                    crate::package::resolve_import_from(path, base_dir)
                },
            }),
            _ => None,
        })
        .collect()
}

/// Parse an imported module (no checking: it is checked on its own).
fn load_module(path: &Path) -> Option<Program> {
    let source = std::fs::read_to_string(path).ok()?;
    let tokens = crate::lexer::Lexer::new(&source).tokenize().ok()?;
    crate::parser::Parser::new(tokens).parse_program().ok()
}

/// Names a wildcard `import "file"` brings into scope (see the
/// interpreter's `Stmt::Import`): top-level functions, variables, structs
/// and the variants of `type` definitions (plus the type names, usable in
/// annotations).
pub fn exported_names(program: &Program) -> HashSet<String> {
    let mut names = HashSet::new();
    for s in &program.statements {
        match &s.stmt {
            Stmt::FnDef { name, .. } | Stmt::Let { name, .. } | Stmt::StructDef { name, .. } => {
                names.insert(name.clone());
            }
            Stmt::TypeDef { name, variants } => {
                names.insert(name.clone());
                for v in variants {
                    names.insert(v.name.clone());
                }
            }
            Stmt::InterfaceDef { name, .. } => {
                names.insert(name.clone());
            }
            _ => {}
        }
    }
    names
}

/// Unknown names and types, and names used before their definition.
fn name_diagnostics(
    index: &SyntaxIndex,
    resolution: &Resolution,
    imports: &[ImportedNames],
    strict: bool,
) -> Vec<Diagnostic> {
    // A wildcard import we could not read might define anything.
    if imports.iter().any(|i| i.names.is_none()) {
        return Vec::new();
    }
    let severity = if strict {
        Severity::Error
    } else {
        Severity::Warning
    };
    let mut out = Vec::new();
    let mut seen_unknown: HashSet<(String, usize)> = HashSet::new();
    for (id, occ) in index.occurrences.iter().enumerate() {
        match (&resolution.resolved[id], &occ.role) {
            (Resolved::Unresolved, Role::TypeRef) | (Resolved::Unresolved, Role::Def(_)) => {
                // Alias members that are not types (`type T = Foo | Int`
                // with no `Foo`) are unknown types too.
                let candidates = resolve::visible_type_names(index, imports);
                let suggestion = suggest::closest(&occ.name, candidates.iter().map(String::as_str));
                out.push(Diagnostic {
                    code: Code::UnknownType,
                    severity,
                    message: format!("unknown type '{}'", occ.name),
                    span: occ.span,
                    help: suggestion.map(|s| format!("did you mean '{}'?", s)),
                    fixes: suggestion
                        .map(|s| Fix {
                            title: format!("Change to '{}'", s),
                            span: occ.span,
                            replacement: s.to_string(),
                        })
                        .into_iter()
                        .collect(),
                });
            }
            (Resolved::Unresolved, Role::Ref) => {
                // One diagnostic per name per scope is enough.
                if !seen_unknown.insert((occ.name.clone(), occ.scope)) {
                    continue;
                }
                let candidates = resolve::visible_value_names(index, occ.scope, imports);
                let suggestion = suggest::closest(&occ.name, candidates.iter().map(String::as_str));
                out.push(Diagnostic {
                    code: Code::UnknownName,
                    severity,
                    message: format!("unknown name '{}'", occ.name),
                    span: occ.span,
                    help: Some(match suggestion {
                        Some(s) => format!("did you mean '{}'?", s),
                        None => "define it with `let` (or `fn`) before using it".to_string(),
                    }),
                    fixes: suggestion
                        .map(|s| Fix {
                            title: format!("Change to '{}'", s),
                            span: occ.span,
                            replacement: s.to_string(),
                        })
                        .into_iter()
                        .collect(),
                });
            }
            (Resolved::Local(def), Role::Ref) if resolution.before_definition.contains(&id) => {
                let def_occ = &index.occurrences[*def];
                let what = match def_occ.def_kind() {
                    Some(DefKind::Function) => "function",
                    _ => "variable",
                };
                out.push(Diagnostic {
                    code: Code::UseBeforeDefinition,
                    severity,
                    message: format!(
                        "{} '{}' is used before it is defined (line {})",
                        what, occ.name, def_occ.span.start.line
                    ),
                    span: occ.span,
                    help: Some(format!(
                        "move the definition of '{}' above this use",
                        occ.name
                    )),
                    fixes: Vec::new(),
                });
            }
            _ => {}
        }
    }
    out
}
