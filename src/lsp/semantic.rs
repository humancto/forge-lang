//! Language features built on the type checker's analysis: hover with
//! inferred types, scope-aware go-to-definition / references / rename
//! across files, quick fixes, semantic tokens and inlay hints.
//!
//! Every feature starts from `typechecker::analyze`, which yields the
//! syntax index (where each name is), the resolution (which definition each
//! name means) and the inferred types. Positions in the index are 1-based
//! lines and char columns; LSP positions are 0-based lines and UTF-16
//! columns, converted at the edges by [`to_pos`] / [`range`].
//!
//! Cross-file symbols: a name imported from another file (`import "lib.fg"`
//! or `import { f } from "lib.fg"`) is identified by the file and the name
//! of the top-level definition there. References and rename search the
//! defining file plus every workspace file that imports it.

use crate::parser::index::{DefKind, OccId, Pos, Role, ScopeKind, Span};
use crate::typechecker::resolve::Resolved;
use crate::typechecker::types::Ty;
use crate::typechecker::{analyze, Analysis, CheckOptions};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Workspace root (from `initialize`), used to find files that import a
/// symbol's file.
static ROOT: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Most files checked by a references/rename workspace scan.
const MAX_WORKSPACE_FILES: usize = 2000;

pub(crate) fn set_root(params: &Value) {
    let root = params
        .pointer("/rootUri")
        .and_then(Value::as_str)
        .or_else(|| {
            params
                .pointer("/workspaceFolders/0/uri")
                .and_then(Value::as_str)
        })
        .and_then(uri_to_path)
        .or_else(|| {
            params
                .pointer("/rootPath")
                .and_then(Value::as_str)
                .map(PathBuf::from)
        });
    if let Ok(mut r) = ROOT.lock() {
        *r = root;
    }
}

fn root() -> Option<PathBuf> {
    ROOT.lock().ok().and_then(|r| r.clone())
}

pub(crate) fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let raw = uri.strip_prefix("file://")?;
    Some(PathBuf::from(percent_decode(raw)))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn path_to_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// A checked document.
pub(crate) struct Doc {
    pub uri: String,
    pub text: String,
    pub path: Option<PathBuf>,
    pub analysis: Analysis,
}

static CACHE: std::sync::LazyLock<Mutex<HashMap<String, Arc<Doc>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Analyze the document at `uri` (open buffer first, else the file on
/// disk). Cached by content. `None` when it does not lex/parse.
pub(crate) fn doc(uri: &str) -> Option<Arc<Doc>> {
    let text = super::get_document(uri).or_else(|| super::read_document(uri))?;
    doc_from(uri, text)
}

fn doc_from(uri: &str, text: String) -> Option<Arc<Doc>> {
    if let Some(hit) = CACHE
        .lock()
        .ok()
        .and_then(|c| c.get(uri).filter(|d| d.text == text).cloned())
    {
        return Some(hit);
    }
    let path = uri_to_path(uri);
    let options = CheckOptions {
        strict: false,
        file: path.clone().filter(|p| p.exists()),
    };
    let analysis = analyze(&text, &options).ok()?;
    let doc = Arc::new(Doc {
        uri: uri.to_string(),
        text,
        path,
        analysis,
    });
    if let Ok(mut c) = CACHE.lock() {
        if c.len() > 256 {
            c.clear();
        }
        c.insert(uri.to_string(), doc.clone());
    }
    Some(doc)
}

fn doc_for_path(path: &Path) -> Option<Arc<Doc>> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    doc(&path_to_uri(&canonical))
}

// ---------------------------------------------------------------------
// Positions
// ---------------------------------------------------------------------

fn line_text(text: &str, line1: usize) -> &str {
    text.lines().nth(line1.saturating_sub(1)).unwrap_or("")
}

/// LSP (0-based line, UTF-16 column) → index position.
pub(crate) fn to_pos(text: &str, line0: usize, character: usize) -> Pos {
    let line = line_text(text, line0 + 1);
    let mut units = 0;
    let mut col = 1;
    for ch in line.chars() {
        if units >= character {
            break;
        }
        units += ch.len_utf16();
        col += 1;
    }
    Pos::new(line0 + 1, col)
}

fn utf16_col(text: &str, pos: Pos) -> usize {
    line_text(text, pos.line)
        .chars()
        .take(pos.col.saturating_sub(1))
        .map(char::len_utf16)
        .sum()
}

pub(crate) fn range(text: &str, span: Span) -> Value {
    json!({
        "start": {"line": span.start.line.saturating_sub(1), "character": utf16_col(text, span.start)},
        "end": {"line": span.end.line.saturating_sub(1), "character": utf16_col(text, span.end)}
    })
}

fn location(doc: &Doc, span: Span) -> Value {
    json!({"uri": doc.uri, "range": range(&doc.text, span)})
}

// ---------------------------------------------------------------------
// Symbols
// ---------------------------------------------------------------------

/// A symbol, identified independently of the file it is used from.
#[derive(Clone)]
struct Symbol {
    /// The file that defines it.
    doc: Arc<Doc>,
    /// Its definition occurrence in that file.
    def: OccId,
}

impl Symbol {
    fn name(&self) -> &str {
        &self.doc.analysis.index.occurrences[self.def].name
    }

    fn kind(&self) -> Option<&DefKind> {
        self.doc.analysis.index.occurrences[self.def].def_kind()
    }

    /// Top-level definitions can be imported by other files.
    fn is_exported(&self) -> bool {
        let occ = &self.doc.analysis.index.occurrences[self.def];
        occ.scope == 0
            && matches!(
                occ.def_kind(),
                Some(
                    DefKind::Function
                        | DefKind::Variable { .. }
                        | DefKind::Struct
                        | DefKind::TypeDef
                        | DefKind::Variant { .. }
                        | DefKind::Interface
                )
            )
    }
}

fn occurrence_at(doc: &Doc, line0: usize, character: usize) -> Option<OccId> {
    let pos = to_pos(&doc.text, line0, character);
    doc.analysis.index.occurrence_at(pos)
}

/// The top-level definition named `name` in `doc`.
fn top_level_def(doc: &Doc, name: &str) -> Option<OccId> {
    doc.analysis
        .index
        .occurrences
        .iter()
        .enumerate()
        .find(|(_, o)| {
            o.scope == 0
                && o.name == name
                && o.is_def()
                && !matches!(o.def_kind(), Some(DefKind::Import { .. }))
        })
        .map(|(id, _)| id)
}

fn resolve_import_path(doc: &Doc, import: &str) -> Option<PathBuf> {
    let base = doc.path.as_deref().and_then(Path::parent);
    crate::package::resolve_import_from(import, base)
}

/// Follow an occurrence to the symbol it denotes (through imports).
fn symbol_of(doc: &Arc<Doc>, occ: OccId) -> Option<Symbol> {
    let a = &doc.analysis;
    let name = a.index.occurrences[occ].name.clone();
    match a.resolution.resolved.get(occ)? {
        Resolved::Def | Resolved::Local(_) => {
            let def = a.resolution.definition(occ)?;
            if let Some(DefKind::Import { path }) = a.index.occurrences[def].def_kind() {
                let target = doc_for_path(&resolve_import_path(doc, path)?)?;
                let def = top_level_def(&target, &name)?;
                return Some(Symbol { doc: target, def });
            }
            Some(Symbol {
                doc: doc.clone(),
                def,
            })
        }
        Resolved::Imported(i) => {
            let import = a.wildcard_imports.get(*i)?;
            let target = doc_for_path(&resolve_import_path(doc, &import.path)?)?;
            let def = top_level_def(&target, &name)?;
            Some(Symbol { doc: target, def })
        }
        _ => None,
    }
}

/// Occurrences in `doc` that denote `sym` (which may live in another file).
fn occurrences_in(doc: &Arc<Doc>, sym: &Symbol) -> Vec<OccId> {
    let a = &doc.analysis;
    let same_file = match (&doc.path, &sym.doc.path) {
        (Some(p), Some(q)) => same_path(p, q),
        _ => doc.uri == sym.doc.uri,
    };
    if same_file {
        return a.resolution.occurrences_of(sym.def);
    }
    if !sym.is_exported() {
        return Vec::new();
    }
    let name = sym.name();
    let imports_target = |import: &str| {
        resolve_import_path(doc, import)
            .zip(sym.doc.path.as_ref())
            .is_some_and(|(p, q)| same_path(&p, q))
    };
    let mut out = Vec::new();
    for (id, occ) in a.index.occurrences.iter().enumerate() {
        if occ.name != name {
            continue;
        }
        let hit = match &a.resolution.resolved[id] {
            Resolved::Def | Resolved::Local(_) => a
                .resolution
                .definition(id)
                .and_then(|d| a.index.occurrences[d].def_kind())
                .is_some_and(|k| matches!(k, DefKind::Import { path } if imports_target(path))),
            Resolved::Imported(i) => a
                .wildcard_imports
                .get(*i)
                .is_some_and(|imp| imports_target(&imp.path)),
            _ => false,
        };
        if hit {
            out.push(id);
        }
    }
    out
}

fn same_path(a: &Path, b: &Path) -> bool {
    let ca = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
    let cb = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    ca == cb
}

/// Documents to search for references to `sym`: its own file, the file
/// asking, open documents and `.fg` files under the workspace root (or the
/// symbol's directory).
fn search_docs(origin: &Arc<Doc>, sym: &Symbol) -> Vec<Arc<Doc>> {
    let mut docs: BTreeMap<String, Arc<Doc>> = BTreeMap::new();
    docs.insert(sym.doc.uri.clone(), sym.doc.clone());
    docs.insert(origin.uri.clone(), origin.clone());
    if !sym.is_exported() {
        return docs.into_values().collect();
    }
    for uri in super::open_document_uris() {
        if let Some(d) = doc(&uri) {
            docs.entry(uri).or_insert(d);
        }
    }
    let dir = root().or_else(|| {
        sym.doc
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
    });
    if let Some(dir) = dir {
        let mut files = Vec::new();
        collect_fg_files(&dir, &mut files, 0);
        for path in files {
            let uri = path_to_uri(&path);
            if docs.contains_key(&uri) {
                continue;
            }
            // Cheap filter before a full analysis.
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !text.contains("import") || !text.contains(sym.name()) {
                continue;
            }
            if let Some(d) = doc_from(&uri, text) {
                docs.insert(uri, d);
            }
        }
    }
    docs.into_values().collect()
}

fn collect_fg_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 12 || out.len() >= MAX_WORKSPACE_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            collect_fg_files(&path, out, depth + 1);
        } else if path.extension().is_some_and(|e| e == "fg") {
            out.push(path.canonicalize().unwrap_or(path));
        }
        if out.len() >= MAX_WORKSPACE_FILES {
            return;
        }
    }
}

// ---------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------

/// `textDocument/definition`. `None` = not handled here (fall back).
pub(crate) fn definition(uri: &str, line: usize, character: usize) -> Option<Value> {
    let doc = doc(uri)?;
    let occ = occurrence_at(&doc, line, character)?;
    let sym = symbol_of(&doc, occ)?;
    let span = sym.doc.analysis.index.occurrences[sym.def].span;
    Some(location(&sym.doc, span))
}

/// `textDocument/references`.
pub(crate) fn references(
    uri: &str,
    line: usize,
    character: usize,
    include_declaration: bool,
) -> Option<Value> {
    let doc = doc(uri)?;
    let occ = occurrence_at(&doc, line, character)?;
    let sym = symbol_of(&doc, occ)?;
    let mut out = Vec::new();
    for d in search_docs(&doc, &sym) {
        for id in occurrences_in(&d, &sym) {
            let o = &d.analysis.index.occurrences[id];
            if !include_declaration && o.is_def() {
                continue;
            }
            out.push(location(&d, o.span));
        }
    }
    Some(Value::Array(out))
}

const KEYWORDS: &[&str] = &[
    "let",
    "mut",
    "fn",
    "return",
    "if",
    "else",
    "match",
    "for",
    "in",
    "while",
    "loop",
    "break",
    "continue",
    "struct",
    "type",
    "interface",
    "impl",
    "pub",
    "import",
    "spawn",
    "squad",
    "true",
    "false",
    "null",
    "set",
    "to",
    "change",
    "define",
    "otherwise",
    "nah",
    "each",
    "repeat",
    "times",
    "grab",
    "from",
    "wait",
    "seconds",
    "say",
    "yell",
    "whisper",
    "thing",
    "power",
    "give",
    "craft",
    "the",
    "try",
    "catch",
    "forge",
    "hold",
    "emit",
    "unpack",
    "async",
    "await",
    "yield",
    "when",
    "unless",
    "until",
    "must",
    "check",
    "safe",
    "where",
    "timeout",
    "retry",
    "schedule",
    "every",
    "any",
    "ask",
    "prompt",
    "agent",
    "transform",
    "table",
    "select",
    "order",
    "by",
    "limit",
    "keep",
    "take",
    "freeze",
    "watch",
    "download",
    "crawl",
];

fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
        && !KEYWORDS.contains(&name)
}

/// `textDocument/prepareRename`: the range of the renamable name, or
/// `null` for builtins, fields and keywords.
pub(crate) fn prepare_rename(uri: &str, line: usize, character: usize) -> Value {
    let Some(doc) = doc(uri) else {
        return Value::Null;
    };
    let Some(occ) = occurrence_at(&doc, line, character) else {
        return Value::Null;
    };
    if symbol_of(&doc, occ).is_none() {
        return Value::Null;
    }
    let o = &doc.analysis.index.occurrences[occ];
    json!({"range": range(&doc.text, o.span), "placeholder": o.name})
}

/// `textDocument/rename`: a workspace edit renaming every occurrence of the
/// symbol (scope-aware: shadowed and unrelated names are untouched).
pub(crate) fn rename(
    uri: &str,
    line: usize,
    character: usize,
    new_name: &str,
) -> Result<Value, String> {
    if !valid_identifier(new_name) {
        return Err(format!("'{}' is not a valid identifier", new_name));
    }
    let doc = doc(uri).ok_or("the document does not parse")?;
    let occ = occurrence_at(&doc, line, character).ok_or("no symbol at this position")?;
    let sym = symbol_of(&doc, occ).ok_or("builtins, fields and modules cannot be renamed")?;
    let mut changes: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for d in search_docs(&doc, &sym) {
        let edits: Vec<Value> = occurrences_in(&d, &sym)
            .into_iter()
            .map(|id| {
                json!({
                    "range": range(&d.text, d.analysis.index.occurrences[id].span),
                    "newText": new_name,
                })
            })
            .collect();
        if !edits.is_empty() {
            changes.insert(d.uri.clone(), edits);
        }
    }
    Ok(json!({ "changes": changes }))
}

fn markdown(value: String) -> Value {
    json!({"contents": {"kind": "markdown", "value": value}})
}

fn code_block(body: &str) -> String {
    format!("```forge\n{}\n```", body)
}

/// Signature line of a function definition, with inferred types.
fn fn_signature(name: &str, ty: Option<&Ty>, doc: &Doc, def: OccId) -> String {
    let index = &doc.analysis.index;
    // Parameter names: the Parameter defs of the scope opened after `def`.
    let fn_scope = index
        .scopes
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            s.kind == ScopeKind::Function && s.start >= index.occurrences[def].span.end
        })
        .min_by_key(|(_, s)| s.start)
        .map(|(id, _)| id);
    let params: Vec<&str> = index
        .occurrences
        .iter()
        .filter(|o| Some(o.scope) == fn_scope && matches!(o.def_kind(), Some(DefKind::Parameter)))
        .map(|o| o.name.as_str())
        .collect();
    match ty {
        Some(Ty::Fn(f)) => {
            let ps: Vec<String> = f
                .params
                .iter()
                .enumerate()
                .map(|(i, t)| match params.get(i) {
                    Some(n) if t.is_any() => n.to_string(),
                    Some(n) => format!("{}: {}", n, t),
                    None => t.to_string(),
                })
                .collect();
            let generics = if f.type_params.is_empty() {
                String::new()
            } else {
                format!("<{}>", f.type_params.join(", "))
            };
            format!("fn {}{}({}) -> {}", name, generics, ps.join(", "), f.ret)
        }
        _ => format!("fn {}({})", name, params.join(", ")),
    }
}

fn describe_def(doc: &Doc, def: OccId) -> String {
    let a = &doc.analysis;
    let o = &a.index.occurrences[def];
    let ty = a.facts.def_types.get(&def);
    let ty_str = ty.map(|t| t.to_string()).unwrap_or_else(|| "Any".into());
    match o.def_kind() {
        Some(DefKind::Function) | Some(DefKind::Callable) => fn_signature(&o.name, ty, doc, def),
        Some(DefKind::Method { owner }) => {
            format!(
                "// method of {}\n{}",
                owner,
                fn_signature(&o.name, ty, doc, def)
            )
        }
        Some(DefKind::Variable { mutable }) => format!(
            "{} {}: {}",
            if *mutable { "let mut" } else { "let" },
            o.name,
            ty_str
        ),
        Some(DefKind::Parameter) => format!("(parameter) {}: {}", o.name, ty_str),
        Some(DefKind::LoopVariable) => format!("(loop variable) {}: {}", o.name, ty_str),
        Some(DefKind::PatternBinding) => format!("(binding) {}: {}", o.name, ty_str),
        Some(DefKind::CatchVariable) => format!("(catch) {}", o.name),
        Some(DefKind::Import { path }) => format!("import {{ {} }} from \"{}\"", o.name, path),
        Some(DefKind::Struct) => {
            let fields: Vec<String> = a
                .index
                .occurrences
                .iter()
                .enumerate()
                .filter(|(_, f)| matches!(f.def_kind(), Some(DefKind::Field { owner }) if *owner == o.name))
                .map(|(id, f)| {
                    format!(
                        "  {}: {}",
                        f.name,
                        a.facts
                            .def_types
                            .get(&id)
                            .map(|t| t.to_string())
                            .unwrap_or_else(|| "Any".into())
                    )
                })
                .collect();
            format!("struct {} {{\n{}\n}}", o.name, fields.join("\n"))
        }
        Some(DefKind::Field { owner }) => format!("(field of {}) {}: {}", owner, o.name, ty_str),
        Some(DefKind::TypeDef) => format!("type {} = {}", o.name, ty_str),
        Some(DefKind::Variant { owner }) => {
            format!("(variant of {}) {}: {}", owner, o.name, ty_str)
        }
        Some(DefKind::Interface) => format!("interface {}", o.name),
        Some(DefKind::TypeParameter) => format!("(type parameter) {}", o.name),
        None => o.name.clone(),
    }
}

/// `textDocument/hover`. `None` = not handled here (fall back).
pub(crate) fn hover(uri: &str, line: usize, character: usize) -> Option<Value> {
    let doc = doc(uri)?;
    let occ = occurrence_at(&doc, line, character)?;
    let a = &doc.analysis;
    let o = &a.index.occurrences[occ];
    if matches!(o.role, Role::Field) {
        let ty = a.facts.field_types.get(&occ)?;
        return Some(markdown(code_block(&format!("(field) {}: {}", o.name, ty))));
    }
    if let Some(sym) = symbol_of(&doc, occ) {
        let text = describe_def(&sym.doc, sym.def);
        let mut value = code_block(&text);
        if !Arc::ptr_eq(&sym.doc, &doc) {
            if let Some(p) = &sym.doc.path {
                value.push_str(&format!("\n\nDefined in `{}`", p.display()));
            }
        }
        return Some(markdown(value));
    }
    if matches!(a.resolution.resolved[occ], Resolved::Global) {
        if let Some(sig) = crate::typechecker::builtins::function(&o.name) {
            let generics = if sig.type_params.is_empty() {
                String::new()
            } else {
                format!("<{}>", sig.type_params.join(", "))
            };
            let mut value = code_block(&format!(
                "fn {}{}{}",
                o.name,
                generics,
                Ty::Fn(sig.clone()).to_string().trim_start_matches("fn")
            ));
            if let Some(doc_line) = super::builtin_doc(&o.name) {
                if let Some((_, desc)) = doc_line.split_once(" — ") {
                    value.push_str("\n\n");
                    value.push_str(desc);
                }
            }
            return Some(markdown(value));
        }
    }
    None
}

/// `textDocument/codeAction`: quick fixes from diagnostics ("did you mean").
pub(crate) fn code_actions(uri: &str, params: &Value) -> Value {
    let Some(doc) = doc(uri) else {
        return json!([]);
    };
    let (Some(start), Some(end)) = (params.pointer("/range/start"), params.pointer("/range/end"))
    else {
        return json!([]);
    };
    let pos = |v: &Value| {
        to_pos(
            &doc.text,
            v["line"].as_u64().unwrap_or(0) as usize,
            v["character"].as_u64().unwrap_or(0) as usize,
        )
    };
    let (start, end) = (pos(start), pos(end));
    let mut actions = Vec::new();
    for d in &doc.analysis.diagnostics {
        if d.span.end < start || d.span.start > end {
            continue;
        }
        for fix in &d.fixes {
            let diagnostic = json!({
                "range": range(&doc.text, d.span),
                "severity": if d.is_error() { 1 } else { 2 },
                "code": d.code.as_str(),
                "source": "forge-typecheck",
                "message": d.message,
            });
            actions.push(json!({
                "title": fix.title,
                "kind": "quickfix",
                "isPreferred": true,
                "diagnostics": [diagnostic],
                "edit": {"changes": {doc.uri.clone(): [{
                    "range": range(&doc.text, fix.span),
                    "newText": fix.replacement,
                }]}}
            }));
        }
    }
    Value::Array(actions)
}

pub(crate) const TOKEN_TYPES: &[&str] = &[
    "namespace",
    "type",
    "struct",
    "enum",
    "interface",
    "typeParameter",
    "parameter",
    "variable",
    "property",
    "enumMember",
    "function",
    "method",
];
pub(crate) const TOKEN_MODIFIERS: &[&str] = &["declaration", "readonly", "defaultLibrary"];

fn token_type(name: &str) -> u32 {
    TOKEN_TYPES.iter().position(|t| *t == name).unwrap_or(7) as u32
}

fn kind_token(kind: &DefKind) -> &'static str {
    match kind {
        DefKind::Function | DefKind::Callable => "function",
        DefKind::Method { .. } => "method",
        DefKind::Parameter => "parameter",
        DefKind::TypeParameter => "typeParameter",
        DefKind::Struct => "struct",
        DefKind::Interface => "interface",
        DefKind::TypeDef => "enum",
        DefKind::Variant { .. } => "enumMember",
        DefKind::Field { .. } => "property",
        _ => "variable",
    }
}

/// `textDocument/semanticTokens/full`: every name, classified by what it
/// resolves to (functions, parameters, types, fields, builtins, ...).
pub(crate) fn semantic_tokens(uri: &str) -> Value {
    let Some(doc) = doc(uri) else {
        return json!({"data": []});
    };
    let a = &doc.analysis;
    let mut tokens: Vec<(Pos, usize, u32, u32)> = Vec::new();
    for (id, o) in a.index.occurrences.iter().enumerate() {
        if o.span.start.line != o.span.end.line || o.span.start.line == 0 {
            continue;
        }
        let (ty, mut mods) = match (&o.role, &a.resolution.resolved[id]) {
            (Role::Field, _) => ("property", 0),
            (Role::Def(k), Resolved::Def) => (kind_token(k), 1),
            (_, Resolved::Local(def)) => {
                let k = a.index.occurrences[*def].def_kind();
                (k.map_or("variable", kind_token), 0)
            }
            (_, Resolved::Imported(_)) => {
                let kind = symbol_of(&doc, id).and_then(|s| s.kind().cloned());
                (kind.as_ref().map_or("variable", kind_token), 0)
            }
            (Role::TypeRef, _) => ("type", 4),
            (_, Resolved::Global) if crate::typechecker::builtins::module(&o.name).is_some() => {
                ("namespace", 4)
            }
            (_, Resolved::Global) => ("function", 4),
            _ => ("variable", 0),
        };
        let def = a.resolution.definition(id);
        if def
            .and_then(|d| a.index.occurrences[d].def_kind())
            .is_some_and(|k| matches!(k, DefKind::Variable { mutable: false }))
        {
            mods |= 2;
        }
        let len: usize = line_text(&doc.text, o.span.start.line)
            .chars()
            .skip(o.span.start.col - 1)
            .take(o.span.end.col - o.span.start.col)
            .map(char::len_utf16)
            .sum();
        tokens.push((o.span.start, len, token_type(ty), mods));
    }
    tokens.sort_by_key(|t| t.0);
    tokens.dedup_by_key(|t| t.0);
    let mut data = Vec::with_capacity(tokens.len() * 5);
    let (mut prev_line, mut prev_col) = (0usize, 0usize);
    for (pos, len, ty, mods) in tokens {
        let line = pos.line - 1;
        let col = utf16_col(&doc.text, pos);
        let delta_line = line - prev_line;
        let delta_col = if delta_line == 0 { col - prev_col } else { col };
        data.extend([delta_line as u32, delta_col as u32, len as u32, ty, mods]);
        prev_line = line;
        prev_col = col;
    }
    json!({ "data": data })
}

/// `textDocument/inlayHint`: inferred types after unannotated `let`
/// bindings (`let n = 1` shows `: Int`).
pub(crate) fn inlay_hints(uri: &str, params: &Value) -> Value {
    let Some(doc) = doc(uri) else {
        return json!([]);
    };
    let first = params
        .pointer("/range/start/line")
        .and_then(Value::as_u64)
        .map_or(1, |l| l as usize + 1);
    let last = params
        .pointer("/range/end/line")
        .and_then(Value::as_u64)
        .map_or(usize::MAX, |l| l as usize + 1);
    let a = &doc.analysis;
    let mut hints = Vec::new();
    for (id, o) in a.index.occurrences.iter().enumerate() {
        if !matches!(o.def_kind(), Some(DefKind::Variable { .. })) {
            continue;
        }
        if o.span.start.line < first || o.span.start.line > last {
            continue;
        }
        let Some(ty) = a.facts.def_types.get(&id) else {
            continue;
        };
        if ty.is_unknown() || matches!(ty, Ty::Null) {
            continue;
        }
        // Skip annotated bindings: the next non-space char is `:`.
        let rest: String = line_text(&doc.text, o.span.end.line)
            .chars()
            .skip(o.span.end.col - 1)
            .collect();
        if rest.trim_start().starts_with(':') {
            continue;
        }
        hints.push(json!({
            "position": {"line": o.span.end.line - 1, "character": utf16_col(&doc.text, o.span.end)},
            "label": format!(": {}", ty),
            "kind": 1,
            "paddingLeft": false,
        }));
    }
    Value::Array(hints)
}
