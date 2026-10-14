import re
import sys

sys.setrecursionlimit(100000)


class LispError(Exception):
    pass


class Closure:
    def __init__(self, params, body, env):
        self.params, self.body, self.env = params, body, env


class Builtin:
    def __init__(self, name):
        self.name = name


def tokenize(text):
    return re.findall(r"[()]|[^\s()]+", text)


def parse(tokens, i):
    t = tokens[i]
    if t == "(":
        items = []
        i += 1
        while tokens[i] != ")":
            x, i = parse(tokens, i)
            items.append(x)
        return items, i + 1
    if re.fullmatch(r"-?\d+", t):
        return int(t), i + 1
    return t, i + 1  # a symbol (str)


def show(v):
    if v is True:
        return "#t"
    if v is False:
        return "#f"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, tuple):
        return "(" + " ".join(show(x) for x in v) + ")"
    if isinstance(v, Closure):
        return "<lambda>"
    if isinstance(v, Builtin):
        return "<builtin {}>".format(v.name)
    return str(v)


GLOBALS = {}
for name in ["+", "-", "*", "/", "=", "<", ">", "list", "cons", "car", "cdr", "null?", "length"]:
    GLOBALS[name] = Builtin(name)


def lookup(sym, env):
    while env is not None:
        frame, env = env
        if sym in frame:
            return frame[sym]
    if sym in GLOBALS:
        return GLOBALS[sym]
    raise LispError("unbound symbol: " + sym)


def ints(name, args):
    for a in args:
        if not isinstance(a, int) or isinstance(a, bool):
            raise LispError("type error in " + name)
    return args


def call_builtin(name, args):
    if name == "+":
        return sum(ints(name, args))
    if name == "*":
        r = 1
        for a in ints(name, args):
            r *= a
        return r
    if name == "-":
        ints(name, args)
        if len(args) == 1:
            return -args[0]
        r = args[0]
        for a in args[1:]:
            r -= a
        return r
    if name == "/":
        a, b = ints(name, args)
        if b == 0:
            raise LispError("division by zero")
        q = abs(a) // abs(b)
        return q if (a >= 0) == (b >= 0) else -q
    if name in ("=", "<", ">"):
        a, b = ints(name, args)
        return {"=": a == b, "<": a < b, ">": a > b}[name]
    if name == "list":
        return tuple(args)
    if name == "cons":
        if not isinstance(args[1], tuple):
            raise LispError("type error in cons")
        return (args[0],) + args[1]
    if name in ("car", "cdr"):
        lst = args[0]
        if not isinstance(lst, tuple):
            raise LispError("type error in " + name)
        if not lst:
            raise LispError(name + " of empty list")
        return lst[0] if name == "car" else lst[1:]
    if name == "null?":
        return args[0] == ()
    if name == "length":
        if not isinstance(args[0], tuple):
            raise LispError("type error in length")
        return len(args[0])


def evaluate(x, env):
    if isinstance(x, bool):
        return x
    if isinstance(x, int):
        return x
    if isinstance(x, str):
        if x == "#t":
            return True
        if x == "#f":
            return False
        return lookup(x, env)
    if not x:
        return ()
    head = x[0]
    if head == "if":
        c = evaluate(x[1], env)
        return evaluate(x[2] if c is not False else x[3], env)
    if head == "lambda":
        return Closure(x[1], x[2], env)
    if head == "let":
        frame = {}
        for name, e in x[1]:
            frame[name] = evaluate(e, env)
        return evaluate(x[2], (frame, env))
    if head == "begin":
        v = ()
        for e in x[1:]:
            v = evaluate(e, env)
        return v
    f = evaluate(head, env)
    args = [evaluate(a, env) for a in x[1:]]
    if isinstance(f, Builtin):
        return call_builtin(f.name, args)
    if isinstance(f, Closure):
        if len(args) != len(f.params):
            raise LispError("arity: expected {} arguments, got {}".format(len(f.params), len(args)))
        return evaluate(f.body, (dict(zip(f.params, args)), f.env))
    raise LispError("not a function: " + show(f))


tokens = tokenize(sys.stdin.read())
i = 0
while i < len(tokens):
    form, i = parse(tokens, i)
    try:
        if isinstance(form, list) and form and form[0] == "define":
            if isinstance(form[1], list):
                name = form[1][0]
                GLOBALS[name] = Closure(form[1][1:], form[2], None)
            else:
                name = form[1]
                GLOBALS[name] = evaluate(form[2], None)
            print(name)
        else:
            print(show(evaluate(form, None)))
    except LispError as e:
        print("error: {}".format(e))
