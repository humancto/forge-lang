/// VM Builtin Dispatch
/// Extracted from machine.rs to keep that file navigable.
/// This is a continuation of `impl VM` — same struct, separate file.
/// Do NOT change logic here; this is a pure structural extraction.
use indexmap::IndexMap;

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
                Ok(Value::Obj(r))
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
            "__forge_register_method" => {
                if args.len() != 4 {
                    return Err(VMError::new(
                        "__forge_register_method() requires (type_name, method_name, has_receiver, function)",
                    ));
                }
                let type_name = self.get_string_arg(&args, 0)?;
                let method_name = self.get_string_arg(&args, 1)?;
                let has_receiver = match args[2] {
                    Value::Bool(flag) => flag,
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

                Ok(Value::Null)
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
                Ok(Value::Null)
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
                    return Ok(Value::Bool(true));
                };

                if let (Some(bound_variant), Some(value_variant)) = (
                    self.value_variant_name(&bound_value),
                    self.value_variant_name(&value),
                ) {
                    return Ok(Value::Bool(bound_variant == value_variant));
                }

                Ok(Value::Bool(true))
            }
            "println" | "say" => {
                let text: Vec<String> = args.iter().map(|v| v.display(&self.gc)).collect();
                let output = text.join(" ");
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            "print" => {
                let text: Vec<String> = args.iter().map(|v| v.display(&self.gc)).collect();
                print!("{}", text.join(" "));
                Ok(Value::Null)
            }
            "yell" => {
                let text: Vec<String> = args.iter().map(|v| v.display(&self.gc)).collect();
                let output = text.join(" ").to_uppercase();
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            "whisper" => {
                let text: Vec<String> = args.iter().map(|v| v.display(&self.gc)).collect();
                let output = text.join(" ").to_lowercase();
                println!("{}", output);
                self.output.push(output);
                Ok(Value::Null)
            }
            "len" => match args.first() {
                Some(v) => {
                    let len = match v {
                        Value::Obj(r) => self.gc.get(*r).map_or(0, |o| match &o.kind {
                            ObjKind::String(s) => s.len() as i64,
                            ObjKind::Array(a) => a.len() as i64,
                            ObjKind::Object(o) => o.len() as i64,
                            _ => 0,
                        }),
                        _ => 0,
                    };
                    Ok(Value::Int(len))
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
            "int" => match args.first() {
                Some(Value::Int(n)) => Ok(Value::Int(*n)),
                Some(Value::Float(n)) => Ok(Value::Int(*n as i64)),
                // Parity with interpreter: bool → 0/1
                Some(Value::Bool(b)) => Ok(Value::Int(if *b { 1 } else { 0 })),
                Some(Value::Obj(r)) => {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::String(s) = &obj.kind {
                            return s.parse::<i64>().map(Value::Int).map_err(|_| {
                                VMError::new(&format!("cannot convert '{}' to Int", s))
                            });
                        }
                    }
                    Err(VMError::new("int() requires number, bool, or string"))
                }
                _ => Err(VMError::new("int() requires number, bool, or string")),
            },
            "float" => match args.first() {
                Some(Value::Int(n)) => Ok(Value::Float(*n as f64)),
                Some(Value::Float(n)) => Ok(Value::Float(*n)),
                // Parity with interpreter: parse string to float
                Some(Value::Obj(r)) => {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::String(s) = &obj.kind {
                            return s.parse::<f64>().map(Value::Float).map_err(|_| {
                                VMError::new(&format!("cannot convert '{}' to Float", s))
                            });
                        }
                    }
                    Err(VMError::new("float() requires a number or numeric string"))
                }
                _ => Err(VMError::new("float() requires a number or numeric string")),
            },
            "range" => match (args.first(), args.get(1)) {
                (Some(Value::Int(start)), Some(Value::Int(end))) => {
                    let items: Vec<Value> = (*start..*end).map(Value::Int).collect();
                    let r = self.gc.alloc(ObjKind::Array(items));
                    Ok(Value::Obj(r))
                }
                (Some(Value::Int(end_val)), None) => {
                    let items: Vec<Value> = (0..*end_val).map(Value::Int).collect();
                    let r = self.gc.alloc(ObjKind::Array(items));
                    Ok(Value::Obj(r))
                }
                _ => Err(VMError::new("range() requires integer arguments")),
            },
            "push" => {
                if args.len() != 2 {
                    return Err(VMError::new("push() requires array and value"));
                }
                if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            let mut new_items = items.clone();
                            new_items.push(args[1].clone());
                            let nr = self.gc.alloc(ObjKind::Array(new_items));
                            return Ok(Value::Obj(nr));
                        }
                    }
                }
                Err(VMError::new("push() requires an array"))
            }
            "pop" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            let mut new_items = items.clone();
                            new_items.pop();
                            let nr = self.gc.alloc(ObjKind::Array(new_items));
                            return Ok(Value::Obj(nr));
                        }
                    }
                }
                Err(VMError::new("pop() requires an array"))
            }
            // Lowercase aliases must come BEFORE the capitalized forms
            // so the match arms are not shadowed ("Ok" would match before "ok" | "Ok")
            "ok" => {
                let val = args.first().cloned().unwrap_or(Value::Null);
                let r = self.gc.alloc(ObjKind::ResultOk(val));
                Ok(Value::Obj(r))
            }
            "err" => {
                let val = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| self.alloc_string("error"));
                let r = self.gc.alloc(ObjKind::ResultErr(val));
                Ok(Value::Obj(r))
            }
            "Ok" | "Some" => {
                let val = args.first().cloned().unwrap_or(Value::Null);
                if name == "Some" {
                    let mut obj = IndexMap::new();
                    obj.insert("__type__".to_string(), self.alloc_string("Option"));
                    obj.insert("__variant__".to_string(), self.alloc_string("Some"));
                    obj.insert("_0".to_string(), val);
                    let r = self.gc.alloc(ObjKind::Object(obj));
                    Ok(Value::Obj(r))
                } else {
                    let r = self.gc.alloc(ObjKind::ResultOk(val));
                    Ok(Value::Obj(r))
                }
            }
            "Err" => {
                let val = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| self.alloc_string("error"));
                let r = self.gc.alloc(ObjKind::ResultErr(val));
                Ok(Value::Obj(r))
            }
            "is_ok" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        return Ok(Value::Bool(matches!(obj.kind, ObjKind::ResultOk(_))));
                    }
                }
                Ok(Value::Bool(false))
            }
            "is_err" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        return Ok(Value::Bool(matches!(obj.kind, ObjKind::ResultErr(_))));
                    }
                }
                Ok(Value::Bool(false))
            }
            "unwrap" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
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
                if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
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
                let cond = args.first().cloned().unwrap_or(Value::Bool(false));
                if !cond.is_truthy(&self.gc) {
                    let msg = args
                        .get(1)
                        .map(|v| v.display(&self.gc))
                        .unwrap_or_else(|| "assertion failed".to_string());
                    return Err(VMError::new(&format!("assertion failed: {}", msg)));
                }
                Ok(Value::Null)
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
                Ok(Value::Null)
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
                Ok(Value::Null)
            }
            "any" => {
                if args.len() < 2 {
                    return Err(VMError::new("any() requires (array, function)"));
                }
                let items = if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
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
                        return Ok(Value::Bool(true));
                    }
                }
                Ok(Value::Bool(false))
            }
            "all" => {
                if args.len() < 2 {
                    return Err(VMError::new("all() requires (array, function)"));
                }
                let items = if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
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
                        return Ok(Value::Bool(false));
                    }
                }
                Ok(Value::Bool(true))
            }
            "unique" => {
                if let Some(Value::Obj(r)) = args.first() {
                    let items = if let Some(obj) = self.gc.get(*r) {
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
                    return Ok(Value::Obj(r));
                }
                Err(VMError::new("unique() requires an array"))
            }
            "sum" => {
                if let Some(Value::Obj(r)) = args.first() {
                    let items = if let Some(obj) = self.gc.get(*r) {
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
                        match item {
                            Value::Int(n) => {
                                total_int += n;
                                total_float += *n as f64;
                            }
                            Value::Float(n) => {
                                total_float += n;
                                is_float = true;
                            }
                            _ => return Err(VMError::new("sum() requires array of numbers")),
                        }
                    }
                    return Ok(if is_float {
                        Value::Float(total_float)
                    } else {
                        Value::Int(total_int)
                    });
                }
                Err(VMError::new("sum() requires an array"))
            }
            "min_of" => {
                if let Some(Value::Obj(r)) = args.first() {
                    let items = if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("min_of() requires an array"));
                        }
                    } else {
                        return Err(VMError::new("null array"));
                    };
                    if items.is_empty() {
                        return Ok(Value::Null);
                    }
                    let mut min = items[0].clone();
                    for item in &items[1..] {
                        let less = match (&min, item) {
                            (Value::Int(a), Value::Int(b)) => b < a,
                            (Value::Float(a), Value::Float(b)) => b < a,
                            (Value::Int(a), Value::Float(b)) => b < &(*a as f64),
                            (Value::Float(a), Value::Int(b)) => (*b as f64) < *a,
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
                if let Some(Value::Obj(r)) = args.first() {
                    let items = if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(a) = &obj.kind {
                            a.clone()
                        } else {
                            return Err(VMError::new("max_of() requires an array"));
                        }
                    } else {
                        return Err(VMError::new("null array"));
                    };
                    if items.is_empty() {
                        return Ok(Value::Null);
                    }
                    let mut max = items[0].clone();
                    for item in &items[1..] {
                        let greater = match (&max, item) {
                            (Value::Int(a), Value::Int(b)) => b > a,
                            (Value::Float(a), Value::Float(b)) => b > a,
                            (Value::Int(a), Value::Float(b)) => b > &(*a as f64),
                            (Value::Float(a), Value::Int(b)) => (*b as f64) > *a,
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
                let items = if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
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
                Ok(Value::Obj(r))
            }
            "filter" => {
                if args.len() != 2 {
                    return Err(VMError::new("filter() requires (array, function)"));
                }
                let items = if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
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
                Ok(Value::Obj(r))
            }
            "reduce" => {
                if args.len() != 3 {
                    return Err(VMError::new("reduce() requires (array, initial, function)"));
                }
                let items = if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
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
                if let Some(Value::Obj(r)) = args.first() {
                    let items_clone = if let Some(obj) = self.gc.get(*r) {
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
                                    Ok(Value::Int(n)) => {
                                        if n < 0 {
                                            std::cmp::Ordering::Less
                                        } else if n > 0 {
                                            std::cmp::Ordering::Greater
                                        } else {
                                            std::cmp::Ordering::Equal
                                        }
                                    }
                                    Ok(_) => std::cmp::Ordering::Equal,
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
                            return Ok(Value::Obj(nr));
                        }
                        // Default sort: ints, floats, strings
                        let mut sorted = items;
                        sorted.sort_by(|a, b| match (a, b) {
                            (Value::Int(x), Value::Int(y)) => x.cmp(y),
                            (Value::Float(x), Value::Float(y)) => {
                                x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (Value::Obj(rx), Value::Obj(ry)) => {
                                let sx = self.get_string(&Value::Obj(*rx)).unwrap_or_default();
                                let sy = self.get_string(&Value::Obj(*ry)).unwrap_or_default();
                                sx.cmp(&sy)
                            }
                            _ => std::cmp::Ordering::Equal,
                        });
                        let nr = self.gc.alloc(ObjKind::Array(sorted));
                        return Ok(Value::Obj(nr));
                    }
                }
                Err(VMError::new("sort() requires an array"))
            }
            "reverse" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            let mut rev = items.clone();
                            rev.reverse();
                            let nr = self.gc.alloc(ObjKind::Array(rev));
                            return Ok(Value::Obj(nr));
                        }
                    }
                }
                Err(VMError::new("reverse() requires an array"))
            }
            "contains" => match (args.first(), args.get(1)) {
                (Some(Value::Obj(r)), Some(val)) => {
                    if let Some(obj) = self.gc.get(*r) {
                        match &obj.kind {
                            ObjKind::String(s) => {
                                let sub = val.display(&self.gc);
                                return Ok(Value::Bool(s.contains(&sub)));
                            }
                            ObjKind::Array(items) => {
                                let found = items
                                    .iter()
                                    .any(|v| v.display(&self.gc) == val.display(&self.gc));
                                return Ok(Value::Bool(found));
                            }
                            _ => {}
                        }
                    }
                    Ok(Value::Bool(false))
                }
                _ => Err(VMError::new("contains() requires (collection, value)")),
            },
            "keys" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            // Collect keys as owned Strings first to release gc borrow
                            let key_strings: Vec<String> = map.keys().cloned().collect();
                            drop(obj); // release gc borrow before alloc_string calls
                            let keys: Vec<Value> =
                                key_strings.iter().map(|k| self.alloc_string(k)).collect();
                            let nr = self.gc.alloc(ObjKind::Array(keys));
                            return Ok(Value::Obj(nr));
                        }
                    }
                }
                Err(VMError::new("keys() requires an object"))
            }
            "values" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            let vals: Vec<Value> = map.values().cloned().collect();
                            let nr = self.gc.alloc(ObjKind::Array(vals));
                            return Ok(Value::Obj(nr));
                        }
                    }
                }
                Err(VMError::new("values() requires an object"))
            }
            "enumerate" => {
                if let Some(Value::Obj(r)) = args.first() {
                    let items_clone: Option<Vec<Value>> = if let Some(obj) = self.gc.get(*r) {
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
                            row.insert("index".to_string(), Value::Int(idx as i64));
                            row.insert("value".to_string(), item.clone());
                            let rr = self.gc.alloc(ObjKind::Object(row));
                            pairs.push(Value::Obj(rr));
                        }
                        let nr = self.gc.alloc(ObjKind::Array(pairs));
                        return Ok(Value::Obj(nr));
                    }
                }
                Err(VMError::new("enumerate() requires an array"))
            }
            "split" => {
                if let (Some(Value::Obj(r1)), Some(Value::Obj(r2))) = (args.first(), args.get(1)) {
                    let s = self.get_string(&Value::Obj(*r1)).unwrap_or_default();
                    let delim = self.get_string(&Value::Obj(*r2)).unwrap_or_default();
                    // Parity with interpreter: empty delimiter splits into individual chars
                    let parts: Vec<Value> = if delim.is_empty() {
                        s.chars()
                            .map(|c| self.alloc_string(&c.to_string()))
                            .collect()
                    } else {
                        s.split(&delim).map(|p| self.alloc_string(p)).collect()
                    };
                    let nr = self.gc.alloc(ObjKind::Array(parts));
                    return Ok(Value::Obj(nr));
                }
                Err(VMError::new("split() requires (string, delimiter)"))
            }
            "join" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
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
                    return Ok(Value::Bool(s.starts_with(&prefix)));
                }
                Err(VMError::new("starts_with() requires (string, prefix)"))
            }
            "ends_with" => {
                if args.len() == 2 {
                    let s = args[0].display(&self.gc);
                    let suffix = args[1].display(&self.gc);
                    return Ok(Value::Bool(s.ends_with(&suffix)));
                }
                Err(VMError::new("ends_with() requires (string, suffix)"))
            }
            "wait" => {
                if let Some(Value::Int(secs)) = args.first() {
                    std::thread::sleep(std::time::Duration::from_secs(*secs as u64));
                }
                Ok(Value::Null)
            }
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
                match args.first() {
                    // Native Option encoding via ADT object
                    Some(Value::Obj(r)) => {
                        if let Some(obj) = self.gc.get(*r) {
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
                                    return Ok(Value::Bool(variant == "Some"));
                                }
                                // Non-Option object is truthy → Some
                                return Ok(Value::Bool(true));
                            }
                        }
                        Ok(Value::Bool(true)) // non-null Obj is Some
                    }
                    Some(Value::Null) => Ok(Value::Bool(false)),
                    Some(_) => Ok(Value::Bool(true)),
                    None => Err(VMError::new("is_some() requires an argument")),
                }
            }
            "is_none" => {
                match args.first() {
                    Some(Value::Obj(r)) => {
                        if let Some(obj) = self.gc.get(*r) {
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
                                    return Ok(Value::Bool(variant == "None"));
                                }
                                return Ok(Value::Bool(false)); // non-Option object is Some
                            }
                        }
                        Ok(Value::Bool(false)) // non-null Obj is Some
                    }
                    Some(Value::Null) => Ok(Value::Bool(true)),
                    Some(_) => Ok(Value::Bool(false)),
                    None => Err(VMError::new("is_none() requires an argument")),
                }
            }
            "satisfies" => {
                if args.len() != 2 {
                    return Err(VMError::new("satisfies() requires (value, interface)"));
                }
                let method_names = self.interface_method_names(&args[1]);
                if method_names.is_empty() {
                    return Ok(Value::Bool(false));
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
                    return Ok(Value::Bool(true));
                }

                if let Some(type_name) = self.value_type_name(&args[0]) {
                    if let Some(type_methods) = self.method_tables.get(&type_name) {
                        let all_satisfied = method_names
                            .iter()
                            .all(|method_name| type_methods.contains_key(method_name));
                        return Ok(Value::Bool(all_satisfied));
                    }
                }
                Ok(Value::Bool(false))
            }
            n if n.starts_with("math.") => {
                crate::stdlib::math::call_vm(n, &args, &self.gc).map_err(|e| VMError::new(&e))
            }
            n if n.starts_with("fs.") => {
                let result =
                    crate::stdlib::fs::call_vm(n, &args, &self.gc).map_err(|e| VMError::new(&e))?;
                match result {
                    crate::stdlib::fs::FsResult::StringVal(s) => Ok(self.alloc_string(&s)),
                    crate::stdlib::fs::FsResult::BoolVal(b) => Ok(Value::Bool(b)),
                    crate::stdlib::fs::FsResult::ArrayVal(items) => {
                        let vals: Vec<Value> = items.iter().map(|s| self.alloc_string(s)).collect();
                        let r = self.gc.alloc(ObjKind::Array(vals));
                        Ok(Value::Obj(r))
                    }
                    crate::stdlib::fs::FsResult::NullVal => Ok(Value::Null),
                }
            }
            n if n.starts_with("io.") => {
                crate::stdlib::io::call_vm(n, &args, &self.gc).map_err(|e| VMError::new(&e))
            }
            n if n.starts_with("crypto.") => {
                let str_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(obj) = self.gc.get(*r) {
                                if let ObjKind::String(s) = &obj.kind {
                                    return crate::interpreter::Value::String(s.clone());
                                }
                            }
                            crate::interpreter::Value::Null
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::crypto::call(n, str_args).map_err(|e| VMError::new(&e))?;
                match result {
                    crate::interpreter::Value::String(s) => Ok(self.alloc_string(&s)),
                    _ => Ok(Value::Null),
                }
            }
            n if n.starts_with("db.") => {
                let str_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(obj) = self.gc.get(*r) {
                                if let ObjKind::String(s) = &obj.kind {
                                    return crate::interpreter::Value::String(s.clone());
                                }
                            }
                            crate::interpreter::Value::Null
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result = crate::stdlib::db::call(n, str_args).map_err(|e| VMError::new(&e))?;
                match result {
                    crate::interpreter::Value::Bool(b) => Ok(Value::Bool(b)),
                    crate::interpreter::Value::Int(n) => Ok(Value::Int(n)),
                    crate::interpreter::Value::String(s) => Ok(self.alloc_string(&s)),
                    crate::interpreter::Value::Array(items) => {
                        let vm_items: Vec<Value> = items
                            .iter()
                            .map(|v| match v {
                                crate::interpreter::Value::Object(map) => {
                                    let mut vm_map = IndexMap::new();
                                    for (k, v) in map {
                                        let vm_v = match v {
                                            crate::interpreter::Value::Int(n) => Value::Int(*n),
                                            crate::interpreter::Value::Float(n) => Value::Float(*n),
                                            crate::interpreter::Value::String(s) => {
                                                self.alloc_string(s)
                                            }
                                            crate::interpreter::Value::Bool(b) => Value::Bool(*b),
                                            _ => Value::Null,
                                        };
                                        vm_map.insert(k.clone(), vm_v);
                                    }
                                    let r = self.gc.alloc(ObjKind::Object(vm_map));
                                    Value::Obj(r)
                                }
                                _ => Value::Null,
                            })
                            .collect();
                        let r = self.gc.alloc(ObjKind::Array(vm_items));
                        Ok(Value::Obj(r))
                    }
                    _ => Ok(Value::Null),
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
                    Ok(Value::Obj(r))
                } else {
                    Err(VMError::new(&format!("invalid ADT constructor: {}", n)))
                }
            }
            "fetch" => match args.first() {
                Some(Value::Obj(r)) => {
                    let url = self.get_string(&Value::Obj(*r)).unwrap_or_default();
                    let method = "GET".to_string();
                    match crate::runtime::client::fetch_blocking(&url, &method, None, None, None) {
                        Ok(interp_val) => Ok(self.convert_interp_value(&interp_val)),
                        Err(e) => Err(VMError::new(&format!("fetch error: {}", e))),
                    }
                }
                _ => Err(VMError::new("fetch() requires a URL string")),
            },
            "exit" => {
                let code = match args.first() {
                    Some(Value::Int(n)) => *n as i32,
                    _ => 0,
                };
                std::process::exit(code);
            }
            "run_command" => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                let result =
                    crate::stdlib::exec_module::call(interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            n if n.starts_with("env.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
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
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        Value::Float(n) => crate::interpreter::Value::Float(*n),
                        Value::Bool(b) => crate::interpreter::Value::Bool(*b),
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
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
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
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        _ => crate::interpreter::Value::Null,
                    })
                    .collect();
                crate::stdlib::log::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(Value::Null)
            }
            n if n.starts_with("http.") => {
                let interp_args: Vec<crate::interpreter::Value> = args
                    .iter()
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
                                crate::interpreter::Value::String(s)
                            } else if let Some(obj) = self.gc.get(*r) {
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
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
                        Value::Float(n) => crate::interpreter::Value::Float(*n),
                        Value::Bool(b) => crate::interpreter::Value::Bool(*b),
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
                    .map(|v| match v {
                        Value::Obj(r) => {
                            if let Some(s) = self.get_string(&Value::Obj(*r)) {
                                crate::interpreter::Value::String(s)
                            } else {
                                crate::interpreter::Value::Null
                            }
                        }
                        Value::Int(n) => crate::interpreter::Value::Int(*n),
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
            n if n.starts_with("mysql.") => {
                let interp_args = self.args_to_interp(&args);
                let result =
                    crate::stdlib::mysql::call(n, interp_args).map_err(|e| VMError::new(&e))?;
                Ok(self.convert_interp_value(&result))
            }
            "shell" => {
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
                    Value::Int(output.status.code().unwrap_or(-1) as i64),
                );
                map.insert("ok".to_string(), Value::Bool(output.status.success()));
                let r = self.gc.alloc(ObjKind::Object(map));
                Ok(Value::Obj(r))
            }
            "sh" => {
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
                Ok(Value::Obj(r))
            }
            "sh_json" => {
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
                let cmd = self.get_string_arg(&args, 0)?;
                let status = std::process::Command::new("/bin/sh")
                    .arg("-c")
                    .arg(&cmd)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map_err(|e| VMError::new(&format!("sh_ok error: {}", e)))?;
                Ok(Value::Bool(status.success()))
            }
            "which" => {
                let cmd = self.get_string_arg(&args, 0)?;
                let result = std::process::Command::new("/usr/bin/which")
                    .arg(&cmd)
                    .output();
                match result {
                    Ok(output) if output.status.success() => Ok(self
                        .alloc_string(&String::from_utf8_lossy(&output.stdout).trim().to_string())),
                    _ => Ok(Value::Null),
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
                Ok(Value::Obj(r))
            }
            "pipe_to" => {
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
                    Value::Int(output.status.code().unwrap_or(-1) as i64),
                );
                map.insert("ok".to_string(), Value::Bool(output.status.success()));
                let r = self.gc.alloc(ObjKind::Object(map));
                Ok(Value::Obj(r))
            }
            "has_key" => {
                if let (Some(Value::Obj(r)), Some(key_val)) = (args.first(), args.get(1)) {
                    let key = self.get_string(key_val).unwrap_or_default();
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            return Ok(Value::Bool(map.contains_key(&key)));
                        }
                    }
                }
                Ok(Value::Bool(false))
            }
            "get" => {
                if let (Some(Value::Obj(r)), Some(key_val)) = (args.first(), args.get(1)) {
                    let key = self.get_string(key_val).unwrap_or_default();
                    let default = args.get(2).cloned().unwrap_or(Value::Null);
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            if key.contains('.') {
                                let parts: Vec<&str> = key.split('.').collect();
                                let mut current_map = map.clone();
                                for (i, part) in parts.iter().enumerate() {
                                    if let Some(val) = current_map.get(*part) {
                                        if i == parts.len() - 1 {
                                            return Ok(val.clone());
                                        }
                                        if let Value::Obj(inner_r) = val {
                                            if let Some(inner_obj) = self.gc.get(*inner_r) {
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
                    Ok(Value::Null)
                }
            }
            "pick" => {
                if let (Some(Value::Obj(r)), Some(Value::Obj(keys_r))) = (args.first(), args.get(1))
                {
                    let mut result = IndexMap::new();
                    let field_names: Vec<String> = if let Some(obj) = self.gc.get(*keys_r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            items.iter().filter_map(|v| self.get_string(v)).collect()
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            for name in &field_names {
                                if let Some(val) = map.get(name) {
                                    result.insert(name.clone(), val.clone());
                                }
                            }
                        }
                    }
                    let r = self.gc.alloc(ObjKind::Object(result));
                    Ok(Value::Obj(r))
                } else {
                    Ok(Value::Null)
                }
            }
            "omit" => {
                if let (Some(Value::Obj(r)), Some(Value::Obj(keys_r))) = (args.first(), args.get(1))
                {
                    let omit_names: Vec<String> = if let Some(obj) = self.gc.get(*keys_r) {
                        if let ObjKind::Array(items) = &obj.kind {
                            items.iter().filter_map(|v| self.get_string(v)).collect()
                        } else {
                            vec![]
                        }
                    } else {
                        vec![]
                    };
                    let mut result = IndexMap::new();
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Object(map) = &obj.kind {
                            for (k, v) in map {
                                if !omit_names.contains(k) {
                                    result.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                    let r = self.gc.alloc(ObjKind::Object(result));
                    Ok(Value::Obj(r))
                } else {
                    Ok(Value::Null)
                }
            }
            "merge" => {
                let mut result = IndexMap::new();
                for arg in &args {
                    if let Value::Obj(r) = arg {
                        if let Some(obj) = self.gc.get(*r) {
                            if let ObjKind::Object(map) = &obj.kind {
                                for (k, v) in map {
                                    result.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    }
                }
                let r = self.gc.alloc(ObjKind::Object(result));
                Ok(Value::Obj(r))
            }
            "entries" => {
                if let Some(Value::Obj(r)) = args.first() {
                    let kv_pairs: Vec<(String, Value)> = if let Some(obj) = self.gc.get(*r) {
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
                        pairs.push(Value::Obj(pair_r));
                    }
                    let r = self.gc.alloc(ObjKind::Array(pairs));
                    return Ok(Value::Obj(r));
                }
                Err(VMError::new("entries() requires an object"))
            }
            "from_entries" => {
                if let Some(Value::Obj(r)) = args.first() {
                    if let Some(obj) = self.gc.get(*r) {
                        if let ObjKind::Array(pairs) = &obj.kind {
                            let mut result = IndexMap::new();
                            let pairs_clone = pairs.clone();
                            for pair in &pairs_clone {
                                if let Value::Obj(pr) = pair {
                                    if let Some(pobj) = self.gc.get(*pr) {
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
                            return Ok(Value::Obj(r));
                        }
                    }
                }
                Ok(Value::Null)
            }
            "find" => {
                // find(array, predicate) -> first matching element or Null
                if args.len() < 2 {
                    return Err(VMError::new("find() requires (array, function)"));
                }
                let items = if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
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
                Ok(Value::Null)
            }
            "flat_map" => {
                // flat_map(array, function) -> flattened array
                if args.len() < 2 {
                    return Err(VMError::new("flat_map() requires (array, function)"));
                }
                let items = if let Value::Obj(r) = &args[0] {
                    if let Some(obj) = self.gc.get(*r) {
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
                    match result {
                        Value::Obj(r) => {
                            if let Some(obj) = self.gc.get(r) {
                                if let ObjKind::Array(sub) = &obj.kind {
                                    out.extend(sub.clone());
                                    continue;
                                }
                            }
                            out.push(Value::Obj(r));
                        }
                        other => out.push(other),
                    }
                }
                let r = self.gc.alloc(ObjKind::Array(out));
                Ok(Value::Obj(r))
            }
            // Note: lowercase ok/err aliases are handled ABOVE (before "Ok"/"Err") to avoid
            // unreachable pattern warnings. The dead duplicates below have been removed.
            _ => Err(VMError::new(&format!("unknown builtin: {}", name))),
        }
    }

    fn make_struct_marker(&mut self, type_name: &str) -> Value {
        let mut marker = IndexMap::new();
        marker.insert("__kind__".to_string(), self.alloc_string("struct"));
        marker.insert("name".to_string(), self.alloc_string(type_name));
        let r = self.gc.alloc(ObjKind::Object(marker));
        Value::Obj(r)
    }

    fn parse_object_fields(&self, value: &Value) -> Result<IndexMap<String, Value>, VMError> {
        match value {
            Value::Obj(r) => match self.gc.get(*r) {
                Some(obj) => match &obj.kind {
                    ObjKind::Object(map) => Ok(map.clone()),
                    _ => Err(VMError::new("expected object value")),
                },
                None => Err(VMError::new("dangling object reference")),
            },
            Value::Null => Ok(IndexMap::new()),
            _ => Err(VMError::new("expected object value")),
        }
    }

    fn get_object_fields(&self, value: &Value) -> Option<IndexMap<String, Value>> {
        match value {
            Value::Obj(r) => self.gc.get(*r).and_then(|obj| match &obj.kind {
                ObjKind::Object(map) => Some(map.clone()),
                _ => None,
            }),
            _ => None,
        }
    }

    fn parse_embedded_fields(&self, value: &Value) -> Result<Vec<(String, String)>, VMError> {
        match value {
            Value::Obj(r) => match self.gc.get(*r) {
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
            Value::Null => Ok(Vec::new()),
            _ => Err(VMError::new("expected embedded field metadata array")),
        }
    }

    fn interface_method_names(&self, iface: &Value) -> Vec<String> {
        let Some(fields) = self.get_object_fields(iface) else {
            return Vec::new();
        };
        let Some(Value::Obj(method_ref)) = fields.get("methods") else {
            return Vec::new();
        };
        let Some(method_obj) = self.gc.get(*method_ref) else {
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
        match value {
            Value::Obj(r) => self.gc.get(*r).is_some_and(|obj| {
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
