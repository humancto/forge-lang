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
//! Every diagnostic has a stable code (`T0001`...; see [`Code`]). In
//! edition 2026 all of them are warnings by default; `--strict` makes them
//! errors. From edition 2027, a contradiction of a *declared* type
//! (`Code::contradicts_declaration`: T0001-T0003, T0012, T0015, T0016) is
//! an error by default, and annotated function boundaries are checked at
//! run time too (`enforce`); diagnostics resting on inferred types stay
//! warnings unless `--strict` ([`SeverityPolicy`]).
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

pub use diagnostics::{Basis, Code, Diagnostic, Fix, SeverityPolicy};
pub use infer::TypeFacts;

use crate::parser::ast::{Program, Stmt};
use crate::parser::index::{DefKind, Role, Span, SyntaxIndex};
use crate::semantics::edition::Edition;
use resolve::{ImportedNames, Resolution, Resolved};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct CheckOptions {
    /// Report every diagnostic as an error.
    pub strict: bool,
    /// The file being checked; imports are resolved relative to it.
    pub file: Option<PathBuf>,
    /// The edition whose rules grade the diagnostics; `None`: the edition
    /// in effect on this thread (`semantics::edition::current`).
    pub edition: Option<Edition>,
    /// Do not read imported files: their declarations are unknown (`Any`).
    /// For hosts that must not touch the file system outside a run's
    /// permission policy (`Sandbox`).
    pub skip_imports: bool,
}

impl CheckOptions {
    pub fn policy(&self) -> SeverityPolicy {
        SeverityPolicy::new(
            self.strict,
            self.edition
                .unwrap_or_else(crate::semantics::edition::current),
        )
    }
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
    /// `Some` for `import { a, b as c } from "path"`.
    names: Option<Vec<crate::parser::ast::ImportName>>,
    /// `Some(m)` for `import "path" as m`.
    namespace: Option<String>,
    resolved: Option<PathBuf>,
    /// Position of the `import` statement.
    line: usize,
    col: usize,
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

/// Why [`prepare`] refused a program.
#[derive(Debug, Clone)]
pub enum PrepareError {
    Frontend(FrontendError),
    /// The checker found errors (all diagnostics, warnings included, in
    /// checking order).
    Type(Vec<Diagnostic>),
}

/// The front end of every surface that *runs* code (`forge run`/`test`/
/// `-e`, the REPL, `Sandbox`): check `source`, refuse it when a diagnostic
/// is an error under `options.policy()` (`--strict`, or a contradiction of
/// a declared type from edition 2027), and instrument annotated function
/// boundaries for run-time checks when the policy enforces them
/// ([`enforce`]). Returns the program to run and the (warning)
/// diagnostics to show.
pub fn prepare(
    source: &str,
    options: &CheckOptions,
) -> Result<(Program, Vec<Diagnostic>), PrepareError> {
    let policy = options.policy();
    let options = CheckOptions {
        edition: Some(policy.edition),
        ..options.clone()
    };
    let analysis = analyze(source, &options).map_err(PrepareError::Frontend)?;
    let diagnostics = analysis.diagnostics;
    if diagnostics.iter().any(|d| d.is_error()) {
        return Err(PrepareError::Type(diagnostics));
    }
    let mut program = analysis.program;
    if policy.enforces_at_run_time() {
        enforce::instrument(&mut program);
    }
    Ok((program, diagnostics))
}

/// Lex and parse an imported module (`path` as written in the `import`,
/// for messages) for both engines. From edition 2027 the module gets the
/// same treatment as the entry program ([`prepare`]): a contradiction of a
/// declared type refuses the import, and annotated boundaries are checked
/// at run time. The check never reads other files (`skip_imports`): the
/// module's own imports are prepared when they are loaded, under the
/// run's permission policy. Edition 2026 parses exactly as before.
pub fn prepare_module(source: &str, path: &str, file: &Path) -> Result<Program, String> {
    let options = CheckOptions {
        file: Some(file.to_path_buf()),
        skip_imports: true,
        ..Default::default()
    };
    if !options.policy().enforces_at_run_time() {
        let tokens = crate::lexer::Lexer::new(source)
            .tokenize()
            .map_err(|e| format!("import '{}' lex error: {}", path, e.message))?;
        return crate::parser::Parser::new(tokens)
            .parse_program()
            .map_err(|e| format!("import '{}' parse error: {}", path, e.message));
    }
    match prepare(source, &options) {
        Ok((program, _warnings)) => Ok(program),
        Err(PrepareError::Frontend(FrontendError::Lex { message, .. })) => {
            Err(format!("import '{}' lex error: {}", path, message))
        }
        Err(PrepareError::Frontend(FrontendError::Parse { message, .. })) => {
            Err(format!("import '{}' parse error: {}", path, message))
        }
        Err(PrepareError::Type(diagnostics)) => {
            let errors: Vec<String> = diagnostics
                .iter()
                .filter(|d| d.is_error())
                .map(|d| format!("{}:{}: [{}] {}", path, d.line(), d.code, d.message))
                .collect();
            Err(format!(
                "import '{}' type error: {}",
                path,
                errors.join("; ")
            ))
        }
    }
}

/// The closing line of a run refused by type errors (`forge run`), naming
/// the rule that made them errors.
pub fn refusal_summary(errors: usize, policy: SeverityPolicy) -> String {
    format!(
        "type checking failed with {} error{} ({})",
        errors,
        if errors == 1 { "" } else { "s" },
        if policy.strict {
            "--strict".to_string()
        } else {
            format!("edition {}: declared types are enforced", policy.edition)
        }
    )
}

/// Check a program parsed with a syntax index.
pub fn analyze_parsed(program: Program, index: SyntaxIndex, options: &CheckOptions) -> Analysis {
    let base_dir = options
        .file
        .as_deref()
        .and_then(Path::parent)
        .map(Path::to_path_buf);
    let imports = collect_imports(&program, base_dir.as_deref(), options.skip_imports);
    let loaded: Vec<Option<Program>> = imports
        .iter()
        .map(|i| i.resolved.as_deref().and_then(load_module))
        .collect();

    let wildcard_imports: Vec<ImportedNames> = imports
        .iter()
        .zip(&loaded)
        .filter(|(i, _)| i.names.is_none() && i.namespace.is_none() && !is_builtin_module(&i.path))
        .map(|(i, program)| ImportedNames {
            path: i.path.clone(),
            names: program.as_ref().map(exported_names),
        })
        .collect();

    let mut index = index;
    apply_hoisting(&program, &mut index);
    let resolution = resolve::resolve(&index, &wildcard_imports);

    let policy = options.policy();
    let mut checker = infer::Checker::new(policy, Some(&index), Some(&resolution));
    checker.unknown_wildcard = wildcard_imports.iter().any(|i| i.names.is_none());
    for (info, program) in imports.iter().zip(&loaded) {
        if let Some(name) = &info.namespace {
            checker.import_namespace(name, &info.path, program.as_ref());
        } else if let Some(program) = program {
            checker.import_declarations(program, info.names.as_deref());
        }
    }
    checker.check_program(&program);
    let (mut diagnostics, facts, suppressed) = checker.into_results();

    diagnostics.extend(name_diagnostics(
        &index,
        &resolution,
        &wildcard_imports,
        policy,
    ));
    diagnostics.retain(|d| !suppressed.iter().any(|r| within(d.span, *r)));
    if !options.skip_imports {
        diagnostics.extend(import_diagnostics(
            &imports,
            &loaded,
            base_dir.as_deref(),
            policy,
        ));
        diagnostics.extend(import_rename_diagnostics(&imports, &loaded, &index, policy));
    }
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

/// Top-level functions run before the statements written above them
/// (`semantics::hoist`, the plan both engines use): make each hoisted
/// definition visible from the first statement it now runs before, so a
/// call above it is not reported as a use before definition (T0018).
fn apply_hoisting(program: &Program, index: &mut SyntaxIndex) {
    let plan =
        crate::semantics::hoist::plan(&program.statements, &crate::semantics::hoist::is_predefined);
    for &(moved, before) in &plan.moved {
        let (Stmt::FnDef { name, .. }, Some(target)) = (
            &program.statements[moved].stmt,
            program.statements.get(before),
        ) else {
            continue;
        };
        let visible_from = crate::parser::index::Pos::new(target.line, target.col);
        // A hoisted name is bound once in the file, so its definition is
        // the only file-scope function definition with that name.
        for occ in index.occurrences.iter_mut() {
            if occ.name == *name
                && matches!(occ.def_kind(), Some(DefKind::Function))
                && occ.scope == 0
                && occ.visible_from > visible_from
            {
                occ.visible_from = visible_from;
            }
        }
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

fn collect_imports(program: &Program, base_dir: Option<&Path>, skip: bool) -> Vec<ImportInfo> {
    program
        .statements
        .iter()
        .filter_map(|s| match &s.stmt {
            Stmt::Import { path, names } => Some(ImportInfo {
                path: path.clone(),
                names: names.clone(),
                namespace: None,
                line: s.line,
                col: s.col,
                resolved: if skip || is_builtin_module(path) {
                    None
                } else {
                    crate::package::resolve_import_from(path, base_dir)
                },
            }),
            Stmt::ImportNamespace { path, name } => Some(ImportInfo {
                path: path.clone(),
                names: None,
                namespace: Some(name.clone()),
                line: s.line,
                col: s.col,
                resolved: if skip || is_builtin_module(path) {
                    None
                } else {
                    crate::package::resolve_import_from(path, base_dir)
                },
            }),
            _ => None,
        })
        .collect()
}

/// T0020 for each file import that cannot be found, read or parsed. Its
/// names stay unknown (`Any`, never a builtin of the same name), so the
/// import is the only diagnostic about it.
fn import_diagnostics(
    imports: &[ImportInfo],
    loaded: &[Option<Program>],
    base_dir: Option<&Path>,
    policy: SeverityPolicy,
) -> Vec<Diagnostic> {
    imports
        .iter()
        .zip(loaded)
        .filter(|(i, program)| !is_builtin_module(&i.path) && program.is_none())
        .map(|(i, _)| {
            let start = crate::parser::index::Pos::new(i.line, i.col.max(1));
            let end = crate::parser::index::Pos::new(i.line, i.col.max(1) + "import".len());
            let (message, help) = match &i.resolved {
                None => {
                    let tried = base_dir.map_or_else(
                        || PathBuf::from(&i.path),
                        |dir| dir.join(&i.path),
                    );
                    (
                        format!(
                            "cannot import '{}': file not found (looked for {})",
                            i.path,
                            tried.display()
                        ),
                        "paths are resolved relative to the importing file, then the working directory, then forge_modules/ (the .fg suffix is optional)".to_string(),
                    )
                }
                Some(found) => (
                    format!(
                        "cannot import '{}': {} cannot be read or does not parse",
                        i.path,
                        found.display()
                    ),
                    format!("run `forge check {}` to see the problem", found.display()),
                ),
            };
            Diagnostic {
                code: Code::UnresolvedImport,
                severity: policy.severity(Code::UnresolvedImport, Basis::Declared),
                message,
                span: Span::new(start, end),
                help: Some(help),
                fixes: Vec::new(),
            }
        })
        .collect()
}

/// T0021 for each `import { name as alias }` whose `name` is a struct,
/// `type`, variant or interface of the module: the same rule (and text)
/// as the engines' E0019 (`semantics::check_import_alias`), reported at
/// the alias.
fn import_rename_diagnostics(
    imports: &[ImportInfo],
    loaded: &[Option<Program>],
    index: &SyntaxIndex,
    policy: SeverityPolicy,
) -> Vec<Diagnostic> {
    use crate::parser::index::{DefKind, Role};
    let mut out = Vec::new();
    for (info, program) in imports.iter().zip(loaded) {
        let (Some(program), Some(names)) = (program, &info.names) else {
            continue;
        };
        for name in names {
            let Some(alias) = &name.alias else { continue };
            let Err(message) =
                crate::semantics::check_import_alias(&info.path, program, &name.name, alias)
            else {
                continue;
            };
            let at_alias = index.occurrences.iter().find(|o| {
                o.name == *alias
                    && matches!(&o.role, Role::Def(DefKind::Import { path, name: n })
                        if *path == info.path && *n == name.name)
            });
            let span = match at_alias {
                Some(o) => o.span,
                None => {
                    let start = crate::parser::index::Pos::new(info.line, info.col.max(1));
                    let end =
                        crate::parser::index::Pos::new(info.line, info.col.max(1) + "import".len());
                    Span::new(start, end)
                }
            };
            let headline = crate::semantics::errors::headline(&message).to_string();
            out.push(Diagnostic {
                code: Code::ImportRename,
                severity: policy.severity(Code::ImportRename, Basis::Declared),
                message: headline,
                span,
                help: crate::semantics::errors::message_hint(&message).map(str::to_string),
                fixes: Vec::new(),
            });
        }
    }
    out
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
    policy: SeverityPolicy,
) -> Vec<Diagnostic> {
    // A wildcard import we could not read might define anything.
    if imports.iter().any(|i| i.names.is_none()) {
        return Vec::new();
    }
    let severity = |code: Code| policy.severity(code, Basis::Declared);
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
                    severity: severity(Code::UnknownType),
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
                    severity: severity(Code::UnknownName),
                    message: format!("unknown name '{}'", occ.name),
                    span: occ.span,
                    // A builtin removed by the edition (`say` in 2027)
                    // names its replacement instead of a near spelling.
                    help: Some(
                        match (
                            crate::builtins_registry::removal_hint(&occ.name),
                            suggestion,
                        ) {
                            (Some(hint), _) => hint,
                            (None, Some(s)) => format!("did you mean '{}'?", s),
                            (None, None) => {
                                "define it with `let` (or `fn`) before using it".to_string()
                            }
                        },
                    ),
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
                    severity: severity(Code::UseBeforeDefinition),
                    message: format!(
                        "{} '{}' is used before it is defined (line {})",
                        what, occ.name, def_occ.span.start.line
                    ),
                    span: occ.span,
                    help: Some(if def_occ.visible_from < def_occ.span.start {
                        // A hoisted function waits for the top-level names
                        // it reads (`apply_hoisting`).
                        format!(
                            "'{}' is available from line {} on, once the top-level names it reads are defined: move this use below that line",
                            occ.name, def_occ.visible_from.line
                        )
                    } else {
                        format!("move the definition of '{}' above this use", occ.name)
                    }),
                    fixes: Vec::new(),
                });
            }
            _ => {}
        }
    }
    out
}
