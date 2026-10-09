//! Grammar-based generator of small, well-formed, deterministic Forge
//! programs for the differential (interpreter vs VM) fuzz target.
//!
//! The generator consumes an [`arbitrary::Unstructured`] byte stream, so
//! libFuzzer's coverage-guided mutations steer program *structure*, and the
//! stable smoke harness (`tests/fuzz_smoke.rs`) can drive it from a seeded
//! PRNG. Running out of bytes is not an error: every choice then takes its
//! first (simplest) alternative, so generation always terminates.
//!
//! Generated programs stay inside the deterministic core of the language:
//! integer/float/string/bool/null values, arrays, objects, tuples,
//! arithmetic and comparison, `if`/`while`/`for`/`match`/`try`, `break` /
//! `continue`, named functions and closures (including closures that mutate
//! captured variables). No I/O, no time, no randomness, no concurrency.
//!
//! Termination is structural: `while` loops count a dedicated counter that
//! the body never assigns up to a small bound, `for` loops iterate finite
//! literals, and a function or closure can only call functions defined
//! *before* it (no recursion). The harness additionally enforces a
//! wall-clock budget in case nested loops multiply.
//!
//! Each program collects its observable results with `out.push(..)` and
//! ends with the expression `out`, whose displayed value both engines must
//! agree on.

use arbitrary::{Result, Unstructured};

/// Upper bounds that keep programs small and fast.
const MAX_EXPR_DEPTH: u32 = 4;
const MAX_BLOCK_DEPTH: u32 = 3;
const MAX_TOP_STMTS: usize = 14;
const MAX_BLOCK_STMTS: usize = 4;
const MAX_FUNCTIONS: usize = 5;
const MAX_LOOP_BOUND: i64 = 4;

#[derive(Clone)]
struct Var {
    name: String,
    /// Declared `let mut` and not a loop counter: may be assigned.
    assignable: bool,
}

#[derive(Clone)]
struct Callable {
    name: String,
    arity: usize,
}

/// Generate one program.
pub fn program(u: &mut Unstructured<'_>) -> Result<String> {
    let mut g = Gen {
        u,
        src: String::new(),
        indent: 0,
        scopes: vec![Vec::new()],
        callables: Vec::new(),
        next_id: 0,
        loop_depth: 0,
        block_depth: 0,
        in_function: false,
        no_strings: false,
    };
    g.line("let mut out = []");
    let n = g.u.int_in_range(1..=MAX_TOP_STMTS)?;
    for _ in 0..n {
        g.top_stmt()?;
    }
    g.line("out");
    Ok(g.src)
}

struct Gen<'a, 'b> {
    u: &'a mut Unstructured<'b>,
    src: String,
    indent: usize,
    scopes: Vec<Vec<Var>>,
    callables: Vec<Callable>,
    next_id: usize,
    loop_depth: u32,
    block_depth: u32,
    in_function: bool,
    /// Inside a string interpolation: string literals (and the quotes they
    /// would need) cannot appear there.
    no_strings: bool,
}

impl Gen<'_, '_> {
    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.src.push_str("    ");
        }
        self.src.push_str(text);
        self.src.push('\n');
    }

    fn fresh(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{}{}", prefix, self.next_id)
    }

    fn declare(&mut self, name: &str, assignable: bool) {
        self.scopes
            .last_mut()
            .expect("BUG: generator always has a scope")
            .push(Var {
                name: name.to_string(),
                assignable,
            });
    }

    fn visible(&self) -> Vec<Var> {
        self.scopes.iter().flatten().cloned().collect()
    }

    fn pick_var(&mut self, assignable_only: bool) -> Result<Option<String>> {
        let vars: Vec<Var> = self
            .visible()
            .into_iter()
            .filter(|v| !assignable_only || v.assignable)
            .collect();
        if vars.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.u.choose(&vars)?.name.clone()))
    }

    // ----- statements -------------------------------------------------

    fn top_stmt(&mut self) -> Result<()> {
        if self.callables.len() < MAX_FUNCTIONS && self.u.ratio(1, 5)? {
            return self.function_decl();
        }
        self.stmt()
    }

    fn block(&mut self, header: &str, body: impl FnOnce(&mut Self) -> Result<()>) -> Result<()> {
        self.line(&format!("{} {{", header));
        self.indent += 1;
        self.block_depth += 1;
        self.scopes.push(Vec::new());
        let r = body(self);
        self.scopes.pop();
        self.block_depth -= 1;
        self.indent -= 1;
        self.line("}");
        r
    }

    fn stmts(&mut self) -> Result<()> {
        let n = self.u.int_in_range(1..=MAX_BLOCK_STMTS)?;
        for _ in 0..n {
            self.stmt()?;
        }
        Ok(())
    }

    fn stmt(&mut self) -> Result<()> {
        let nested_ok = self.block_depth < MAX_BLOCK_DEPTH;
        let choice = self.u.int_in_range(0..=13)?;
        match choice {
            0 | 1 => {
                let e = self.expr(0)?;
                self.line(&format!("out.push({})", e));
            }
            2 => {
                let name = self.fresh("v");
                let e = self.expr(0)?;
                self.line(&format!("let {} = {}", name, e));
                self.declare(&name, false);
            }
            3 => {
                let name = self.fresh("m");
                let e = self.expr(0)?;
                self.line(&format!("let mut {} = {}", name, e));
                self.declare(&name, true);
            }
            4 => match self.pick_var(true)? {
                Some(name) => {
                    let e = self.expr(0)?;
                    // Plain, compound, field and index assignment.
                    let target = match self.u.int_in_range(0..=5)? {
                        0 => format!("{}.a", name),
                        1 => format!("{}[{}]", name, self.u.int_in_range(-1i64..=2)?),
                        _ => name,
                    };
                    let op = *self.u.choose(&["=", "+=", "-="])?;
                    self.line(&format!("{} {} {}", target, op, e));
                }
                None => self.push_simple()?,
            },
            5 if nested_ok => {
                let c = self.expr(0)?;
                self.block(&format!("if {}", c), |g| g.stmts())?;
                if self.u.ratio(1, 2)? {
                    self.block("else", |g| g.stmts())?;
                }
            }
            6 if nested_ok => self.while_loop()?,
            7 if nested_ok => self.for_loop()?,
            8 if nested_ok => self.match_stmt()?,
            9 if nested_ok => {
                self.block("try", |g| g.stmts())?;
                let e = self.fresh("e");
                self.block(&format!("catch {}", e), |g| {
                    g.line("out.push(\"caught\")");
                    Ok(())
                })?;
            }
            10 if self.loop_depth > 0 && nested_ok => {
                let c = self.expr(0)?;
                let kw = *self.u.choose(&["break", "continue"])?;
                self.line(&format!("if {} {{ {} }}", c, kw));
            }
            11 if nested_ok => self.closure_decl()?,
            12 => {
                // A call for its effects (closures may mutate captures).
                match self.call_expr(0)? {
                    Some(call) => self.line(&call),
                    None => self.push_simple()?,
                }
            }
            _ => self.push_simple()?,
        }
        Ok(())
    }

    fn push_simple(&mut self) -> Result<()> {
        let e = self.atom()?;
        self.line(&format!("out.push({})", e));
        Ok(())
    }

    fn while_loop(&mut self) -> Result<()> {
        let counter = self.fresh("i");
        let bound = self.u.int_in_range(0..=MAX_LOOP_BOUND)?;
        self.line(&format!("let mut {} = 0", counter));
        // The counter is readable but never assignable by generated code.
        self.declare(&counter, false);
        self.loop_depth += 1;
        let r = self.block(&format!("while {} < {}", counter, bound), |g| {
            g.line(&format!("{} = {} + 1", counter, counter));
            g.stmts()
        });
        self.loop_depth -= 1;
        r
    }

    fn for_loop(&mut self) -> Result<()> {
        let var = self.fresh("x");
        let iterable = match self.u.int_in_range(0..=2)? {
            0 => {
                let a = self.u.int_in_range(-1..=2)?;
                let b = self.u.int_in_range(a..=a + MAX_LOOP_BOUND)?;
                format!("range({}, {})", a, b)
            }
            1 => {
                let n = self.u.int_in_range(0..=3)?;
                let mut items = Vec::new();
                for _ in 0..n {
                    items.push(self.expr(2)?);
                }
                format!("[{}]", items.join(", "))
            }
            _ => {
                let s = *self.u.choose(&["\"\"", "\"ab\"", "\"xyz\""])?;
                format!("{}.chars()", s)
            }
        };
        self.loop_depth += 1;
        let r = self.block(&format!("for {} in {}", var, iterable), |g| {
            g.declare(&var, false);
            g.stmts()
        });
        self.loop_depth -= 1;
        r
    }

    fn match_stmt(&mut self) -> Result<()> {
        let scrutinee = self.expr(1)?;
        self.line(&format!("match {} {{", scrutinee));
        self.indent += 1;
        let arms = self.u.int_in_range(1..=3)?;
        for _ in 0..arms {
            let pat = self.pattern_literal()?;
            self.arm(&pat)?;
        }
        if self.u.ratio(3, 4)? {
            if self.u.ratio(1, 2)? {
                self.arm("_")?;
            } else {
                let b = self.fresh("b");
                self.arm_binding(&b)?;
            }
        }
        self.indent -= 1;
        self.line("}");
        Ok(())
    }

    fn arm(&mut self, pat: &str) -> Result<()> {
        self.line(&format!("{} => {{", pat));
        self.arm_body(None)
    }

    fn arm_binding(&mut self, name: &str) -> Result<()> {
        self.line(&format!("{} => {{", name));
        self.arm_body(Some(name))
    }

    fn arm_body(&mut self, binding: Option<&str>) -> Result<()> {
        self.indent += 1;
        self.block_depth += 1;
        self.scopes.push(Vec::new());
        if let Some(b) = binding {
            self.declare(b, false);
        }
        let r = self.stmts();
        self.scopes.pop();
        self.block_depth -= 1;
        self.indent -= 1;
        self.line("}");
        r
    }

    fn params(&mut self) -> Result<Vec<String>> {
        let arity = self.u.int_in_range(0..=3)?;
        Ok((0..arity).map(|_| self.fresh("p")).collect())
    }

    /// A function body: statements then an explicit `return`.
    fn function_body(&mut self, params: &[String]) -> Result<()> {
        let saved_loop = self.loop_depth;
        let saved_in_fn = self.in_function;
        self.loop_depth = 0;
        self.in_function = true;
        self.indent += 1;
        self.block_depth += 1;
        self.scopes.push(Vec::new());
        for p in params {
            // Parameters are mutable in both engines; let bodies assign them.
            self.declare(p, true);
        }
        let n = self.u.int_in_range(0..=MAX_BLOCK_STMTS)?;
        let mut r = Ok(());
        for _ in 0..n {
            r = self.stmt();
            if r.is_err() {
                break;
            }
        }
        if r.is_ok() {
            r = self.expr(0).map(|e| self.line(&format!("return {}", e)));
        }
        self.scopes.pop();
        self.block_depth -= 1;
        self.indent -= 1;
        self.loop_depth = saved_loop;
        self.in_function = saved_in_fn;
        self.line("}");
        r
    }

    /// Top-level `fn`. Named functions see only their parameters, `out`,
    /// and earlier functions — not other top-level variables — so that
    /// hoisting differences cannot matter.
    fn function_decl(&mut self) -> Result<()> {
        let name = self.fresh("f");
        let params = self.params()?;
        self.line(&format!("fn {}({}) {{", name, params.join(", ")));
        let saved = std::mem::replace(&mut self.scopes, vec![Vec::new()]);
        let r = self.function_body(&params);
        self.scopes = saved;
        r?;
        self.callables.push(Callable {
            name,
            arity: params.len(),
        });
        Ok(())
    }

    /// `let c = fn(..) { .. }` — a closure over everything in scope.
    fn closure_decl(&mut self) -> Result<()> {
        let name = self.fresh("c");
        let params = self.params()?;
        self.line(&format!("let {} = fn({}) {{", name, params.join(", ")));
        self.function_body(&params)?;
        self.declare(&name, false);
        self.callables.push(Callable {
            name,
            arity: params.len(),
        });
        Ok(())
    }

    // ----- expressions ------------------------------------------------

    fn literal(&mut self) -> Result<String> {
        Ok(match self.u.int_in_range(0..=6)? {
            0 => self.u.int_in_range(-3i64..=10)?.to_string(),
            1 => self
                .u
                .choose(&[
                    "9223372036854775807",
                    "4611686018427387904",
                    "140737488355328",
                    "1000000007",
                    "(-9223372036854775807 - 1)",
                    "0",
                ])?
                .to_string(),
            2 => self
                .u
                .choose(&["0.5", "1.5", "2.25", "3.0", "0.1", "100.0"])?
                .to_string(),
            3 if self.no_strings => "7".to_string(),
            3 => self
                .u
                .choose(&["\"\"", "\"a\"", "\"bc\"", "\"x y\"", "\"10\"", "\"3.5\""])?
                .to_string(),
            4 => "true".to_string(),
            5 => "false".to_string(),
            _ => "null".to_string(),
        })
    }

    /// Literal patterns accepted by `match`: non-negative ints, floats,
    /// strings and bools (no `null`, no negative numbers).
    fn pattern_literal(&mut self) -> Result<String> {
        let lit = self.literal()?;
        Ok(
            if lit == "null" || lit.starts_with('-') || lit.starts_with('(') {
                "0".to_string()
            } else {
                lit
            },
        )
    }

    fn atom(&mut self) -> Result<String> {
        if self.u.ratio(1, 2)? {
            if let Some(v) = self.pick_var(false)? {
                return Ok(v);
            }
        }
        self.literal()
    }

    fn expr(&mut self, depth: u32) -> Result<String> {
        if depth >= MAX_EXPR_DEPTH {
            return self.atom();
        }
        let d = depth + 1;
        Ok(match self.u.int_in_range(0..=15)? {
            0..=2 => self.atom()?,
            3..=5 => {
                let op = *self.u.choose(&[
                    "+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=", "&&", "||",
                ])?;
                format!("({} {} {})", self.expr(d)?, op, self.expr(d)?)
            }
            6 => {
                let op = *self.u.choose(&["-", "!"])?;
                format!("{}({})", op, self.expr(d)?)
            }
            7 => {
                let n = self.u.int_in_range(0..=3)?;
                let mut items = Vec::new();
                for _ in 0..n {
                    items.push(self.expr(d)?);
                }
                format!("[{}]", items.join(", "))
            }
            8 => {
                let idx = self.u.int_in_range(-1i64..=3)?;
                format!("{}[{}]", self.expr(d)?, idx)
            }
            9 => {
                let n = self.u.int_in_range(0..=2)?;
                let mut fields = Vec::new();
                for key in ["a", "b", "c"].iter().take(n) {
                    fields.push(format!("{}: {}", key, self.expr(d)?));
                }
                format!("{{ {} }}", fields.join(", "))
            }
            10 => {
                let key = *self.u.choose(&["a", "b", "c"])?;
                format!("{}.{}", self.expr(d)?, key)
            }
            11 => format!("({}, {})", self.expr(d)?, self.expr(d)?),
            12 => match self.call_expr(d)? {
                Some(c) => c,
                None => self.atom()?,
            },
            13 => {
                let f = *self.u.choose(&[
                    "len",
                    "str",
                    "typeof",
                    "int",
                    "float",
                    "abs",
                    "reverse",
                    "sum",
                    "keys",
                    "contains",
                    "join",
                    "is_ok",
                    "unwrap_or",
                    "max_of",
                    "min_of",
                ])?;
                let args = match f {
                    "contains" | "unwrap_or" => vec![self.expr(d)?, self.expr(d)?],
                    "join" if self.no_strings => vec![self.expr(d)?],
                    "join" => vec![self.expr(d)?, "\",\"".to_string()],
                    _ => vec![self.expr(d)?],
                };
                format!("{}({})", f, args.join(", "))
            }
            14 if !self.no_strings => {
                // String interpolation of arbitrary (string-free) values.
                self.no_strings = true;
                let inner = self.expr(d);
                self.no_strings = false;
                format!("\"<{{{}}}>\"", inner?)
            }
            _ => {
                let m = *self.u.choose(&["upper", "lower", "trim", "len"])?;
                format!("{}.{}()", self.expr(d)?, m)
            }
        })
    }

    /// A call of a previously defined function or closure with the right
    /// number of arguments.
    fn call_expr(&mut self, depth: u32) -> Result<Option<String>> {
        if self.callables.is_empty() {
            return Ok(None);
        }
        let f = self.u.choose(&self.callables)?.clone();
        // A closure is only callable where its binding is visible. Named
        // functions are visible everywhere after their definition (and
        // `callables` only holds already-finished definitions, so nothing
        // can call itself).
        if f.name.starts_with('c') && !self.visible().iter().any(|v| v.name == f.name) {
            return Ok(None);
        }
        let mut args = Vec::new();
        for _ in 0..f.arity {
            args.push(self.expr(depth + 1)?);
        }
        Ok(Some(format!("{}({})", f.name, args.join(", "))))
    }
}
