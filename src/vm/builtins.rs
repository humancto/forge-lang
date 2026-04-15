/// VM Builtin Dispatch
/// Extracted from machine.rs to keep that file navigable.
/// This is a continuation of `impl VM` — same struct, separate file.
/// Do NOT change logic here; this is a pure structural extraction.
use chrono::{Datelike, Timelike, Utc};
use indexmap::IndexMap;
use std::sync::Arc;

use super::machine::{VMError, VM};
use super::value::*;

impl VM {
    pub(super) fn call_native(&mut self, name: &str, args: Vec<Value>) -> Result<Value, VMError> {
        match name {
            "__forge_register_struct" => {
                if args.len() != 3 {
                    return Err(VMError::new(
                        "__forge_register_struct() requires (name, embeds, defaults)",
                    ));
                }
                let type_name = self.get_string_arg(&args, 0)?;
                let embeds = self.parse_embedded_fields(&args[1])?;
                let defaults = self.parse_object_fields(&args[2])?;

                if embeds.is_empty() {
                    self.embedded_fields.remove(&type_name);
                } else {
                    self.embedded_fields.insert(type_name.clone(), embeds);
                }

                if defaults.is_empty() {
                    self.struct_defaults.remove(&type_name);
                } else {
                    self.struct_defaults.insert(type_name.clone(), defaults);
                }

                let marker = self.make_struct_marker(&type_name);
                self.globals.insert(type_name, marker.clone());
                Ok(marker)
            }
            "__forge_new_struct" => {
                if args.len() != 2 {
                    return Err(VMError::new(
                        "__forge_new_struct() requires (name, provided_fields)",
                    ));
                }
                let type_name = self.get_string_arg(&args, 0)?;
                let mut fields = self
                    .struct_defaults
                    .get(&type_name)
                    .cloned()
                    .unwrap_or_default();
                for (key, value) in self.parse_object_fields(&args[1])? {
                    fields.insert(key, value);
                }
                fields.insert("__type__".to_string(), self.alloc_string(&type_name));
                let r = self.gc.alloc(ObjKind::Object(fields));
                Ok(Value::obj(r))
            }
            "__forge_register_interface" => {
                if args.len() != 2 {
                    return Err(VMError::new(
                        "__forge_register_interface() requires (name, interface)",
                    ));
                }
                let name = self.get_string_arg(&args, 0)?;
                let iface = args[1].clone();
                self.globals.insert(name.clone(), iface.clone());
                self.globals
                    .insert(format!("__interface_{}__", name), iface.clone());
                Ok(iface)
            }
            "__forge_register_prompt" => {
                if args.len() != 1 {
                    return Err(VMError::new("__forge_register_prompt() requires (name)"));
                }
                let name = self.get_string_arg(&args, 0)?;
                let placeholder = self.alloc_builtin(&format!("prompt:{}", name));
                self.globals.insert(name, placeholder.clone());
                Ok(placeholder)
            }
            "__forge_register_agent" => {
                if args.len() != 1 {
                    return Err(VMError::new("__forge_register_agent() requires (name)"));
                }
                let name = self.get_string_arg(&args, 0)?;
                let placeholder = self.alloc_builtin(&format!("agent:{}", name));
                self.globals.insert(name, placeholder.clone());
                Ok(placeholder)
            }
            "__forge_register_method" => {
                if args.len() != 4 {
                    return Err(VMError::new(
                        "__forge_register_method() requires (type_name, method_name, has_receiver, function)",
                    ));
                }
                let type_name = self.get_string_arg(&args, 0)?;
                let method_name = self.get_string_arg(&args, 1)?;
                let has_receiver = match args[2].as_bool() {
                    Some(flag) => flag,
                    _ => {
                        return Err(VMError::new(
                            "__forge_register_method() third argument must be Bool",
                        ))
                    }
                };
                let func = args[3].clone();

                self.method_tables
                    .entry(type_name.clone())
                    .or_default()
                    .insert(method_name.clone(), func.clone());

                if !has_receiver {
                    self.static_methods
                        .entry(type_name)
                        .or_default()
                        .insert(method_name, func);
                }

                Ok(Value::null())
            }
            "__forge_validate_impl" => {
                if args.len() != 2 {
                    return Err(VMError::new(
                        "__forge_validate_impl() requires (type_name, ability_name)",
                    ));
                }
                let type_name = self.get_string_arg(&args, 0)?;
                let ability_name = self.get_string_arg(&args, 1)?;
                let iface_key = format!("__interface_{}__", ability_name);
                let iface =
                    self.globals.get(&iface_key).cloned().ok_or_else(|| {
                        VMError::new(&format!("unknown power '{}'", ability_name))
                    })?;
                let type_methods = self.method_tables.get(&type_name);
                for required in self.interface_method_names(&iface) {
                    let implemented =
                        type_methods.is_some_and(|methods| methods.contains_key(&required));
                    if !implemented {
                        return Err(VMError::new(&format!(
                            "'{}' does not implement '{}' required by power '{}'",
                            type_name, required, ability_name
                        )));
                    }
                }
                Ok(Value::null())
            }
            "__forge_call_method" => {
                if args.len() < 2 {
                    return Err(VMError::new(
                        "__forge_call_method() requires (receiver, method_name, ...args)",
                    ));
                }
                let receiver = args[0].clone();
                let method_name = self.get_string_arg(&args, 1)?;
                self.call_forge_method(receiver, &method_name, &args[2..])
            }
            "__forge_binding_matches" => {
                if args.len() != 2 {
                    return Err(VMError::new(
                        "__forge_binding_matches() requires (binding_name, value)",
                    ));
                }
                let binding_name = self.get_string_arg(&args, 0)?;
                let value = args[1].clone();

                let Some(bound_value) = self.globals.get(&binding_name).cloned() else {
                    return Ok(Value::bool_val(true));
                };

                if let (Some(bound_variant), Some(value_variant)) = (
                    self.value_variant_name(&bound_value),
                    self.value_variant_name(&value),
                ) {
                    return Ok(Value::bool_val(bound_variant == value_variant));
                }

                Ok(Value::bool_val(true))
            }
            "__forge_retry_count" => {
                if args.len() != 1 {
                    return Err(VMError::new("__forge_retry_count() requires (count)"));
                }
                if let Some(n) = args[0].as_int(&self.gc) {
                    Ok(Value::small_int(n.max(0)))
                } else {
                    Ok(Value::small_int(3))
                }
            }
            "__forge_retry_wait" => {
                if args.len() != 1 {
                    return Err(VMError::new("__forge_retry_wait() requires (attempt)"));
                }
                let attempt = args[0]
                    .as_int(&self.gc)
                    .map(|n| n.max(0) as u64)
                    .unwrap_or(0);
                if attempt > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(100 * attempt));
                }
                Ok(Value::null())
            }
            "__forge_retry_failed" => {
                if args.len() != 2 {
                    return Err(VMError::new(
                        "__forge_retry_failed() requires (count, last_error)",
                    ));
                }
                let count = match args[0].classify(&self.gc) {
                    ValueKind::Int(n) => n.max(0),
                    _ => 0,
                };
                let last_error = match args[1].classify(&self.gc) {
                    ValueKind::Obj(r) => self
                        .gc
                        .get(r)
                        .and_then(|obj| match &obj.kind {
                            ObjKind::Object(map) => map.get("message").cloned(),
                            _ => None,
                        })
                        .map(|value| value.display(&self.gc))
                        .unwrap_or_default(),
                    _ => args[1].display(&self.gc),
                };
                Err(VMError::new(&format!(
                    "retry failed after {} attempts: {}",
                    count, last_error
                )))
            }
            "__forge_where_filter" => {
                if args.len() != 4 {
                    return Err(VMError::new(
                        "__forge_where_filter() requires (array, field, op, value)",
                    ));
                }
                let items =
                    self.array_items(&args[0], "__forge_where_filter() first arg must be array")?;
                let field = self.get_string_arg(&args, 1)?;
                let op = self.get_string_arg(&args, 2)?;
                let cmp_value = args[3].clone();
                let filtered = items
                    .into_iter()
                    .filter(|item| {
                        self.get_object_fields(item)
                            .and_then(|fields| fields.get(&field).cloned())
                            .is_some_and(|field_value| {
                                self.query_compare(&field_value, &op, &cmp_value)
                            })
                    })
                    .collect::<Vec<_>>();
                let r = self.gc.alloc(ObjKind::Array(filtered));
                Ok(Value::obj(r))
            }
            "__forge_pipe_sort" => {
                if args.len() != 2 {
                    return Err(VMError::new("__forge_pipe_sort() requires (array, field)"));
                }
                let mut items =
                    self.array_items(&args[0], "__forge_pipe_sort() first arg must be array")?;
                let field = self.get_string_arg(&args, 1)?;
                items.sort_by(|a, b| {
                    let left = self
                        .get_object_fields(a)
                        .and_then(|fields| fields.get(&field).cloned())
                        .unwrap_or(Value::null());
                    let right = self
                        .get_object_fields(b)
                        .and_then(|fields| fields.get(&field).cloned())
                        .unwrap_or(Value::null());
                    self.query_value_cmp(&left, &right)
                });
                let r = self.gc.alloc(ObjKind::Array(items));
                Ok(Value::obj(r))
            }
            "__forge_pipe_take" => {
                if args.len() != 2 {
                    return Err(VMError::new("__forge_pipe_take() requires (array, count)"));
                }
                let items =
                    self.array_items(&args[0], "__forge_pipe_take() first arg must be array")?;
                let count = match args[1].classify(&self.gc) {
                    ValueKind::Int(n) => n.max(0) as usize,
                    ValueKind::Float(n) => n.max(0.0) as usize,
                    _ => 10,
                };
                let r = self
                    .gc
                    .alloc(ObjKind::Array(items.into_iter().take(count).collect()));
                Ok(Value::obj(r))
            }
            "__forge_raise_error" => {
                if args.len() != 1 {
                    return Err(VMError::new("__forge_raise_error() requires (error)"));
                }
                let message = match args[0].classify(&self.gc) {
                    ValueKind::Obj(r) => self
                        .gc
                        .get(r)
                        .and_then(|obj| match &obj.kind {
                            ObjKind::Object(map) => map.get("message").cloned(),
                            _ => None,
                        })
                        .map(|value| value.display(&self.gc))
                        .unwrap_or_else(|| args[0].display(&self.gc)),
                    _ => args[0].display(&self.gc),
                };
                Err(VMError::new(&message))
            }
            "__forge_import_module" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(VMError::new(
                        "__forge_import_module() requires (path, [names])",
                    ));
                }

                let requested_names = match args.get(1).map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Obj(r)) => self
                        .gc
                        .get(r)
                        .and_then(|obj| match &obj.kind {
                            ObjKind::Array(items) => Some(
                                items.iter()
                                    .filter_map(|item| self.get_string(item))
                                    .collect::<Vec<_>>(),
                            ),
                            _ => None,
                        })
                        .ok_or_else(|| {
                            VMError::new(
                                "__forge_import_module() second argument must be an array of strings",
                            )
                        })?,
                    Some(ValueKind::Null) | None => Vec::new(),
                    Some(_) => {
                        return Err(VMError::new(
                            "__forge_import_module() second argument must be an array of strings",
                        ))
                    }
                };

                let path = self.get_string_arg(&args, 0)?;
                let file_path = crate::package::resolve_import(&path)
                    .unwrap_or_else(|| std::path::PathBuf::from(&path));
                let source = std::fs::read_to_string(&file_path)
                    .map_err(|e| VMError::new(&format!("cannot import '{}': {}", path, e)))?;

                let mut lexer = crate::lexer::Lexer::new(&source);
                let tokens = lexer.tokenize().map_err(|e| {
                    VMError::new(&format!("import '{}' lex error: {}", path, e.message))
                })?;
                let mut parser = crate::parser::Parser::new(tokens);
                let program = parser.parse_program().map_err(|e| {
                    VMError::new(&format!("import '{}' parse error: {}", path, e.message))
                })?;

                let export_names = if requested_names.is_empty() {
                    program
                        .statements
                        .iter()
                        .filter_map(|spanned| match &spanned.stmt {
                            crate::parser::ast::Stmt::FnDef { name, .. }
                            | crate::parser::ast::Stmt::Let { name, .. } => Some(name.clone()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                } else {
                    requested_names
                };

                let chunk = crate::vm::compiler::compile_module(&program).map_err(|e| {
                    VMError::new(&format!("import '{}' compile error: {}", path, e.message))
                })?;
                self.execute_module(&chunk).map_err(|e| {
                    VMError::new(&format!("import '{}' runtime error: {}", path, e))
                })?;

                let mut exports = IndexMap::new();
                for name in export_names {
                    let value = self.globals.get(&name).cloned().ok_or_else(|| {
                        VMError::new(&format!("import '{}' does not export '{}'", path, name))
                    })?;
                    exports.insert(name, value);
                }
                let exports_ref = self.gc.alloc(ObjKind::Object(exports));
                Ok(Value::obj(exports_ref))
            }
            "println" | "say" => {
                let text: Vec<String> = args.iter().map(|v| v.display(&self.gc)).collect();
                let output = text.join(" ");
                println!("{}", output);
                self.output.push(output);
                Ok(Value::null())
            }
            "print" => {
                let text: Vec<String> = args.iter().map(|v| v.display(&self.gc)).collect();
                print!("{}", text.join(" "));
                Ok(Value::null())
            }
            "yell" => {
                let text: Vec<String> = args.iter().map(|v| v.display(&self.gc)).collect();
                let output = text.join(" ").to_uppercase();
                println!("{}", output);
                self.output.push(output);
                Ok(Value::null())
            }
            "whisper" => {
                let text: Vec<String> = args.iter().map(|v| v.display(&self.gc)).collect();
                let output = text.join(" ").to_lowercase();
                println!("{}", output);
                self.output.push(output);
                Ok(Value::null())
            }
            "len" => match args.first() {
                Some(v) => {
                    let len = if let Some(r) = v.as_obj() {
                        self.gc.get(r).map_or(0, |o| match &o.kind {
                            ObjKind::String(s) => s.chars().count() as i64,
                            ObjKind::Array(a) | ObjKind::Tuple(a) | ObjKind::Set(a) => {
                                a.len() as i64
                            }
                            ObjKind::Object(o) => o.len() as i64,
                            _ => 0,
                        })
                    } else {
                        0
                    };
                    Ok(Value::small_int(len))
                }
                None => Err(VMError::new("len() requires an argument")),
            },
            "type" => match args.first() {
                Some(v) => {
                    let name = v.type_name(&self.gc);
                    Ok(self.alloc_string(name))
                }
                None => Err(VMError::new("type() requires an argument")),
            },
            "str" => {
                let s = args
                    .first()
                    .map(|v| v.display(&self.gc))
                    .unwrap_or_default();
                Ok(self.alloc_string(&s))
            }
            "int" => match args.first().map(|v| v.classify(&self.gc)) {
                Some(ValueKind::Int(n)) => Ok(Value::int(n, &mut self.gc)),
                Some(ValueKind::Float(n)) => Ok(Value::int(n as i64, &mut self.gc)),
                // Parity with interpreter: bool → 0/1
                Some(ValueKind::Bool(b)) => Ok(Value::small_int(if b { 1 } else { 0 })),
                Some(ValueKind::Obj(r)) => {
                    let s_owned = self.gc.get(r).and_then(|obj| match &obj.kind {
                        ObjKind::String(s) => Some(s.clone()),
                        _ => None,
                    });
                    if let Some(s) = s_owned {
                        return s
                            .parse::<i64>()
                            .map(|n| Value::int(n, &mut self.gc))
                            .map_err(|_| VMError::new(&format!("cannot convert '{}' to Int", s)));
                    }
                    Err(VMError::new("int() requires number, bool, or string"))
                }
                _ => Err(VMError::new("int() requires number, bool, or string")),
            },
            "float" => match args.first().map(|v| v.classify(&self.gc)) {
                Some(ValueKind::Int(n)) => Ok(Value::float(n as f64)),
                Some(ValueKind::Float(n)) => Ok(Value::float(n)),
                // Parity with interpreter: parse string to float
                Some(ValueKind::Obj(r)) => {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::String(s) = &obj.kind {
                            return s.parse::<f64>().map(Value::float).map_err(|_| {
                                VMError::new(&format!("cannot convert '{}' to Float", s))
                            });
                        }
                    }
                    Err(VMError::new("float() requires a number or numeric string"))
                }
                _ => Err(VMError::new("float() requires a number or numeric string")),
            },
            "range" => match (
                args.first().and_then(|v| v.as_int(&self.gc)),
                args.get(1).and_then(|v| v.as_int(&self.gc)),
            ) {
                (Some(start), Some(end)) => {
                    let items: Vec<Value> = (start..end).map(Value::small_int).collect();
                    let r = self.gc.alloc(ObjKind::Array(items));
                    Ok(Value::obj(r))
                }
                (Some(end_val), None) => {
                    let items: Vec<Value> = (0..end_val).map(Value::small_int).collect();
                    let r = self.gc.alloc(ObjKind::Array(items));
                    Ok(Value::obj(r))
                }
                _ => Err(VMError::new("range() requires integer arguments")),
            },
            "set" => {
                if args.is_empty() {
                    let r = self.gc.alloc(ObjKind::Set(Vec::new()));
                    return Ok(Value::obj(r));
                }
                if args.len() != 1 {
                    return Err(VMError::new("set() takes 0 or 1 argument"));
                }
                if let Some(r) = args[0].as_obj() {
                    if let Some(obj) = self.gc.get(r) {
                        match &obj.kind {
                            ObjKind::Array(items) | ObjKind::Tuple(items) | ObjKind::Set(items) => {
                                // Clone to drop GC borrow, then dedup via set_eq.
                                let source = items.clone();
                                let mut deduped = Vec::new();
                                for item in source {
                                    if !deduped
                                        .iter()
                                        .any(|existing: &Value| existing.set_eq(&item, &self.gc))
                                    {
                                        deduped.push(item);
                                    }
                                }
                                let nr = self.gc.alloc(ObjKind::Set(deduped));
                                return Ok(Value::obj(nr));
                            }
                            _ => {}
                        }
                    }
                }
                Err(VMError::new("set() requires an array, tuple, or set"))
            }
            "push" => {
                if args.len() != 2 {
                    return Err(VMError::new("push() requires array and value"));
                }
                if let Some(r) = args[0].as_obj() {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            let mut new_items = items.clone();
                            new_items.push(args[1].clone());
                            let nr = self.gc.alloc(ObjKind::Array(new_items));
                            return Ok(Value::obj(nr));
                        }
                    }
                }
                Err(VMError::new("push() requires an array"))
            }
            "pop" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            let mut new_items = items.clone();
                            new_items.pop();
                            let nr = self.gc.alloc(ObjKind::Array(new_items));
                            return Ok(Value::obj(nr));
                        }
                    }
                }
                Err(VMError::new("pop() requires an array"))
            }
            // Lowercase aliases must come BEFORE the capitalized forms
            // so the match arms are not shadowed ("Ok" would match before "ok" | "Ok")
            "ok" => {
                let val = args.first().cloned().unwrap_or(Value::null());
                let r = self.gc.alloc(ObjKind::ResultOk(val));
                Ok(Value::obj(r))
            }
            "err" => {
                let val = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| self.alloc_string("error"));
                let r = self.gc.alloc(ObjKind::ResultErr(val));
                Ok(Value::obj(r))
            }
            "Ok" | "Some" => {
                let val = args.first().cloned().unwrap_or(Value::null());
                if name == "Some" {
                    let mut obj = IndexMap::new();
                    obj.insert("__type__".to_string(), self.alloc_string("Option"));
                    obj.insert("__variant__".to_string(), self.alloc_string("Some"));
                    obj.insert("_0".to_string(), val);
                    let r = self.gc.alloc(ObjKind::Object(obj));
                    Ok(Value::obj(r))
                } else {
                    let r = self.gc.alloc(ObjKind::ResultOk(val));
                    Ok(Value::obj(r))
                }
            }
            "Err" => {
                let val = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| self.alloc_string("error"));
                let r = self.gc.alloc(ObjKind::ResultErr(val));
                Ok(Value::obj(r))
            }
            "is_ok" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    if let Some(obj) = self.gc.get(r) {
                        return Ok(Value::bool_val(matches!(obj.kind, ObjKind::ResultOk(_))));
                    }
                }
                Ok(Value::bool_val(false))
            }
            "is_err" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    if let Some(obj) = self.gc.get(r) {
                        return Ok(Value::bool_val(matches!(obj.kind, ObjKind::ResultErr(_))));
                    }
                }
                Ok(Value::bool_val(false))
            }
            "unwrap" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::ResultOk(v) = &obj.kind {
                            return Ok(v.clone());
                        }
                        if let ObjKind::ResultErr(v) = &obj.kind {
                            return Err(VMError::new(&format!(
                                "unwrap() on Err: {}",
                                v.display(&self.gc)
                            )));
                        }
                    }
                }
                Err(VMError::new("unwrap() requires a Result"))
            }
            "unwrap_or" => {
                if args.len() < 2 {
                    return Err(VMError::new("unwrap_or() requires 2 args"));
                }
                if let Some(r) = args[0].as_obj() {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::ResultOk(v) = &obj.kind {
                            return Ok(v.clone());
                        }
                        if matches!(obj.kind, ObjKind::ResultErr(_)) {
                            return Ok(args[1].clone());
                        }
                    }
                }
                Ok(args[1].clone())
            }
            "assert" => {
                let cond = args.first().cloned().unwrap_or(Value::bool_val(false));
                if !cond.is_truthy(&self.gc) {
                    let msg = args
                        .get(1)
                        .map(|v| v.display(&self.gc))
                        .unwrap_or_else(|| "assertion failed".to_string());
                    return Err(VMError::new(&format!("assertion failed: {}", msg)));
                }
                Ok(Value::null())
            }
            "assert_eq" => {
                if args.len() < 2 {
                    return Err(VMError::new("assert_eq() requires 2 arguments"));
                }
                if !args[0].equals(&args[1], &self.gc) {
                    let left = args[0].display(&self.gc);
                    let right = args[1].display(&self.gc);
                    return Err(VMError::new(&format!(
                        "assertion failed: expected `{}`, got `{}`",
                        right, left
                    )));
                }
                Ok(Value::null())
            }
            "assert_ne" => {
                if args.len() < 2 {
                    return Err(VMError::new("assert_ne() requires 2 arguments"));
                }
                if args[0].equals(&args[1], &self.gc) {
                    let left = args[0].display(&self.gc);
                    return Err(VMError::new(&format!(
                        "assertion failed: expected values to differ, both were `{}`",
                        left
                    )));
                }
                Ok(Value::null())
            }
            "any" => {
                if args.len() < 2 {
                    return Err(VMError::new("any() requires (array, function)"));
                }
                let items = if let Some(r) = args[0].as_obj() {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("any() first arg must be array"));
                        }
                    } else {
                        return Err(VMError::new("null array"));
                    }
                } else {
                    return Err(VMError::new("any() first arg must be array"));
                };
                let func = args[1].clone();
                for item in items {
                    if self
                        .call_value(func.clone(), vec![item])?
                        .is_truthy(&self.gc)
                    {
                        return Ok(Value::bool_val(true));
                    }
                }
                Ok(Value::bool_val(false))
            }
            "all" => {
                if args.len() < 2 {
                    return Err(VMError::new("all() requires (array, function)"));
                }
                let items = if let Some(r) = args[0].as_obj() {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("all() first arg must be array"));
                        }
                    } else {
                        return Err(VMError::new("null array"));
                    }
                } else {
                    return Err(VMError::new("all() first arg must be array"));
                };
                let func = args[1].clone();
                for item in items {
                    if !self
                        .call_value(func.clone(), vec![item])?
                        .is_truthy(&self.gc)
                    {
                        return Ok(Value::bool_val(false));
                    }
                }
                Ok(Value::bool_val(true))
            }
            "unique" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    let items = if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("unique() requires an array"));
                        }
                    } else {
                        return Err(VMError::new("null array"));
                    };
                    let mut seen: Vec<String> = Vec::new();
                    let mut out = Vec::new();
                    for item in items {
                        let key = item.display(&self.gc);
                        if !seen.contains(&key) {
                            seen.push(key);
                            out.push(item);
                        }
                    }
                    let r = self.gc.alloc(ObjKind::Array(out));
                    return Ok(Value::obj(r));
                }
                Err(VMError::new("unique() requires an array"))
            }
            "sum" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    let items = if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("sum() requires an array"));
                        }
                    } else {
                        return Err(VMError::new("null array"));
                    };
                    let mut total_int: i64 = 0;
                    let mut total_float: f64 = 0.0;
                    let mut is_float = false;
                    for item in &items {
                        match item.classify(&self.gc) {
                            ValueKind::Int(n) => {
                                total_int += n;
                                total_float += n as f64;
                            }
                            ValueKind::Float(n) => {
                                total_float += n;
                                is_float = true;
                            }
                            _ => return Err(VMError::new("sum() requires array of numbers")),
                        }
                    }
                    return Ok(if is_float {
                        Value::float(total_float)
                    } else {
                        Value::int(total_int, &mut self.gc)
                    });
                }
                Err(VMError::new("sum() requires an array"))
            }
            "min_of" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    let items = if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("min_of() requires an array"));
                        }
                    } else {
                        return Err(VMError::new("null array"));
                    };
                    if items.is_empty() {
                        return Ok(Value::null());
                    }
                    let mut min = items[0].clone();
                    for item in &items[1..] {
                        let less = match (min.classify(&self.gc), item.classify(&self.gc)) {
                            (ValueKind::Int(a), ValueKind::Int(b)) => b < a,
                            (ValueKind::Float(a), ValueKind::Float(b)) => b < a,
                            (ValueKind::Int(a), ValueKind::Float(b)) => b < (a as f64),
                            (ValueKind::Float(a), ValueKind::Int(b)) => (b as f64) < a,
                            _ => false,
                        };
                        if less {
                            min = item.clone();
                        }
                    }
                    return Ok(min);
                }
                Err(VMError::new("min_of() requires an array"))
            }
            "max_of" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    let items = if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("max_of() requires an array"));
                        }
                    } else {
                        return Err(VMError::new("null array"));
                    };
                    if items.is_empty() {
                        return Ok(Value::null());
                    }
                    let mut max = items[0].clone();
                    for item in &items[1..] {
                        let greater = match (max.classify(&self.gc), item.classify(&self.gc)) {
                            (ValueKind::Int(a), ValueKind::Int(b)) => b > a,
                            (ValueKind::Float(a), ValueKind::Float(b)) => b > a,
                            (ValueKind::Int(a), ValueKind::Float(b)) => b > (a as f64),
                            (ValueKind::Float(a), ValueKind::Int(b)) => (b as f64) > a,
                            _ => false,
                        };
                        if greater {
                            max = item.clone();
                        }
                    }
                    return Ok(max);
                }
                Err(VMError::new("max_of() requires an array"))
            }
            "map" => {
                if args.len() != 2 {
                    return Err(VMError::new("map() requires (array, function)"));
                }
                let items = if let Some(r) = args[0].as_obj() {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("map() first arg must be array"));
                        }
                    } else {
                        return Err(VMError::new("null array"));
                    }
                } else {
                    return Err(VMError::new("map() first arg must be array"));
                };
                let func = args[1].clone();
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.call_value(func.clone(), vec![item])?);
                }
                let r = self.gc.alloc(ObjKind::Array(out));
                Ok(Value::obj(r))
            }
            "filter" => {
                if args.len() != 2 {
                    return Err(VMError::new("filter() requires (array, function)"));
                }
                let items = if let Some(r) = args[0].as_obj() {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("filter() first arg must be array"));
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    return Err(VMError::new("filter() first arg must be array"));
                };
                let func = args[1].clone();
                let mut out = Vec::new();
                for item in items {
                    let keep = self.call_value(func.clone(), vec![item.clone()])?;
                    if keep.is_truthy(&self.gc) {
                        out.push(item);
                    }
                }
                let r = self.gc.alloc(ObjKind::Array(out));
                Ok(Value::obj(r))
            }
            "reduce" => {
                if args.len() != 3 {
                    return Err(VMError::new("reduce() requires (array, initial, function)"));
                }
                let items = if let Some(r) = args[0].as_obj() {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("reduce() first arg must be array"));
                        }
                    } else {
                        Vec::new()
                    }
                } else {
                    return Err(VMError::new("reduce() first arg must be array"));
                };
                let mut acc = args[1].clone();
                let func = args[2].clone();
                for item in items {
                    acc = self.call_value(func.clone(), vec![acc, item])?;
                }
                Ok(acc)
            }
            "sort" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    let items_clone = if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            Some(a.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(items) = items_clone {
                        // Optional custom comparator (second arg)
                        if let Some(func) = args.get(1).cloned() {
                            let mut sorted = items;
                            let mut err: Option<VMError> = None;
                            sorted.sort_by(|a, b| {
                                if err.is_some() {
                                    return std::cmp::Ordering::Equal;
                                }
                                match self.call_value(func.clone(), vec![a.clone(), b.clone()]) {
                                    Ok(ref v) => match v.as_int(&self.gc) {
                                        Some(n) => {
                                            if n < 0 {
                                                std::cmp::Ordering::Less
                                            } else if n > 0 {
                                                std::cmp::Ordering::Greater
                                            } else {
                                                std::cmp::Ordering::Equal
                                            }
                                        }
                                        None => std::cmp::Ordering::Equal,
                                    },
                                    Err(e) => {
                                        err = Some(e);
                                        std::cmp::Ordering::Equal
                                    }
                                }
                            });
                            if let Some(e) = err {
                                return Err(e);
                            }
                            let nr = self.gc.alloc(ObjKind::Array(sorted));
                            return Ok(Value::obj(nr));
                        }
                        // Default sort: ints, floats, strings
                        let mut sorted = items;
                        sorted.sort_by(|a, b| match (a.classify(&self.gc), b.classify(&self.gc)) {
                            (ValueKind::Int(x), ValueKind::Int(y)) => x.cmp(&y),
                            (ValueKind::Float(x), ValueKind::Float(y)) => {
                                x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (ValueKind::Obj(rx), ValueKind::Obj(ry)) => {
                                let sx = self.get_string(&Value::obj(rx)).unwrap_or_default();
                                let sy = self.get_string(&Value::obj(ry)).unwrap_or_default();
                                sx.cmp(&sy)
                            }
                            _ => std::cmp::Ordering::Equal,
                        });
                        let nr = self.gc.alloc(ObjKind::Array(sorted));
                        return Ok(Value::obj(nr));
                    }
                }
                Err(VMError::new("sort() requires an array"))
            }
            "reverse" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            let mut rev = items.clone();
                            rev.reverse();
                            let nr = self.gc.alloc(ObjKind::Array(rev));
                            return Ok(Value::obj(nr));
                        }
                    }
                }
                Err(VMError::new("reverse() requires an array"))
            }
            "contains" => match (args.first().and_then(|v| v.as_obj()), args.get(1)) {
                (Some(r), Some(val)) => {
                    if let Some(obj) = self.gc.get(r) {
                        match &obj.kind {
                            ObjKind::String(s) => {
                                let sub = val.display(&self.gc);
                                return Ok(Value::bool_val(s.contains(&sub)));
                            }
                            ObjKind::Set(items) => {
                                // Sets use value equality (same as .has()) so that
                                // `contains(s, 1)` and `s.has(1)` agree.
                                let items = items.clone();
                                let found = items.iter().any(|v| v.set_eq(val, &self.gc));
                                return Ok(Value::bool_val(found));
                            }
                            ObjKind::Array(items) | ObjKind::Tuple(items) => {
                                let found = items
                                    .iter()
                                    .any(|v| v.display(&self.gc) == val.display(&self.gc));
                                return Ok(Value::bool_val(found));
                            }
                            _ => {}
                        }
                    }
                    Ok(Value::bool_val(false))
                }
                _ => Err(VMError::new("contains() requires (collection, value)")),
            },
            "keys" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            // Collect keys as owned Strings first to release gc borrow
                            let key_strings: Vec<String> = map.keys().cloned().collect();
                            let _ = obj; // release gc borrow before alloc_string calls
                            let keys: Vec<Value> =
                                key_strings.iter().map(|k| self.alloc_string(k)).collect();
                            let nr = self.gc.alloc(ObjKind::Array(keys));
                            return Ok(Value::obj(nr));
                        }
                    }
                }
                Err(VMError::new("keys() requires an object"))
            }
            "values" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            let vals: Vec<Value> = map.values().cloned().collect();
                            let nr = self.gc.alloc(ObjKind::Array(vals));
                            return Ok(Value::obj(nr));
                        }
                    }
                }
                Err(VMError::new("values() requires an object"))
            }
            "enumerate" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    let items_clone: Option<Vec<Value>> = if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            Some(items.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(items) = items_clone {
                        let mut pairs = Vec::new();
                        for (idx, item) in items.iter().enumerate() {
                            let mut row = IndexMap::new();
                            row.insert("index".to_string(), Value::small_int(idx as i64));
                            row.insert("value".to_string(), item.clone());
                            let rr = self.gc.alloc(ObjKind::Object(row));
                            pairs.push(Value::obj(rr));
                        }
                        let nr = self.gc.alloc(ObjKind::Array(pairs));
                        return Ok(Value::obj(nr));
                    }
                }
                Err(VMError::new("enumerate() requires an array"))
            }
            "split" => {
                if let (Some(r1), Some(r2)) = (
                    args.first().and_then(|v| v.as_obj()),
                    args.get(1).and_then(|v| v.as_obj()),
                ) {
                    let s = self.get_string(&Value::obj(r1)).unwrap_or_default();
                    let delim = self.get_string(&Value::obj(r2)).unwrap_or_default();
                    // Parity with interpreter: empty delimiter splits into individual chars
                    let parts: Vec<Value> = if delim.is_empty() {
                        s.chars()
                            .map(|c| self.alloc_string(&c.to_string()))
                            .collect()
                    } else {
                        s.split(&delim).map(|p| self.alloc_string(p)).collect()
                    };
                    let nr = self.gc.alloc(ObjKind::Array(parts));
                    return Ok(Value::obj(nr));
                }
                Err(VMError::new("split() requires (string, delimiter)"))
            }
            "join" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            let sep = args.get(1).map(|v| v.display(&self.gc)).unwrap_or_default();
                            let parts: Vec<String> =
                                items.iter().map(|v| v.display(&self.gc)).collect();
                            return Ok(self.alloc_string(&parts.join(&sep)));
                        }
                    }
                }
                Err(VMError::new("join() requires an array"))
            }
            "replace" => {
                if args.len() == 3 {
                    let s = args[0].display(&self.gc);
                    let from = args[1].display(&self.gc);
                    let to = args[2].display(&self.gc);
                    return Ok(self.alloc_string(&s.replace(&from, &to)));
                }
                Err(VMError::new("replace() requires (string, from, to)"))
            }
            "starts_with" => {
                if args.len() == 2 {
                    let s = args[0].display(&self.gc);
                    let prefix = args[1].display(&self.gc);
                    return Ok(Value::bool_val(s.starts_with(&prefix)));
                }
                Err(VMError::new("starts_with() requires (string, prefix)"))
            }
            "ends_with" => {
                if args.len() == 2 {
                    let s = args[0].display(&self.gc);
                    let suffix = args[1].display(&self.gc);
                    return Ok(Value::bool_val(s.ends_with(&suffix)));
                }
                Err(VMError::new("ends_with() requires (string, suffix)"))
            }
            "wait" => match args.first().map(|v| v.classify(&self.gc)) {
                Some(ValueKind::Int(secs)) => {
                    self.sleep_with_timeout_checks(std::time::Duration::from_secs(
                        secs.max(0) as u64
                    ))?;
                    Ok(Value::null())
                }
                Some(ValueKind::Float(secs)) => {
                    self.sleep_with_timeout_checks(std::time::Duration::from_secs_f64(
                        secs.max(0.0),
                    ))?;
                    Ok(Value::null())
                }
                _ => Err(VMError::new("wait() requires a number of seconds")),
            },
            "uuid" => {
                let id = uuid::Uuid::new_v4().to_string();
                Ok(self.alloc_string(&id))
            }
            "json" => {
                if let Some(v) = args.first() {
                    let s = v.to_json_string(&self.gc);
                    Ok(self.alloc_string(&s))
                } else {
                    Err(VMError::new("json() requires an argument"))
                }
            }
            "is_some" => {
                match args.first().map(|v| v.classify(&self.gc)) {
                    // Native Option encoding via ADT object
                    Some(ValueKind::Obj(r)) => {
                        if let Some(obj) = self.gc.get(r) {
                            if let ObjKind::Object(map) = &obj.kind {
                                // Check __type__ == "Option" and __variant__ == "Some"
                                let is_option = map
                                    .get("__type__")
                                    .and_then(|v| self.get_string(v))
                                    .map(|s| s == "Option")
                                    .unwrap_or(false);
                                if is_option {
                                    let variant = map
                                        .get("__variant__")
                                        .and_then(|v| self.get_string(v))
                                        .unwrap_or_default();
                                    return Ok(Value::bool_val(variant == "Some"));
                                }
                                // Non-Option object is truthy → Some
                                return Ok(Value::bool_val(true));
                            }
                        }
                        Ok(Value::bool_val(true)) // non-null Obj is Some
                    }
                    Some(ValueKind::Null) => Ok(Value::bool_val(false)),
                    Some(_) => Ok(Value::bool_val(true)),
                    None => Err(VMError::new("is_some() requires an argument")),
                }
            }
            "is_none" => {
                match args.first().map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Obj(r)) => {
                        if let Some(obj) = self.gc.get(r) {
                            if let ObjKind::Object(map) = &obj.kind {
                                let is_option = map
                                    .get("__type__")
                                    .and_then(|v| self.get_string(v))
                                    .map(|s| s == "Option")
                                    .unwrap_or(false);
                                if is_option {
                                    let variant = map
                                        .get("__variant__")
                                        .and_then(|v| self.get_string(v))
                                        .unwrap_or_default();
                                    return Ok(Value::bool_val(variant == "None"));
                                }
                                return Ok(Value::bool_val(false)); // non-Option object is Some
                            }
                        }
                        Ok(Value::bool_val(false)) // non-null Obj is Some
                    }
                    Some(ValueKind::Null) => Ok(Value::bool_val(true)),
                    Some(_) => Ok(Value::bool_val(false)),
                    None => Err(VMError::new("is_none() requires an argument")),
                }
            }
            "satisfies" => {
                if args.len() != 2 {
                    return Err(VMError::new("satisfies() requires (value, interface)"));
                }
                let method_names = self.interface_method_names(&args[1]);
                if method_names.is_empty() {
                    return Ok(Value::bool_val(false));
                }

                let structural = if let Some(map) = self.get_object_fields(&args[0]) {
                    method_names.iter().all(|method_name| {
                        map.get(method_name)
                            .is_some_and(|value| self.is_callable_value(value))
                    })
                } else {
                    false
                };
                if structural {
                    return Ok(Value::bool_val(true));
                }

                if let Some(type_name) = self.value_type_name(&args[0]) {
                    if let Some(type_methods) = self.method_tables.get(&type_name) {
                        let all_satisfied = method_names
                            .iter()
                            .all(|method_name| type_methods.contains_key(method_name));
                        return Ok(Value::bool_val(all_satisfied));
                    }
                }
                Ok(Value::bool_val(false))
            }
            n if n.starts_with("math.") => {
                crate::stdlib::math::call_vm(n, &args, &self.gc).map_err(|e| VMError::new(&e))
            }
            n if n.starts_with("fs.") => {
                let result =
                    crate::stdlib::fs::call_vm(n, &args, &self.gc).map_err(|e| VMError::new(&e))?;
                match result {
                    crate::stdlib::fs::FsResult::StringVal(s) => Ok(self.alloc_string(&s)),
                    crate::stdlib::fs::FsResult::BoolVal(b) => Ok(Value::bool_val(b)),
                    crate::stdlib::fs::FsResult::ArrayVal(items) => {
                        let vals: Vec<Value> = items.iter().map(|s| self.alloc_string(s)).collect();
                        let r = self.gc.alloc(ObjKind::Array(vals));
                        Ok(Value::obj(r))
                    }
                    crate::stdlib::fs::FsResult::NullVal => Ok(Value::null()),
                }
            }
            n if n.starts_with("io.") => {
                crate::stdlib::io::call_vm(n, &args, &self.gc).map_err(|e| VMError::new(&e))
            }
            n if n.starts_with("crypto.") => {
                let str_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(obj) = self.gc.get(r) {
                                if let ObjKind::String(s) = &obj.kind {
                                    return crate::interpreter::Value::String(s.clone());
                                }
                            }
                            crate::interpreter::Value::Null
                        }
                        ValueKind::Int(n) => crate::interpreter::Value::Int(n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::crypto::call(n, str_args).map_err(|e| VMError::new(&e))?;
                match result {
                    crate::interpreter::Value::String(s) => Ok(self.alloc_string(&s)),
                    _ => Ok(Value::null()),
                }
            }
            n if n.starts_with("db.") => {
                let str_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(obj) = self.gc.get(r) {
                                if let ObjKind::String(s) = &obj.kind {
                                    return crate::interpreter::Value::String(s.clone());
                                }
                            }
                            crate::interpreter::Value::Null
                        }
                        ValueKind::Int(n) => crate::interpreter::Value::Int(n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result = crate::stdlib::db::call(n, str_args).map_err(|e| VMError::new(&e))?;
                match result {
                    crate::interpreter::Value::Bool(b) => Ok(Value::bool_val(b)),
                    crate::interpreter::Value::Int(n) => Ok(Value::int(n, &mut self.gc)),
                    crate::interpreter::Value::String(s) => Ok(self.alloc_string(&s)),
                    crate::interpreter::Value::Array(items) => {
                        let vm_items: Vec<Value> = items
                            .iter()
                            .map(|v| match v {
                                crate::interpreter::Value::Object(map) => {
                                    let mut vm_map = IndexMap::new();
                                    for (k, v) in map {
                                        let vm_v = match v {
                                            crate::interpreter::Value::Int(n) => {
                                                Value::int(*n, &mut self.gc)
                                            }
                                            crate::interpreter::Value::Float(n) => Value::float(*n),
                                            crate::interpreter::Value::String(s) => {
                                                self.alloc_string(s)
                                            }
                                            crate::interpreter::Value::Bool(b) => {
                                                Value::bool_val(*b)
                                            }
                                            _ => Value::null(),
                                        };
                                        vm_map.insert(k.clone(), vm_v);
                                    }
                                    let r = self.gc.alloc(ObjKind::Object(vm_map));
                                    Value::obj(r)
                                }
                                _ => Value::null(),
                            })
                            .collect();
                        let r = self.gc.alloc(ObjKind::Array(vm_items));
                        Ok(Value::obj(r))
                    }
                    _ => Ok(Value::null()),
                }
            }
            n if n.starts_with("adt:") => {
                let parts: Vec<&str> = n.splitn(4, ':').collect();
                if parts.len() == 4 {
                    let type_name = parts[1];
                    let variant_name = parts[2];
                    let field_count: usize = parts[3].parse().unwrap_or(0);
                    if args.len() != field_count {
                        return Err(VMError::new(&format!(
                            "{}() expects {} args, got {}",
                            variant_name,
                            field_count,
                            args.len()
                        )));
                    }
                    let mut obj = IndexMap::new();
                    obj.insert("__type__".to_string(), self.alloc_string(type_name));
                    obj.insert("__variant__".to_string(), self.alloc_string(variant_name));
                    for (i, arg) in args.into_iter().enumerate() {
                        obj.insert(format!("_{}", i), arg);
                    }
                    let r = self.gc.alloc(ObjKind::Object(obj));
                    Ok(Value::obj(r))
                } else {
                    Err(VMError::new(&format!("invalid ADT constructor: {}", n)))
                }
            }
            "fetch" => match args.first().map(|v| v.classify(&self.gc)) {
                Some(ValueKind::Obj(r)) => {
                    let url = self.get_string(&Value::obj(r)).unwrap_or_default();
                    let method = "GET".to_string();
                    match crate::runtime::client::fetch_blocking(
                        &url, &method, None, None, None, None, None,
                    ) {
                        Ok(interp_val) => Ok(self.convert_interp_value(&interp_val)),
                        Err(e) => Err(VMError::new(&format!("fetch error: {}", e))),
                    }
                }
                _ => Err(VMError::new("fetch() requires a URL string")),
            },
            "exit" => {
                let code = match args.first().map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Int(n)) => n as i32,
                    _ => 0,
                };
                std::process::exit(code);
            }
            "run_command" => {
                crate::permissions::check_run_permission().map_err(|e| VMError::new(&e))?;
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::obj(r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        ValueKind::Int(n) => crate::interpreter::Value::Int(n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::exec_module::call(interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("os.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::obj(r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::os_module::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("path.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::obj(r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        ValueKind::Int(n) => crate::interpreter::Value::Int(n),
                        ValueKind::Float(n) => crate::interpreter::Value::Float(n),
                        ValueKind::Bool(b) => crate::interpreter::Value::Bool(b),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result = crate::stdlib::path_module::call(n, interp_args)
                    .map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("env.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::obj(r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::env::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("json.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::obj(r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        ValueKind::Int(n) => crate::interpreter::Value::Int(n),
                        ValueKind::Float(n) => crate::interpreter::Value::Float(n),
                        ValueKind::Bool(b) => crate::interpreter::Value::Bool(b),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result = crate::stdlib::json_module::call(n, interp_args)
                    .map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("regex.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::obj(r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result = crate::stdlib::regex_module::call(n, interp_args)
                    .map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("log.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::obj(r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        ValueKind::Int(n) => crate::interpreter::Value::Int(n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                crate::stdlib::log::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(Value::null())
            }
            n if n.starts_with("http.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::obj(r)) {
                                crate::interpreter::Value::String(s)
                            } else if let Some(obj) = self.gc.get(r) {
                                if let ObjKind::Object(map) = &obj.kind {
                                    let mut im = indexmap::IndexMap::new();
                                    for (k, val) in map {
                                        im.insert(k.clone(), self.convert_to_interp_val(val));
                                    }
                                    crate::interpreter::Value::Object(im)
                                } else {
                                    crate::interpreter::Value::Null
                                }
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        ValueKind::Int(n) => crate::interpreter::Value::Int(n),
                        ValueKind::Float(n) => crate::interpreter::Value::Float(n),
                        ValueKind::Bool(b) => crate::interpreter::Value::Bool(b),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::http::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("term.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::obj(r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        ValueKind::Int(n) => crate::interpreter::Value::Int(n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::term::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("csv.") => {
                let interp_args = self.args_to_interp(&args);
                let result =
                    crate::stdlib::csv::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("time.") => {
                let interp_args = self.args_to_interp(&args);
                let result =
                    crate::stdlib::time::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            #[cfg(feature = "postgres")]
            n if n.starts_with("pg.") => {
                let interp_args = self.args_to_interp(&args);
                let result =
                    crate::stdlib::pg::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("jwt.") => {
                let interp_args = self.args_to_interp(&args);
                let result =
                    crate::stdlib::jwt::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            #[cfg(feature = "mysql")]
            n if n.starts_with("mysql.") => {
                let interp_args = self.args_to_interp(&args);
                let result =
                    crate::stdlib::mysql::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("npc.") => {
                let interp_args = self.args_to_interp(&args);
                let result =
                    crate::stdlib::npc::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("url.") => {
                let interp_args = self.args_to_interp(&args);
                let result = crate::stdlib::url_module::call(n, interp_args)
                    .map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("toml.") => {
                let interp_args = self.args_to_interp(&args);
                let result = crate::stdlib::toml_module::call(n, interp_args)
                    .map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("ws.") => {
                let interp_args = self.args_to_interp(&args);
                let result =
                    crate::stdlib::ws::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            "shell" => {
                crate::permissions::check_run_permission().map_err(|e| VMError::new(&e))?;
                let cmd = self.get_string_arg(&args, 0)?;
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| VMError::new(&format!("shell error: {}", e)))?;
                let stdout = String::from_utf8_lossy(&output.stdout)
                    .trim_end()
                    .to_string();
                let stderr = String::from_utf8_lossy(&output.stderr)
                    .trim_end()
                    .to_string();
                let mut map = IndexMap::new();
                map.insert("stdout".to_string(), self.alloc_string(&stdout));
                map.insert("stderr".to_string(), self.alloc_string(&stderr));
                map.insert(
                    "status".to_string(),
                    Value::small_int(output.status.code().unwrap_or(-1) as i64),
                );
                map.insert("ok".to_string(), Value::bool_val(output.status.success()));
                let r = self.gc.alloc(ObjKind::Object(map));
                Ok(Value::obj(r))
            }
            "sh" => {
                crate::permissions::check_run_permission().map_err(|e| VMError::new(&e))?;
                let cmd = self.get_string_arg(&args, 0)?;
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| VMError::new(&format!("sh error: {}", e)))?;
                Ok(self.alloc_string(
                    &String::from_utf8_lossy(&output.stdout)
                        .trim_end()
                        .to_string(),
                ))
            }
            "sh_lines" => {
                crate::permissions::check_run_permission().map_err(|e| VMError::new(&e))?;
                let cmd = self.get_string_arg(&args, 0)?;
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| VMError::new(&format!("sh_lines error: {}", e)))?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let lines: Vec<Value> = stdout
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| self.alloc_string(l))
                    .collect();
                let r = self.gc.alloc(ObjKind::Array(lines));
                Ok(Value::obj(r))
            }
            "sh_json" => {
                crate::permissions::check_run_permission().map_err(|e| VMError::new(&e))?;
                let cmd = self.get_string_arg(&args, 0)?;
                let output = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .output()
                    .map_err(|e| VMError::new(&format!("sh_json error: {}", e)))?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let json: serde_json::Value = serde_json::from_str(stdout.trim())
                    .map_err(|e| VMError::new(&format!("sh_json parse error: {}", e)))?;
                let interp_val = crate::runtime::server::json_to_forge(json);
                Ok(self.convert_interp_value(&interp_val))
            }
            "sh_ok" => {
                crate::permissions::check_run_permission().map_err(|e| VMError::new(&e))?;
                let cmd = self.get_string_arg(&args, 0)?;
                let status = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map_err(|e| VMError::new(&format!("sh_ok error: {}", e)))?;
                Ok(Value::bool_val(status.success()))
            }
            "which" => {
                let cmd = self.get_string_arg(&args, 0)?;
                let result = std::process::Command::new("/usr/bin/which")
                    .arg(&cmd)
                    .output();
                match result {
                    Ok(output) if output.status.success() => Ok(self
                        .alloc_string(&String::from_utf8_lossy(&output.stdout).trim().to_string())),
                    _ => Ok(Value::null()),
                }
            }
            "cwd" => {
                let path = std::env::current_dir()
                    .map_err(|e| VMError::new(&format!("cwd error: {}", e)))?;
                Ok(self.alloc_string(&path.display().to_string()))
            }
            "cd" => {
                let path = self.get_string_arg(&args, 0)?;
                std::env::set_current_dir(&path)
                    .map_err(|e| VMError::new(&format!("cd error: {}", e)))?;
                Ok(self.alloc_string(&path))
            }
            "lines" => {
                let text = self.get_string_arg(&args, 0)?;
                let result: Vec<Value> = text.lines().map(|l| self.alloc_string(l)).collect();
                let r = self.gc.alloc(ObjKind::Array(result));
                Ok(Value::obj(r))
            }
            "pipe_to" => {
                crate::permissions::check_run_permission().map_err(|e| VMError::new(&e))?;
                let input = self.get_string_arg(&args, 0)?;
                let cmd = self.get_string_arg(&args, 1)?;
                use std::io::Write;
                let mut child = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| VMError::new(&format!("pipe_to error: {}", e)))?;
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(input.as_bytes());
                }
                let output = child
                    .wait_with_output()
                    .map_err(|e| VMError::new(&format!("pipe_to error: {}", e)))?;
                let mut map = IndexMap::new();
                map.insert(
                    "stdout".to_string(),
                    self.alloc_string(
                        &String::from_utf8_lossy(&output.stdout)
                            .trim_end()
                            .to_string(),
                    ),
                );
                map.insert(
                    "stderr".to_string(),
                    self.alloc_string(
                        &String::from_utf8_lossy(&output.stderr)
                            .trim_end()
                            .to_string(),
                    ),
                );
                map.insert(
                    "status".to_string(),
                    Value::small_int(output.status.code().unwrap_or(-1) as i64),
                );
                map.insert("ok".to_string(), Value::bool_val(output.status.success()));
                let r = self.gc.alloc(ObjKind::Object(map));
                Ok(Value::obj(r))
            }
            "has_key" => {
                if let (Some(r), Some(key_val)) =
                    (args.first().and_then(|v| v.as_obj()), args.get(1))
                {
                    let key = self.get_string(key_val).unwrap_or_default();
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            return Ok(Value::bool_val(map.contains_key(&key)));
                        }
                    }
                }
                Ok(Value::bool_val(false))
            }
            "get" => {
                if let (Some(r), Some(key_val)) =
                    (args.first().and_then(|v| v.as_obj()), args.get(1))
                {
                    let key = self.get_string(key_val).unwrap_or_default();
                    let default = args.get(2).cloned().unwrap_or(Value::null());
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            if key.contains('.') {
                                let parts: Vec<&str> = key.split('.').collect();
                                let mut current_map = map.clone();
                                for (i, part) in parts.iter().enumerate() {
                                    if let Some(val) = current_map.get(*part) {
                                        if i == parts.len() - 1 {
                                            return Ok(val.clone());
                                        }
                                        if let Some(inner_r) = val.as_obj() {
                                            if let Some(inner_obj) = self.gc.get(inner_r) {
                                                if let ObjKind::Object(inner_map) = &inner_obj.kind
                                                {
                                                    current_map = inner_map.clone();
                                                    continue;
                                                }
                                            }
                                        }
                                        return Ok(default);
                                    } else {
                                        return Ok(default);
                                    }
                                }
                            }
                            return Ok(map.get(&key).cloned().unwrap_or(default));
                        }
                    }
                    Ok(default)
                } else {
                    Ok(Value::null())
                }
            }
            "pick" => {
                if let (Some(r), Some(keys_r)) = (
                    args.first().and_then(|v| v.as_obj()),
                    args.get(1).and_then(|v| v.as_obj()),
                ) {
                    let mut result = IndexMap::new();
                    let field_names: Vec<String> = if let Some(obj) = self.gc.get(keys_r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            items.iter().filter_map(|v| self.get_string(v)).collect()
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            for name in &field_names {
                                if let Some(val) = map.get(name) {
                                    result.insert(name.clone(), val.clone());
                                }
                            }
                        }
                    }
                    let r = self.gc.alloc(ObjKind::Object(result));
                    Ok(Value::obj(r))
                } else {
                    Ok(Value::null())
                }
            }
            "omit" => {
                if let (Some(r), Some(keys_r)) = (
                    args.first().and_then(|v| v.as_obj()),
                    args.get(1).and_then(|v| v.as_obj()),
                ) {
                    let omit_names: Vec<String> = if let Some(obj) = self.gc.get(keys_r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            items.iter().filter_map(|v| self.get_string(v)).collect()
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };
                    let mut result = IndexMap::new();
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            for (k, v) in map {
                                if !omit_names.contains(k) {
                                    result.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                    let r = self.gc.alloc(ObjKind::Object(result));
                    Ok(Value::obj(r))
                } else {
                    Ok(Value::null())
                }
            }
            "merge" => {
                let mut result = IndexMap::new();
                for arg in &args {
                    if let Some(r) = arg.as_obj() {
                        if let Some(obj) = self.gc.get(r) {
                            if let ObjKind::Object(map) = &obj.kind {
                                for (k, v) in map {
                                    result.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                }
                let r = self.gc.alloc(ObjKind::Object(result));
                Ok(Value::obj(r))
            }
            "entries" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    let kv_pairs: Vec<(String, Value)> = if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };
                    // Return [] for empty objects (parity with interpreter — not Null)
                    let mut pairs = Vec::new();
                    for (k, v) in kv_pairs {
                        let key = self.alloc_string(&k);
                        let pair_r = self.gc.alloc(ObjKind::Array(vec![key, v]));
                        pairs.push(Value::obj(pair_r));
                    }
                    let r = self.gc.alloc(ObjKind::Array(pairs));
                    return Ok(Value::obj(r));
                }
                Err(VMError::new("entries() requires an object"))
            }
            "from_entries" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(pairs) = &obj.kind {
                            let mut result = IndexMap::new();
                            let pairs_clone = pairs.clone();
                            for pair in &pairs_clone {
                                if let Some(pr) = pair.as_obj() {
                                    if let Some(pobj) = self.gc.get(pr) {
                                        if let ObjKind::Array(kv) = &pobj.kind {
                                            if let (Some(k), Some(v)) = (kv.first(), kv.get(1)) {
                                                if let Some(key) = self.get_string(k) {
                                                    result.insert(key, v.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            let r = self.gc.alloc(ObjKind::Object(result));
                            return Ok(Value::obj(r));
                        }
                    }
                }
                Ok(Value::null())
            }
            "find" => {
                // find(array, predicate) -> first matching element or Null
                if args.len() < 2 {
                    return Err(VMError::new("find() requires (array, function)"));
                }
                let items = if let Some(r) = args[0].as_obj() {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("find() first arg must be array"));
                        }
                    } else {
                        return Err(VMError::new("null array"));
                    }
                } else {
                    return Err(VMError::new("find() first arg must be array"));
                };
                let func = args[1].clone();
                for item in items {
                    let result = self.call_value(func.clone(), vec![item.clone()])?;
                    if result.is_truthy(&self.gc) {
                        return Ok(item);
                    }
                }
                Ok(Value::null())
            }
            "flat_map" => {
                // flat_map(array, function) -> flattened array
                if args.len() < 2 {
                    return Err(VMError::new("flat_map() requires (array, function)"));
                }
                let items = if let Some(r) = args[0].as_obj() {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("flat_map() first arg must be array"));
                        }
                    } else {
                        return Err(VMError::new("null array"));
                    }
                } else {
                    return Err(VMError::new("flat_map() first arg must be array"));
                };
                let func = args[1].clone();
                let mut out = Vec::new();
                for item in items {
                    let result = self.call_value(func.clone(), vec![item])?;
                    match result.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(obj) = self.gc.get(r) {
                                if let ObjKind::Array(sub) = &obj.kind {
                                    out.extend(sub.clone());
                                    continue;
                                }
                            }
                            out.push(Value::obj(r));
                        }
                        _ => out.push(result),
                    }
                }
                let r = self.gc.alloc(ObjKind::Array(out));
                Ok(Value::obj(r))
            }
            // ===== typeof (alias for "type") =====
            "typeof" => match args.first() {
                Some(v) => {
                    let name = v.type_name(&self.gc);
                    Ok(self.alloc_string(name))
                }
                None => Err(VMError::new("typeof() requires an argument")),
            },

            // ===== Collection builtins =====
            "first" => {
                let items = self.array_items(
                    args.first()
                        .ok_or_else(|| VMError::new("first() requires an array"))?,
                    "first() requires an array",
                )?;
                Ok(items.first().cloned().unwrap_or(Value::null()))
            }
            "last" => {
                let items = self.array_items(
                    args.first()
                        .ok_or_else(|| VMError::new("last() requires an array"))?,
                    "last() requires an array",
                )?;
                Ok(items.last().cloned().unwrap_or(Value::null()))
            }
            "zip" => {
                if args.len() < 2 {
                    return Err(VMError::new("zip() requires two arrays"));
                }
                let a = self.array_items(&args[0], "zip() first arg must be array")?;
                let b = self.array_items(&args[1], "zip() second arg must be array")?;
                let pairs: Vec<Value> = a
                    .into_iter()
                    .zip(b.into_iter())
                    .map(|(x, y)| {
                        let pair = self.gc.alloc(ObjKind::Array(vec![x, y]));
                        Value::obj(pair)
                    })
                    .collect();
                let r = self.gc.alloc(ObjKind::Array(pairs));
                Ok(Value::obj(r))
            }
            "flatten" => {
                let items = self.array_items(
                    args.first()
                        .ok_or_else(|| VMError::new("flatten() requires an array"))?,
                    "flatten() requires an array",
                )?;
                let mut result = Vec::new();
                for item in items {
                    match item.classify(&self.gc) {
                        ValueKind::Obj(r) => {
                            if let Some(obj) = self.gc.get(r) {
                                if let ObjKind::Array(inner) = &obj.kind {
                                    result.extend(inner.clone());
                                    continue;
                                }
                            }
                            result.push(item);
                        }
                        _ => result.push(item),
                    }
                }
                let r = self.gc.alloc(ObjKind::Array(result));
                Ok(Value::obj(r))
            }
            "chunk" => {
                if args.len() < 2 {
                    return Err(VMError::new("chunk() requires (array, size)"));
                }
                let items = self.array_items(&args[0], "chunk() first arg must be array")?;
                let size = match args[1].classify(&self.gc) {
                    ValueKind::Int(n) if n > 0 => n as usize,
                    _ => return Err(VMError::new("chunk() size must be positive")),
                };
                let chunks: Vec<Value> = items
                    .chunks(size)
                    .map(|c| {
                        let arr = self.gc.alloc(ObjKind::Array(c.to_vec()));
                        Value::obj(arr)
                    })
                    .collect();
                let r = self.gc.alloc(ObjKind::Array(chunks));
                Ok(Value::obj(r))
            }
            "slice" => {
                let first = args
                    .first()
                    .ok_or_else(|| VMError::new("slice() requires an argument"))?;
                // Check if it's a string
                if let Some(s) = self.get_string(first) {
                    let chars: Vec<char> = s.chars().collect();
                    let len = chars.len() as i64;
                    let start = match args.get(1).map(|v| v.classify(&self.gc)) {
                        Some(ValueKind::Int(n)) => {
                            if n < 0 {
                                (len + n).max(0) as usize
                            } else {
                                n as usize
                            }
                        }
                        _ => 0,
                    };
                    let end = match args.get(2).map(|v| v.classify(&self.gc)) {
                        Some(ValueKind::Int(n)) => {
                            if n < 0 {
                                (len + n).max(0) as usize
                            } else {
                                (n as usize).min(chars.len())
                            }
                        }
                        _ => chars.len(),
                    };
                    if start >= end || start >= chars.len() {
                        return Ok(self.alloc_string(""));
                    }
                    return Ok(self.alloc_string(&chars[start..end].iter().collect::<String>()));
                }
                // Array
                let items = self.array_items(first, "slice() requires an array or string")?;
                let len = items.len() as i64;
                let start = match args.get(1).map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Int(n)) => {
                        if n < 0 {
                            (len + n).max(0) as usize
                        } else {
                            n as usize
                        }
                    }
                    _ => 0,
                };
                let end = match args.get(2).map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Int(n)) => {
                        if n < 0 {
                            (len + n).max(0) as usize
                        } else {
                            (n as usize).min(items.len())
                        }
                    }
                    _ => items.len(),
                };
                if start >= end || start >= items.len() {
                    let r = self.gc.alloc(ObjKind::Array(vec![]));
                    return Ok(Value::obj(r));
                }
                let r = self.gc.alloc(ObjKind::Array(items[start..end].to_vec()));
                Ok(Value::obj(r))
            }
            "compact" => {
                let items = self.array_items(
                    args.first()
                        .ok_or_else(|| VMError::new("compact() requires an array"))?,
                    "compact() requires an array",
                )?;
                let filtered: Vec<Value> = items
                    .into_iter()
                    .filter(|v| !v.is_null() && v.as_bool() != Some(false))
                    .collect();
                let r = self.gc.alloc(ObjKind::Array(filtered));
                Ok(Value::obj(r))
            }
            "partition" => {
                if args.len() < 2 {
                    return Err(VMError::new("partition() requires (array, function)"));
                }
                let items = self.array_items(&args[0], "partition() first arg must be array")?;
                let func = args[1].clone();
                let mut matches = Vec::new();
                let mut rest = Vec::new();
                for item in items {
                    let result = self.call_value(func.clone(), vec![item.clone()])?;
                    if result.is_truthy(&self.gc) {
                        matches.push(item);
                    } else {
                        rest.push(item);
                    }
                }
                let matches_r = self.gc.alloc(ObjKind::Array(matches));
                let rest_r = self.gc.alloc(ObjKind::Array(rest));
                let r = self.gc.alloc(ObjKind::Array(vec![
                    Value::obj(matches_r),
                    Value::obj(rest_r),
                ]));
                Ok(Value::obj(r))
            }
            "group_by" => {
                if args.len() < 2 {
                    return Err(VMError::new("group_by() requires (array, function)"));
                }
                let items = self.array_items(&args[0], "group_by() first arg must be array")?;
                let func = args[1].clone();
                let mut groups: IndexMap<String, Vec<Value>> = IndexMap::new();
                for item in items {
                    let key = self.call_value(func.clone(), vec![item.clone()])?;
                    let key_str = key.display(&self.gc);
                    groups.entry(key_str).or_default().push(item);
                }
                let mut result = IndexMap::new();
                for (k, v) in groups {
                    let arr = self.gc.alloc(ObjKind::Array(v));
                    result.insert(k, Value::obj(arr));
                }
                let r = self.gc.alloc(ObjKind::Object(result));
                Ok(Value::obj(r))
            }
            "sort_by" => {
                if args.len() < 2 {
                    return Err(VMError::new("sort_by() requires (array, key_function)"));
                }
                let items = self.array_items(&args[0], "sort_by() first arg must be array")?;
                let key_fn = args[1].clone();
                // Pre-compute keys to avoid calling inside sort closure
                let mut pairs: Vec<(Value, Value)> = Vec::new();
                for item in items {
                    let key = self.call_value(key_fn.clone(), vec![item.clone()])?;
                    pairs.push((key, item));
                }
                pairs.sort_by(|(ka, _), (kb, _)| {
                    match (ka.classify(&self.gc), kb.classify(&self.gc)) {
                        (ValueKind::Int(a), ValueKind::Int(b)) => a.cmp(&b),
                        (ValueKind::Float(a), ValueKind::Float(b)) => {
                            a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
                        }
                        (ValueKind::Int(a), ValueKind::Float(b)) => (a as f64)
                            .partial_cmp(&b)
                            .unwrap_or(std::cmp::Ordering::Equal),
                        (ValueKind::Float(a), ValueKind::Int(b)) => a
                            .partial_cmp(&(b as f64))
                            .unwrap_or(std::cmp::Ordering::Equal),
                        _ => {
                            let sa = ka.display(&self.gc);
                            let sb = kb.display(&self.gc);
                            sa.cmp(&sb)
                        }
                    }
                });
                let sorted: Vec<Value> = pairs.into_iter().map(|(_, v)| v).collect();
                let r = self.gc.alloc(ObjKind::Array(sorted));
                Ok(Value::obj(r))
            }
            "for_each" => {
                if args.len() < 2 {
                    return Err(VMError::new("for_each() requires (array, function)"));
                }
                let items = self.array_items(&args[0], "for_each() first arg must be array")?;
                let func = args[1].clone();
                for item in items {
                    self.call_value(func.clone(), vec![item])?;
                }
                Ok(Value::null())
            }
            "take_n" => {
                if args.len() < 2 {
                    return Err(VMError::new("take_n() requires (array, count)"));
                }
                let items = self.array_items(&args[0], "take_n() first arg must be array")?;
                let n = match args[1].classify(&self.gc) {
                    ValueKind::Int(n) => (n.max(0) as usize).min(items.len()),
                    _ => return Err(VMError::new("take_n() second arg must be int")),
                };
                let r = self.gc.alloc(ObjKind::Array(items[..n].to_vec()));
                Ok(Value::obj(r))
            }
            "skip" => {
                if args.len() < 2 {
                    return Err(VMError::new("skip() requires (array, count)"));
                }
                let items = self.array_items(&args[0], "skip() first arg must be array")?;
                let n = match args[1].classify(&self.gc) {
                    ValueKind::Int(n) => (n.max(0) as usize).min(items.len()),
                    _ => return Err(VMError::new("skip() second arg must be int")),
                };
                let r = self.gc.alloc(ObjKind::Array(items[n..].to_vec()));
                Ok(Value::obj(r))
            }
            "frequencies" => {
                let items = self.array_items(
                    args.first()
                        .ok_or_else(|| VMError::new("frequencies() requires an array"))?,
                    "frequencies() requires an array",
                )?;
                let mut counts: IndexMap<String, Value> = IndexMap::new();
                for item in &items {
                    let key = item.display(&self.gc);
                    let count = counts
                        .get(&key)
                        .and_then(|v| {
                            if let Some(n) = v.as_int(&self.gc) {
                                Some(n)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    counts.insert(key, Value::small_int(count + 1));
                }
                let r = self.gc.alloc(ObjKind::Object(counts));
                Ok(Value::obj(r))
            }

            // ===== String builtins =====
            "substring" => {
                let s = self.get_string_arg(&args, 0)?;
                let start = match args.get(1).map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Int(n)) => n as usize,
                    _ => return Err(VMError::new("substring() requires (string, start, end?)")),
                };
                let chars: Vec<char> = s.chars().collect();
                let end = match args.get(2).map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Int(n)) => (n as usize).min(chars.len()),
                    _ => chars.len(),
                };
                if start > chars.len() {
                    return Ok(self.alloc_string(""));
                }
                Ok(self.alloc_string(&chars[start..end].iter().collect::<String>()))
            }
            "index_of" => {
                let first = args
                    .first()
                    .ok_or_else(|| VMError::new("index_of() requires an argument"))?;
                // String case
                if let Some(s) = self.get_string(first) {
                    let substr = self.get_string_arg(&args, 1)?;
                    return Ok(Value::small_int(
                        s.find(&substr).map(|i| i as i64).unwrap_or(-1),
                    ));
                }
                // Array case
                let items = self.array_items(first, "index_of() requires a string or array")?;
                let needle = args
                    .get(1)
                    .ok_or_else(|| VMError::new("index_of() requires 2 arguments"))?;
                let idx = items.iter().position(|v| v.equals(needle, &self.gc));
                Ok(Value::small_int(idx.map(|i| i as i64).unwrap_or(-1)))
            }
            "last_index_of" => {
                let s = self.get_string_arg(&args, 0)?;
                let substr = self.get_string_arg(&args, 1)?;
                Ok(Value::small_int(
                    s.rfind(&substr).map(|i| i as i64).unwrap_or(-1),
                ))
            }
            "capitalize" => {
                let s = self.get_string_arg(&args, 0)?;
                let mut chars = s.chars();
                let result = match chars.next() {
                    Some(c) => {
                        let upper: String = c.to_uppercase().collect();
                        let rest: String = chars.collect::<String>().to_lowercase();
                        format!("{}{}", upper, rest)
                    }
                    None => String::new(),
                };
                Ok(self.alloc_string(&result))
            }
            "title" => {
                let s = self.get_string_arg(&args, 0)?;
                let result = s
                    .split_whitespace()
                    .map(|word| {
                        let mut chars = word.chars();
                        match chars.next() {
                            Some(c) => {
                                let upper: String = c.to_uppercase().collect();
                                let rest: String = chars.collect::<String>().to_lowercase();
                                format!("{}{}", upper, rest)
                            }
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(self.alloc_string(&result))
            }
            "upper" => {
                let s = self.get_string_arg(&args, 0)?;
                Ok(self.alloc_string(&s.to_uppercase()))
            }
            "lower" => {
                let s = self.get_string_arg(&args, 0)?;
                Ok(self.alloc_string(&s.to_lowercase()))
            }
            "trim" => {
                let s = self.get_string_arg(&args, 0)?;
                Ok(self.alloc_string(s.trim()))
            }
            "pad_start" => {
                if args.len() < 2 {
                    return Err(VMError::new("pad_start() requires (string, length)"));
                }
                let s = self.get_string_arg(&args, 0)?;
                let target = match args[1].classify(&self.gc) {
                    ValueKind::Int(n) => n as usize,
                    _ => return Err(VMError::new("pad_start() second arg must be int")),
                };
                let pad_char = match args.get(2) {
                    Some(v) => self
                        .get_string(v)
                        .and_then(|c| c.chars().next())
                        .unwrap_or(' '),
                    _ => ' ',
                };
                let char_count = s.chars().count();
                if char_count >= target {
                    Ok(self.alloc_string(&s))
                } else {
                    let padding: String = std::iter::repeat(pad_char)
                        .take(target - char_count)
                        .collect();
                    Ok(self.alloc_string(&format!("{}{}", padding, s)))
                }
            }
            "pad_end" => {
                if args.len() < 2 {
                    return Err(VMError::new("pad_end() requires (string, length)"));
                }
                let s = self.get_string_arg(&args, 0)?;
                let target = match args[1].classify(&self.gc) {
                    ValueKind::Int(n) => n as usize,
                    _ => return Err(VMError::new("pad_end() second arg must be int")),
                };
                let pad_char = match args.get(2) {
                    Some(v) => self
                        .get_string(v)
                        .and_then(|c| c.chars().next())
                        .unwrap_or(' '),
                    _ => ' ',
                };
                let char_count = s.chars().count();
                if char_count >= target {
                    Ok(self.alloc_string(&s))
                } else {
                    let padding: String = std::iter::repeat(pad_char)
                        .take(target - char_count)
                        .collect();
                    Ok(self.alloc_string(&format!("{}{}", s, padding)))
                }
            }
            "repeat_str" => {
                if args.len() < 2 {
                    return Err(VMError::new("repeat_str() requires (string, count)"));
                }
                let s = self.get_string_arg(&args, 0)?;
                let n = match args[1].classify(&self.gc) {
                    ValueKind::Int(n) if n >= 0 => n as usize,
                    ValueKind::Int(_) => {
                        return Err(VMError::new("repeat_str() count must be non-negative"))
                    }
                    _ => return Err(VMError::new("repeat_str() second arg must be int")),
                };
                Ok(self.alloc_string(&s.repeat(n)))
            }
            "count" => {
                if args.len() < 2 {
                    return Err(VMError::new("count() requires (string, substring)"));
                }
                let s = self.get_string_arg(&args, 0)?;
                let substr = self.get_string_arg(&args, 1)?;
                if substr.is_empty() {
                    return Ok(Value::small_int((s.chars().count() + 1) as i64));
                }
                Ok(Value::small_int(s.matches(&*substr).count() as i64))
            }
            "slugify" => {
                let s = self.get_string_arg(&args, 0)?;
                let slug: String = s
                    .to_lowercase()
                    .chars()
                    .map(|c| if c.is_alphanumeric() { c } else { '-' })
                    .collect::<String>()
                    .split('-')
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<&str>>()
                    .join("-");
                Ok(self.alloc_string(&slug))
            }
            "snake_case" => {
                let s = self.get_string_arg(&args, 0)?;
                let chars: Vec<char> = s.chars().collect();
                let mut result = String::new();
                for i in 0..chars.len() {
                    let c = chars[i];
                    if c.is_uppercase() {
                        if i > 0 {
                            let prev = chars[i - 1];
                            if prev.is_lowercase() || prev.is_numeric() {
                                result.push('_');
                            } else if prev.is_uppercase()
                                && i + 1 < chars.len()
                                && chars[i + 1].is_lowercase()
                            {
                                result.push('_');
                            }
                        }
                        result.push(c.to_lowercase().next().unwrap_or(c));
                    } else if c == ' ' || c == '-' {
                        result.push('_');
                    } else {
                        result.push(c);
                    }
                }
                Ok(self.alloc_string(&result))
            }
            "camel_case" => {
                let s = self.get_string_arg(&args, 0)?;
                let parts: Vec<&str> = s
                    .split(|c: char| c == '_' || c == ' ' || c == '-')
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut result = String::new();
                for (i, part) in parts.iter().enumerate() {
                    if i == 0 {
                        result.push_str(&part.to_lowercase());
                    } else {
                        let mut chars = part.chars();
                        if let Some(first) = chars.next() {
                            result.push(first.to_uppercase().next().unwrap_or(first));
                            result.push_str(&chars.as_str().to_lowercase());
                        }
                    }
                }
                Ok(self.alloc_string(&result))
            }

            // ===== Medium priority builtins =====
            "sample" => {
                let items = self.array_items(
                    args.first()
                        .ok_or_else(|| VMError::new("sample() requires an array"))?,
                    "sample() requires an array",
                )?;
                let n = match args.get(1).map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Int(n)) => n as usize,
                    _ => 1,
                };
                if items.is_empty() {
                    let r = self.gc.alloc(ObjKind::Array(vec![]));
                    return Ok(Value::obj(r));
                }
                use std::time::{SystemTime, UNIX_EPOCH};
                let seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as u64;
                let mut result = Vec::with_capacity(n);
                for i in 0..n {
                    let mut x = seed.wrapping_add(i as u64);
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    let idx = (x % items.len() as u64) as usize;
                    result.push(items[idx].clone());
                }
                if n == 1 {
                    Ok(result.into_iter().next().unwrap_or(Value::null()))
                } else {
                    let r = self.gc.alloc(ObjKind::Array(result));
                    Ok(Value::obj(r))
                }
            }
            "shuffle" => {
                let mut items = self.array_items(
                    args.first()
                        .ok_or_else(|| VMError::new("shuffle() requires an array"))?,
                    "shuffle() requires an array",
                )?;
                use std::time::{SystemTime, UNIX_EPOCH};
                let mut seed = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos() as u64;
                for i in (1..items.len()).rev() {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    let j = (seed % (i as u64 + 1)) as usize;
                    items.swap(i, j);
                }
                let r = self.gc.alloc(ObjKind::Array(items));
                Ok(Value::obj(r))
            }
            "input" => {
                use std::io::Read as _;
                let mut buffer = String::new();
                std::io::stdin().read_to_string(&mut buffer).ok();
                Ok(self.alloc_string(buffer.trim_end()))
            }
            "unwrap_err" => {
                if let Some(r) = args.first().and_then(|v| v.as_obj()) {
                    if let Some(obj) = self.gc.get(r) {
                        if let ObjKind::ResultErr(v) = &obj.kind {
                            return Ok(self.alloc_string(&v.display(&self.gc)));
                        }
                        if matches!(obj.kind, ObjKind::ResultOk(_)) {
                            return Err(VMError::new("unwrap_err() called on Ok"));
                        }
                    }
                }
                Err(VMError::new("unwrap_err() requires a Result value"))
            }
            "diff" => {
                // Delegate to interpreter for deep diff logic
                if args.len() < 2 {
                    return Err(VMError::new("diff() requires two values to compare"));
                }
                let interp_args = self.args_to_interp(&args);
                // Perform diff using interpreter values directly
                fn diff_interp(
                    a: &crate::interpreter::Value,
                    b: &crate::interpreter::Value,
                ) -> crate::interpreter::Value {
                    if a == b {
                        return crate::interpreter::Value::Null;
                    }
                    match (a, b) {
                        (
                            crate::interpreter::Value::Object(map_a),
                            crate::interpreter::Value::Object(map_b),
                        ) => {
                            let mut changes = IndexMap::new();
                            for (key, val_a) in map_a {
                                if key.starts_with("__") {
                                    continue;
                                }
                                match map_b.get(key) {
                                    Some(val_b) => {
                                        let d = diff_interp(val_a, val_b);
                                        if d != crate::interpreter::Value::Null {
                                            let mut change = IndexMap::new();
                                            change.insert("from".to_string(), val_a.clone());
                                            change.insert("to".to_string(), val_b.clone());
                                            changes.insert(
                                                key.clone(),
                                                crate::interpreter::Value::Object(change),
                                            );
                                        }
                                    }
                                    None => {
                                        let mut change = IndexMap::new();
                                        change.insert("removed".to_string(), val_a.clone());
                                        changes.insert(
                                            key.clone(),
                                            crate::interpreter::Value::Object(change),
                                        );
                                    }
                                }
                            }
                            for (key, val_b) in map_b {
                                if key.starts_with("__") {
                                    continue;
                                }
                                if !map_a.contains_key(key) {
                                    let mut change = IndexMap::new();
                                    change.insert("added".to_string(), val_b.clone());
                                    changes.insert(
                                        key.clone(),
                                        crate::interpreter::Value::Object(change),
                                    );
                                }
                            }
                            if changes.is_empty() {
                                crate::interpreter::Value::Null
                            } else {
                                crate::interpreter::Value::Object(changes)
                            }
                        }
                        _ => {
                            let mut change = IndexMap::new();
                            change.insert("from".to_string(), a.clone());
                            change.insert("to".to_string(), b.clone());
                            crate::interpreter::Value::Object(change)
                        }
                    }
                }
                let result = diff_interp(&interp_args[0], &interp_args[1]);
                Ok(self.convert_interp_value(&result))
            }
            "assert_throws" => {
                if args.is_empty() {
                    return Err(VMError::new("assert_throws() requires a function"));
                }
                let func = args[0].clone();
                match self.call_value(func, vec![]) {
                    Err(_) => Ok(Value::bool_val(true)),
                    Ok(_) => Err(VMError::new(
                        "assertion failed: expected function to throw an error, but it succeeded",
                    )),
                }
            }

            // ===== GenZ debug kit =====
            "sus" => {
                if args.is_empty() {
                    return Err(VMError::new("sus() needs something to inspect, bestie"));
                }
                let val = &args[0];
                let type_str = val.type_name(&self.gc);
                let display = val.display(&self.gc);
                eprintln!(
                    "\x1b[33m\u{1f50d} SUS CHECK:\x1b[0m {} \x1b[2m({})\x1b[0m",
                    display, type_str
                );
                Ok(args.into_iter().next().unwrap_or(Value::null()))
            }
            "bruh" => {
                let msg = args
                    .first()
                    .map(|v| v.display(&self.gc))
                    .unwrap_or_else(|| "something ain't right".to_string());
                Err(VMError::new(&format!("BRUH: {}", msg)))
            }
            "bet" => {
                let condition = match args.first().map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Bool(b)) => b,
                    Some(_) => true,
                    None => return Err(VMError::new("bet() needs a condition, no cap")),
                };
                if condition {
                    Ok(Value::bool_val(true))
                } else {
                    let msg = args
                        .get(1)
                        .map(|v| v.display(&self.gc))
                        .unwrap_or_else(|| "condition was false".to_string());
                    Err(VMError::new(&format!("LOST THE BET: {}", msg)))
                }
            }
            "no_cap" => {
                if args.len() < 2 {
                    return Err(VMError::new("no_cap() needs two values to compare, fr fr"));
                }
                if args[0].equals(&args[1], &self.gc) {
                    Ok(Value::bool_val(true))
                } else {
                    let a = args[0].display(&self.gc);
                    let b = args[1].display(&self.gc);
                    Err(VMError::new(&format!("CAP DETECTED: {} \u{2260} {}", a, b)))
                }
            }
            "ick" => {
                let condition = match args.first().map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Bool(b)) => b,
                    Some(_) => true,
                    None => return Err(VMError::new("ick() needs a condition to reject")),
                };
                if !condition {
                    Ok(Value::bool_val(true))
                } else {
                    let msg = args
                        .get(1)
                        .map(|v| v.display(&self.gc))
                        .unwrap_or_else(|| "that's an ick".to_string());
                    Err(VMError::new(&format!("ICK: {}", msg)))
                }
            }

            // ===== Execution helpers =====
            "cook" => {
                if args.is_empty() {
                    return Err(VMError::new(
                        "cook() needs a function \u{2014} let him cook!",
                    ));
                }
                let func = args[0].clone();
                let start = std::time::Instant::now();
                let result = self.call_value(func, vec![])?;
                let elapsed = start.elapsed();
                let ms = elapsed.as_secs_f64() * 1000.0;
                if ms < 1.0 {
                    eprintln!(
                        "\x1b[32m\u{1f468}\u{200d}\u{1f373} COOKED:\x1b[0m done in {:.2}\u{00b5}s \u{2014} \x1b[2mspeed demon fr\x1b[0m",
                        elapsed.as_secs_f64() * 1_000_000.0
                    );
                } else if ms < 100.0 {
                    eprintln!("\x1b[32m\u{1f468}\u{200d}\u{1f373} COOKED:\x1b[0m done in {:.2}ms \u{2014} \x1b[2mno cap that was fast\x1b[0m", ms);
                } else if ms < 1000.0 {
                    eprintln!("\x1b[33m\u{1f468}\u{200d}\u{1f373} COOKED:\x1b[0m done in {:.0}ms \u{2014} \x1b[2mit's giving adequate\x1b[0m", ms);
                } else {
                    eprintln!("\x1b[31m\u{1f468}\u{200d}\u{1f373} COOKED:\x1b[0m done in {:.2}s \u{2014} \x1b[2mbruh that took a minute\x1b[0m", elapsed.as_secs_f64());
                }
                Ok(result)
            }
            "yolo" => {
                if args.is_empty() {
                    return Err(VMError::new("yolo() needs a function to send it on"));
                }
                let func = args[0].clone();
                match self.call_value(func, vec![]) {
                    Ok(val) => Ok(val),
                    Err(_) => Ok(Value::null()),
                }
            }
            "ghost" => {
                if args.is_empty() {
                    return Err(VMError::new("ghost() needs a function to haunt"));
                }
                let func = args[0].clone();
                let result = self.call_value(func, vec![])?;
                Ok(result)
            }
            "slay" => {
                if args.is_empty() {
                    return Err(VMError::new("slay() needs a function to benchmark"));
                }
                let func = args[0].clone();
                let n = match args.get(1).map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Int(n)) => n as usize,
                    _ => 100,
                };
                let mut times: Vec<f64> = Vec::with_capacity(n);
                let mut last_result = Value::null();
                for _ in 0..n {
                    let start = std::time::Instant::now();
                    last_result = self.call_value(func.clone(), vec![])?;
                    times.push(start.elapsed().as_secs_f64() * 1000.0);
                }
                times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let avg = times.iter().sum::<f64>() / times.len() as f64;
                let min_t = times.first().copied().unwrap_or(0.0);
                let max_t = times.last().copied().unwrap_or(0.0);
                let p99_idx = ((times.len() as f64) * 0.99) as usize;
                let p99 = times
                    .get(p99_idx.min(times.len() - 1))
                    .copied()
                    .unwrap_or(0.0);
                let mut stats = IndexMap::new();
                stats.insert("avg_ms".to_string(), Value::float(avg));
                stats.insert("min_ms".to_string(), Value::float(min_t));
                stats.insert("max_ms".to_string(), Value::float(max_t));
                stats.insert("p99_ms".to_string(), Value::float(p99));
                stats.insert("runs".to_string(), Value::small_int(n as i64));
                stats.insert("result".to_string(), last_result);
                eprintln!(
                    "\x1b[35m\u{1f485} SLAYED:\x1b[0m {}x runs \u{2014} avg {:.3}ms, min {:.3}ms, max {:.3}ms, p99 {:.3}ms",
                    n, avg, min_t, max_t, p99
                );
                let r = self.gc.alloc(ObjKind::Object(stats));
                Ok(Value::obj(r))
            }

            // ----- Channel builtins -----
            "channel" => {
                let (sender, receiver) = match args.first().map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Int(cap)) => {
                        let cap = cap.max(0) as usize;
                        let (tx, rx) = std::sync::mpsc::sync_channel(cap);
                        (VmChannelSender::Bounded(tx), rx)
                    }
                    _ => {
                        let (tx, rx) = std::sync::mpsc::channel();
                        (VmChannelSender::Unbounded(tx), rx)
                    }
                };
                let inner = Arc::new(VmChannelInner {
                    sender: std::sync::Mutex::new(Some(sender)),
                    receiver: std::sync::Mutex::new(Some(receiver)),
                });
                let r = self.gc.alloc(ObjKind::Channel(inner));
                Ok(Value::obj(r))
            }
            "send" => {
                if args.len() < 2 {
                    return Err(VMError::new("send() requires (channel, value)"));
                }
                let ch_arc = self.extract_channel(&args[0])?;
                let shared = value_to_shared(&self.gc, &args[1]);
                let guard = ch_arc.sender.lock().unwrap_or_else(|e| e.into_inner());
                match &*guard {
                    Some(VmChannelSender::Bounded(tx)) => {
                        tx.send(shared)
                            .map_err(|_| VMError::new("channel closed"))?;
                    }
                    Some(VmChannelSender::Unbounded(tx)) => {
                        tx.send(shared)
                            .map_err(|_| VMError::new("channel closed"))?;
                    }
                    None => {
                        return Err(VMError::new("channel closed"));
                    }
                }
                Ok(Value::null())
            }
            "receive" => {
                if args.is_empty() {
                    return Err(VMError::new("receive() requires (channel)"));
                }
                let ch_arc = self.extract_channel(&args[0])?;
                let guard = ch_arc.receiver.lock().unwrap_or_else(|e| e.into_inner());
                match &*guard {
                    Some(rx) => match rx.recv() {
                        Ok(shared) => Ok(shared_to_value(&mut self.gc, &shared)),
                        Err(_) => Ok(Value::null()),
                    },
                    None => Ok(Value::null()),
                }
            }
            "close" => {
                if args.is_empty() {
                    return Err(VMError::new("close() requires (channel)"));
                }
                let ch_arc = self.extract_channel(&args[0])?;
                let mut guard = ch_arc.sender.lock().unwrap_or_else(|e| e.into_inner());
                *guard = None;
                Ok(Value::null())
            }
            "try_send" => {
                if args.len() < 2 {
                    return Err(VMError::new("try_send() requires (channel, value)"));
                }
                let ch_arc = self.extract_channel(&args[0])?;
                let shared = value_to_shared(&self.gc, &args[1]);
                let guard = ch_arc.sender.lock().unwrap_or_else(|e| e.into_inner());
                let ok = match &*guard {
                    Some(VmChannelSender::Bounded(tx)) => tx.try_send(shared).is_ok(),
                    // Unbounded send only fails if receiver is dropped
                    Some(VmChannelSender::Unbounded(tx)) => tx.send(shared).is_ok(),
                    None => false,
                };
                Ok(Value::bool_val(ok))
            }
            "try_receive" => {
                if args.is_empty() {
                    return Err(VMError::new("try_receive() requires (channel)"));
                }
                let ch_arc = self.extract_channel(&args[0])?;
                let guard = ch_arc.receiver.lock().unwrap_or_else(|e| e.into_inner());
                match &*guard {
                    Some(rx) => match rx.try_recv() {
                        Ok(shared) => {
                            let val = shared_to_value(&mut self.gc, &shared);
                            let r = self.gc.alloc(ObjKind::ResultOk(val));
                            Ok(Value::obj(r))
                        }
                        Err(_) => Ok(Value::null()),
                    },
                    None => Ok(Value::null()),
                }
            }
            "select" => {
                if args.is_empty() {
                    return Err(VMError::new("select() requires an array of channels"));
                }
                // Extract channels from the array argument
                let channels: Vec<Arc<VmChannelInner>> = match args[0].classify(&self.gc) {
                    ValueKind::Obj(r) => match self.gc.get(r) {
                        Some(obj) => match &obj.kind {
                            ObjKind::Array(items) => {
                                let mut chs = Vec::with_capacity(items.len());
                                for item in items {
                                    chs.push(self.extract_channel(item)?);
                                }
                                chs
                            }
                            _ => {
                                return Err(VMError::new("select() requires an array of channels"))
                            }
                        },
                        None => return Err(VMError::new("select() requires an array of channels")),
                    },
                    _ => return Err(VMError::new("select() requires an array of channels")),
                };
                if channels.is_empty() {
                    return Ok(Value::null());
                }
                let timeout_ms: Option<u128> = match args.get(1).map(|v| v.classify(&self.gc)) {
                    Some(ValueKind::Int(ms)) => Some((ms).max(0) as u128),
                    Some(ValueKind::Float(ms)) => Some(ms.max(0.0) as u128),
                    _ => None,
                };
                let start = std::time::Instant::now();
                let len = channels.len();
                let mut offset = 0usize;
                loop {
                    let mut all_closed = true;
                    for i in 0..len {
                        let idx = (i + offset) % len;
                        let rx_guard = channels[idx]
                            .receiver
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        if let Some(ref rx) = *rx_guard {
                            match rx.try_recv() {
                                Ok(shared) => {
                                    let val = shared_to_value(&mut self.gc, &shared);
                                    let idx_val = Value::small_int(idx as i64);
                                    let arr = self.gc.alloc(ObjKind::Array(vec![idx_val, val]));
                                    return Ok(Value::obj(arr));
                                }
                                Err(std::sync::mpsc::TryRecvError::Empty) => {
                                    all_closed = false;
                                }
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                    // Sender dropped (channel closed) — treat as closed
                                }
                            }
                        }
                    }
                    if all_closed {
                        return Ok(Value::null());
                    }
                    if let Some(ms) = timeout_ms {
                        if start.elapsed().as_millis() >= ms {
                            return Ok(Value::null());
                        }
                    }
                    offset = (offset + 1) % len;
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }

            "time" => {
                let now = Utc::now();
                let mut m = IndexMap::new();
                m.insert("iso".to_string(), self.alloc_string(&now.to_rfc3339()));
                m.insert(
                    "unix".to_string(),
                    Value::int(now.timestamp(), &mut self.gc),
                );
                m.insert(
                    "unix_ms".to_string(),
                    Value::int(now.timestamp_millis(), &mut self.gc),
                );
                m.insert("year".to_string(), Value::small_int(now.year() as i64));
                m.insert("month".to_string(), Value::small_int(now.month() as i64));
                m.insert("day".to_string(), Value::small_int(now.day() as i64));
                m.insert("hour".to_string(), Value::small_int(now.hour() as i64));
                m.insert("minute".to_string(), Value::small_int(now.minute() as i64));
                m.insert("second".to_string(), Value::small_int(now.second() as i64));
                m.insert(
                    "weekday".to_string(),
                    self.alloc_string(&now.format("%A").to_string()),
                );
                m.insert(
                    "weekday_short".to_string(),
                    self.alloc_string(&now.format("%a").to_string()),
                );
                m.insert(
                    "day_of_year".to_string(),
                    Value::small_int(now.ordinal() as i64),
                );
                m.insert("timezone".to_string(), self.alloc_string("UTC"));
                let r = self.gc.alloc(ObjKind::Object(m));
                Ok(Value::obj(r))
            }
            "await_all" => {
                if args.is_empty() {
                    return Err(VMError::new(
                        "await_all() requires an array of task handles",
                    ));
                }
                // Extract array items, releasing the GC borrow
                let items: Vec<Value> = match args[0].classify(&self.gc) {
                    ValueKind::Obj(r) => match self.gc.get(r) {
                        Some(obj) => match &obj.kind {
                            ObjKind::Array(arr) => arr.clone(),
                            _ => {
                                return Err(VMError::new(
                                    "await_all() requires an array of task handles",
                                ))
                            }
                        },
                        None => {
                            return Err(VMError::new(
                                "await_all() requires an array of task handles",
                            ))
                        }
                    },
                    _ => {
                        return Err(VMError::new(
                            "await_all() requires an array of task handles",
                        ))
                    }
                };
                let mut results = Vec::with_capacity(items.len());
                for item in &items {
                    let maybe_slot = self.extract_task_handle(item);
                    if let Some(slot) = maybe_slot {
                        let (lock, cvar) = &*slot;
                        let mut guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                        while guard.is_none() {
                            guard = cvar.wait(guard).unwrap_or_else(|e| e.into_inner());
                        }
                        let shared = guard.as_ref().cloned().unwrap_or(SharedValue::Null);
                        let val = shared_to_value(&mut self.gc, &shared);
                        // Fail-fast: propagate ResultErr immediately
                        match val.classify(&self.gc) {
                            ValueKind::Obj(r) => {
                                if let Some(obj) = self.gc.get(r) {
                                    if let ObjKind::ResultErr(e) = &obj.kind {
                                        let msg = e.display(&self.gc);
                                        return Err(VMError::new(&format!("task error: {}", msg)));
                                    }
                                    if let ObjKind::ResultOk(v) = &obj.kind {
                                        let unwrapped = *v;
                                        results.push(unwrapped);
                                        continue;
                                    }
                                }
                                results.push(val);
                            }
                            _ => results.push(val),
                        }
                    } else {
                        // Non-task-handle values pass through
                        results.push(*item);
                    }
                }
                let r = self.gc.alloc(ObjKind::Array(results));
                Ok(Value::obj(r))
            }
            "await_timeout" => {
                if args.len() < 2 {
                    return Err(VMError::new(
                        "await_timeout() requires (handle, timeout_ms)",
                    ));
                }
                let timeout_ms = match args[1].classify(&self.gc) {
                    ValueKind::Int(ms) => (ms).max(0) as u64,
                    ValueKind::Float(ms) => ms.max(0.0) as u64,
                    _ => {
                        return Err(VMError::new(
                            "await_timeout() second argument must be a number (ms)",
                        ))
                    }
                };
                let maybe_slot = self.extract_task_handle(&args[0]);
                match maybe_slot {
                    Some(slot) => {
                        let (lock, cvar) = &*slot;
                        let mut guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                        if guard.is_none() {
                            let deadline = std::time::Duration::from_millis(timeout_ms);
                            let start = std::time::Instant::now();
                            loop {
                                let remaining = deadline.saturating_sub(start.elapsed());
                                if remaining.is_zero() {
                                    return Ok(Value::null());
                                }
                                let (g, timeout_result) = cvar
                                    .wait_timeout(guard, remaining)
                                    .unwrap_or_else(|e| e.into_inner());
                                guard = g;
                                if guard.is_some() {
                                    break;
                                }
                                if timeout_result.timed_out() {
                                    return Ok(Value::null());
                                }
                            }
                        }
                        let shared = guard.as_ref().cloned().unwrap_or(SharedValue::Null);
                        let val = shared_to_value(&mut self.gc, &shared);
                        // Unwrap ResultOk, propagate ResultErr
                        match val.classify(&self.gc) {
                            ValueKind::Obj(r) => {
                                if let Some(obj) = self.gc.get(r) {
                                    if let ObjKind::ResultOk(v) = &obj.kind {
                                        return Ok(*v);
                                    }
                                    if let ObjKind::ResultErr(e) = &obj.kind {
                                        let msg = e.display(&self.gc);
                                        return Err(VMError::new(&format!("task error: {}", msg)));
                                    }
                                }
                                Ok(val)
                            }
                            _ => Ok(val),
                        }
                    }
                    None => {
                        return Err(VMError::new(
                            "await_timeout() first argument must be a task handle",
                        ))
                    }
                }
            }

            // Note: lowercase ok/err aliases are handled ABOVE (before "Ok"/"Err") to avoid
            // unreachable pattern warnings. The dead duplicates below have been removed.
            _ => Err(VMError::new(&format!("unknown builtin: {}", name))),
        }
    }

    fn extract_task_handle(
        &self,
        val: &Value,
    ) -> Option<Arc<(std::sync::Mutex<Option<SharedValue>>, std::sync::Condvar)>> {
        match val.classify(&self.gc) {
            ValueKind::Obj(r) => self.gc.get(r).and_then(|obj| {
                if let ObjKind::TaskHandle(slot) = &obj.kind {
                    Some(slot.clone())
                } else {
                    None
                }
            }),
            _ => None,
        }
    }

    fn make_struct_marker(&mut self, type_name: &str) -> Value {
        let mut marker = IndexMap::new();
        marker.insert("__kind__".to_string(), self.alloc_string("struct"));
        marker.insert("name".to_string(), self.alloc_string(type_name));
        let r = self.gc.alloc(ObjKind::Object(marker));
        Value::obj(r)
    }

    fn extract_channel(&self, val: &Value) -> Result<Arc<VmChannelInner>, VMError> {
        match val.classify(&self.gc) {
            ValueKind::Obj(r) => match self.gc.get(r) {
                Some(obj) => match &obj.kind {
                    ObjKind::Channel(ch) => Ok(ch.clone()),
                    _ => Err(VMError::new("expected channel")),
                },
                None => Err(VMError::new("dangling reference")),
            },
            _ => Err(VMError::new("expected channel")),
        }
    }

    fn parse_object_fields(&self, value: &Value) -> Result<IndexMap<String, Value>, VMError> {
        match value.classify(&self.gc) {
            ValueKind::Obj(r) => match self.gc.get(r) {
                Some(obj) => match &obj.kind {
                    ObjKind::Object(map) => Ok(map.clone()),
                    _ => Err(VMError::new("expected object value")),
                },
                None => Err(VMError::new("dangling object reference")),
            },
            ValueKind::Null => Ok(IndexMap::new()),
            _ => Err(VMError::new("expected object value")),
        }
    }

    fn get_object_fields(&self, value: &Value) -> Option<IndexMap<String, Value>> {
        match value.classify(&self.gc) {
            ValueKind::Obj(r) => self.gc.get(r).and_then(|obj| match &obj.kind {
                ObjKind::Object(map) => Some(map.clone()),
                _ => None,
            }),
            _ => None,
        }
    }

    fn array_items(&self, value: &Value, message: &str) -> Result<Vec<Value>, VMError> {
        match value.classify(&self.gc) {
            ValueKind::Obj(r) => match self.gc.get(r) {
                Some(obj) => match &obj.kind {
                    ObjKind::Array(items) => Ok(items.clone()),
                    _ => Err(VMError::new(message)),
                },
                None => Err(VMError::new("dangling array reference")),
            },
            _ => Err(VMError::new(message)),
        }
    }

    fn query_compare(&self, left: &Value, op: &str, right: &Value) -> bool {
        match op {
            "==" => self.query_value_cmp(left, right) == std::cmp::Ordering::Equal,
            "!=" => self.query_value_cmp(left, right) != std::cmp::Ordering::Equal,
            ">" => self.query_value_cmp(left, right) == std::cmp::Ordering::Greater,
            ">=" => {
                let ord = self.query_value_cmp(left, right);
                matches!(ord, std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
            }
            "<" => self.query_value_cmp(left, right) == std::cmp::Ordering::Less,
            "<=" => {
                let ord = self.query_value_cmp(left, right);
                matches!(ord, std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
            }
            _ => false,
        }
    }

    fn query_value_cmp(&self, left: &Value, right: &Value) -> std::cmp::Ordering {
        match (left.classify(&self.gc), right.classify(&self.gc)) {
            (ValueKind::Int(a), ValueKind::Int(b)) => a.cmp(&b),
            (ValueKind::Float(a), ValueKind::Float(b)) => {
                a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
            }
            (ValueKind::Int(a), ValueKind::Float(b)) => (a as f64)
                .partial_cmp(&b)
                .unwrap_or(std::cmp::Ordering::Equal),
            (ValueKind::Float(a), ValueKind::Int(b)) => a
                .partial_cmp(&(b as f64))
                .unwrap_or(std::cmp::Ordering::Equal),
            _ => left.display(&self.gc).cmp(&right.display(&self.gc)),
        }
    }

    fn parse_embedded_fields(&self, value: &Value) -> Result<Vec<(String, String)>, VMError> {
        match value.classify(&self.gc) {
            ValueKind::Obj(r) => match self.gc.get(r) {
                Some(obj) => match &obj.kind {
                    ObjKind::Array(items) => {
                        let mut embeds = Vec::new();
                        for item in items {
                            let fields = self.parse_object_fields(item)?;
                            let field_name = fields
                                .get("field")
                                .and_then(|value| self.get_string(value))
                                .ok_or_else(|| {
                                    VMError::new("embedded field metadata missing field")
                                })?;
                            let type_name = fields
                                .get("type")
                                .and_then(|value| self.get_string(value))
                                .ok_or_else(|| {
                                    VMError::new("embedded field metadata missing type")
                                })?;
                            embeds.push((field_name, type_name));
                        }
                        Ok(embeds)
                    }
                    _ => Err(VMError::new("expected embedded field metadata array")),
                },
                None => Err(VMError::new("dangling embedded field metadata")),
            },
            ValueKind::Null => Ok(Vec::new()),
            _ => Err(VMError::new("expected embedded field metadata array")),
        }
    }

    fn interface_method_names(&self, iface: &Value) -> Vec<String> {
        let Some(fields) = self.get_object_fields(iface) else {
            return Vec::new();
        };
        let Some(method_ref) = fields.get("methods").and_then(|v| v.as_obj()) else {
            return Vec::new();
        };
        let Some(method_obj) = self.gc.get(method_ref) else {
            return Vec::new();
        };
        let ObjKind::Array(methods) = &method_obj.kind else {
            return Vec::new();
        };

        methods
            .iter()
            .filter_map(|method_spec| {
                self.get_object_fields(method_spec)
                    .and_then(|spec| spec.get("name").and_then(|value| self.get_string(value)))
            })
            .collect()
    }

    fn value_type_name(&self, value: &Value) -> Option<String> {
        self.get_object_fields(value).and_then(|fields| {
            fields
                .get("__type__")
                .and_then(|value| self.get_string(value))
        })
    }

    fn value_variant_name(&self, value: &Value) -> Option<String> {
        self.get_object_fields(value).and_then(|fields| {
            fields
                .get("__variant__")
                .and_then(|value| self.get_string(value))
        })
    }

    fn struct_marker_name(&self, value: &Value) -> Option<String> {
        let fields = self.get_object_fields(value)?;
        let kind = fields
            .get("__kind__")
            .and_then(|value| self.get_string(value))?;
        if kind != "struct" {
            return None;
        }
        fields.get("name").and_then(|value| self.get_string(value))
    }

    fn is_callable_value(&self, value: &Value) -> bool {
        match value.classify(&self.gc) {
            ValueKind::Obj(r) => self.gc.get(r).is_some_and(|obj| {
                matches!(
                    obj.kind,
                    ObjKind::Function(_) | ObjKind::Closure(_) | ObjKind::NativeFunction(_)
                )
            }),
            _ => false,
        }
    }

    fn call_forge_method(
        &mut self,
        receiver: Value,
        method_name: &str,
        extra_args: &[Value],
    ) -> Result<Value, VMError> {
        if let Some(fields) = self.get_object_fields(&receiver) {
            if let Some(func) = fields.get(method_name).cloned() {
                return self.call_value(func, extra_args.to_vec());
            }
        }

        if let Some(type_name) = self.struct_marker_name(&receiver) {
            if let Some(func) = self
                .static_methods
                .get(&type_name)
                .and_then(|methods| methods.get(method_name))
                .cloned()
            {
                return self.call_value(func, extra_args.to_vec());
            }
            return Err(VMError::new(&format!(
                "no static method '{}' on {}",
                method_name, type_name
            )));
        }

        if let Some(type_name) = self.value_type_name(&receiver) {
            if let Some(func) = self
                .method_tables
                .get(&type_name)
                .and_then(|methods| methods.get(method_name))
                .cloned()
            {
                let mut full_args = Vec::with_capacity(extra_args.len() + 1);
                full_args.push(receiver.clone());
                full_args.extend(extra_args.iter().cloned());
                return self.call_value(func, full_args);
            }

            if let Some(embed_defs) = self.embedded_fields.get(&type_name).cloned() {
                if let Some(fields) = self.get_object_fields(&receiver) {
                    for (embed_field, embed_type) in embed_defs {
                        let Some(func) = self
                            .method_tables
                            .get(&embed_type)
                            .and_then(|methods| methods.get(method_name))
                            .cloned()
                        else {
                            continue;
                        };
                        let Some(embed_value) = fields.get(&embed_field).cloned() else {
                            continue;
                        };
                        let mut full_args = Vec::with_capacity(extra_args.len() + 1);
                        full_args.push(embed_value);
                        full_args.extend(extra_args.iter().cloned());
                        return self.call_value(func, full_args);
                    }
                }
            }
        }

        // Set-specific methods. Peel `Frozen(Set)` so frozen sets still
        // support read-only methods; mutating methods are rejected on
        // frozen receivers. Clones the items Vec to drop the GC borrow
        // before any further allocation.
        let set_receiver: Option<(Vec<Value>, bool)> = {
            receiver.as_obj().and_then(|r| {
                let obj = self.gc.get(r)?;
                match &obj.kind {
                    ObjKind::Set(items) => Some((items.clone(), false)),
                    ObjKind::Frozen(inner) => {
                        let inner_r = inner.as_obj()?;
                        let inner_obj = self.gc.get(inner_r)?;
                        if let ObjKind::Set(items) = &inner_obj.kind {
                            Some((items.clone(), true))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
        };

        if let Some((items, is_frozen)) = set_receiver {
            // Peel "other" arg too so `a.union(frozen_b)` etc. work. Returns
            // the inner Vec by clone to drop the GC borrow.
            let peel_other = |vm: &Self, v: Value| -> Option<Vec<Value>> {
                let r = v.as_obj()?;
                let obj = vm.gc.get(r)?;
                match &obj.kind {
                    ObjKind::Set(inner) => Some(inner.clone()),
                    ObjKind::Frozen(inner_val) => {
                        let ir = inner_val.as_obj()?;
                        let iobj = vm.gc.get(ir)?;
                        if let ObjKind::Set(inner) = &iobj.kind {
                            Some(inner.clone())
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            };
            match method_name {
                "has" => {
                    if extra_args.len() != 1 {
                        return Err(VMError::new("has() requires one argument"));
                    }
                    let found = items.iter().any(|v| v.set_eq(&extra_args[0], &self.gc));
                    return Ok(Value::bool_val(found));
                }
                "add" => {
                    if is_frozen {
                        return Err(VMError::new("cannot mutate a frozen set"));
                    }
                    if extra_args.len() != 1 {
                        return Err(VMError::new("add() requires one argument"));
                    }
                    let mut new_items = items;
                    if !new_items.iter().any(|v| v.set_eq(&extra_args[0], &self.gc)) {
                        new_items.push(extra_args[0]);
                    }
                    let nr = self.gc.alloc(ObjKind::Set(new_items));
                    return Ok(Value::obj(nr));
                }
                "remove" => {
                    if is_frozen {
                        return Err(VMError::new("cannot mutate a frozen set"));
                    }
                    if extra_args.len() != 1 {
                        return Err(VMError::new("remove() requires one argument"));
                    }
                    let new_items: Vec<Value> = items
                        .into_iter()
                        .filter(|v| !v.set_eq(&extra_args[0], &self.gc))
                        .collect();
                    let nr = self.gc.alloc(ObjKind::Set(new_items));
                    return Ok(Value::obj(nr));
                }
                "union" => {
                    if extra_args.len() != 1 {
                        return Err(VMError::new("union() requires one argument"));
                    }
                    let other = peel_other(self, extra_args[0])
                        .ok_or_else(|| VMError::new("union() requires a set argument"))?;
                    let mut result = items;
                    for v in other {
                        if !result.iter().any(|existing| existing.set_eq(&v, &self.gc)) {
                            result.push(v);
                        }
                    }
                    let nr = self.gc.alloc(ObjKind::Set(result));
                    return Ok(Value::obj(nr));
                }
                "intersect" => {
                    if extra_args.len() != 1 {
                        return Err(VMError::new("intersect() requires one argument"));
                    }
                    let other = peel_other(self, extra_args[0])
                        .ok_or_else(|| VMError::new("intersect() requires a set argument"))?;
                    let result: Vec<Value> = items
                        .into_iter()
                        .filter(|v| other.iter().any(|o| o.set_eq(v, &self.gc)))
                        .collect();
                    let nr = self.gc.alloc(ObjKind::Set(result));
                    return Ok(Value::obj(nr));
                }
                "diff" => {
                    if extra_args.len() != 1 {
                        return Err(VMError::new("diff() requires one argument"));
                    }
                    let other = peel_other(self, extra_args[0])
                        .ok_or_else(|| VMError::new("diff() requires a set argument"))?;
                    let result: Vec<Value> = items
                        .into_iter()
                        .filter(|v| !other.iter().any(|o| o.set_eq(v, &self.gc)))
                        .collect();
                    let nr = self.gc.alloc(ObjKind::Set(result));
                    return Ok(Value::obj(nr));
                }
                "to_array" => {
                    let nr = self.gc.alloc(ObjKind::Array(items));
                    return Ok(Value::obj(nr));
                }
                _ => {}
            }
        }

        if Self::is_builtin_method_name(method_name) {
            if let Some(func) = self.globals.get(method_name).cloned() {
                let mut full_args = Vec::with_capacity(extra_args.len() + 1);
                full_args.push(receiver);
                full_args.extend(extra_args.iter().cloned());
                return self.call_value(func, full_args);
            }
        }

        Err(VMError::new(&format!(
            "no method '{}' on {}",
            method_name,
            receiver.type_name(&self.gc)
        )))
    }

    fn is_builtin_method_name(name: &str) -> bool {
        matches!(
            name,
            "map"
                | "filter"
                | "reduce"
                | "sort"
                | "reverse"
                | "push"
                | "pop"
                | "len"
                | "contains"
                | "keys"
                | "values"
                | "enumerate"
                | "split"
                | "join"
                | "replace"
                | "find"
                | "flat_map"
                | "has_key"
                | "get"
                | "pick"
                | "omit"
                | "merge"
                | "entries"
                | "from_entries"
                | "starts_with"
                | "ends_with"
                | "upper"
                | "lower"
                | "trim"
                | "substring"
                | "index_of"
                | "last_index_of"
                | "pad_start"
                | "pad_end"
                | "capitalize"
                | "title"
                | "repeat_str"
                | "count"
                | "sum"
                | "min_of"
                | "max_of"
                | "any"
                | "all"
                | "unique"
                | "zip"
                | "flatten"
                | "group_by"
                | "chunk"
                | "slice"
                | "slugify"
                | "snake_case"
                | "camel_case"
                | "sample"
                | "shuffle"
                | "partition"
                | "diff"
                | "trim_start"
                | "trim_end"
                | "is_empty"
                | "is_numeric"
                | "is_alpha"
                | "is_alphanumeric"
                | "char_at"
                | "encode_uri"
                | "decode_uri"
                | "words"
                | "bytes"
                | "sort_by"
                | "first"
                | "last"
                | "compact"
                | "take_n"
                | "skip"
                | "frequencies"
                | "for_each"
        )
    }
}
