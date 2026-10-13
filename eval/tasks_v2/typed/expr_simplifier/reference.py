import re
import sys


class ParseError(Exception):
    pass


class Unbound(Exception):
    pass


def tokenize(s):
    return re.findall(r"\(|\)|[^\s()]+", s)


def parse(tokens):
    pos = [0]

    def expr():
        if pos[0] >= len(tokens):
            raise ParseError()
        t = tokens[pos[0]]
        pos[0] += 1
        if re.fullmatch(r"-?\d+", t):
            return ("num", int(t))
        if re.fullmatch(r"[a-z]+", t):
            return ("var", t)
        if t != "(" or pos[0] >= len(tokens):
            raise ParseError()
        op = tokens[pos[0]]
        pos[0] += 1
        if op in ("+", "*"):
            a, b = expr(), expr()
            node = ("add" if op == "+" else "mul", a, b)
        elif op == "-":
            node = ("neg", expr())
        else:
            raise ParseError()
        if pos[0] >= len(tokens) or tokens[pos[0]] != ")":
            raise ParseError()
        pos[0] += 1
        return node

    e = expr()
    if pos[0] != len(tokens):
        raise ParseError()
    return e


def simplify(e):
    k = e[0]
    if k in ("num", "var"):
        return e
    if k == "neg":
        a = simplify(e[1])
        if a[0] == "num":
            return ("num", -a[1])
        if a[0] == "neg":
            return a[1]
        return ("neg", a)
    a, b = simplify(e[1]), simplify(e[2])
    if k == "add":
        if a[0] == "num" and b[0] == "num":
            return ("num", a[1] + b[1])
        if b == ("num", 0):
            return a
        if a == ("num", 0):
            return b
        return ("add", a, b)
    if a[0] == "num" and b[0] == "num":
        return ("num", a[1] * b[1])
    if b == ("num", 1):
        return a
    if a == ("num", 1):
        return b
    if b == ("num", 0) or a == ("num", 0):
        return ("num", 0)
    return ("mul", a, b)


def show(e):
    k = e[0]
    if k == "num":
        return str(e[1])
    if k == "var":
        return e[1]
    if k == "neg":
        inner = show(e[1])
        return "-(" + inner + ")" if e[1][0] in ("add", "mul") else "-" + inner
    if k == "add":
        return show(e[1]) + " + " + show(e[2])

    def operand(x):
        return "(" + show(x) + ")" if x[0] == "add" else show(x)
    return operand(e[1]) + " * " + operand(e[2])


def evaluate(e, env):
    k = e[0]
    if k == "num":
        return e[1]
    if k == "var":
        if e[1] not in env:
            raise Unbound(e[1])
        return env[e[1]]
    if k == "neg":
        return -evaluate(e[1], env)
    a = evaluate(e[1], env)
    b = evaluate(e[2], env)
    return a + b if k == "add" else a * b


for line in sys.stdin.read().split("\n"):
    if not line.strip():
        continue
    binds, _, src = line.partition("|")
    env = {}
    for pair in binds.strip().split(","):
        if pair.strip():
            name, value = pair.strip().split("=")
            env[name] = int(value)
    try:
        e = simplify(parse(tokenize(src)))
    except ParseError:
        print("parse error")
        continue
    try:
        value = str(evaluate(e, env))
    except Unbound as u:
        value = "unbound {}".format(u)
    print("{} => {}".format(show(e), value))
