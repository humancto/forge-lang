import re
import sys


class Fail(Exception):
    pass


def tokenize(src):
    tokens, i = [], 0
    while i < len(src):
        c = src[i]
        if c == " ":
            i += 1
        elif c.isdigit():
            j = i
            while j < len(src) and src[j].isdigit():
                j += 1
            tokens.append(("num", src[i:j]))
            i = j
        elif re.fullmatch(r"[a-z_]", c):
            j = i
            while j < len(src) and re.fullmatch(r"[a-z_0-9]", src[j]):
                j += 1
            tokens.append(("name", src[i:j]))
            i = j
        elif c in "+-*/%()":
            tokens.append(("op", c))
            i += 1
        else:
            raise Fail("unexpected character '{}' at {}".format(c, i + 1))
    return tokens


def parse(src):
    toks = tokenize(src)

    def peek(p):
        return toks[p][1] if p < len(toks) else ""

    def psum(p):
        node, p = pprod(p)
        while peek(p) in ("+", "-"):
            op = peek(p)
            right, p = pprod(p + 1)
            node = ("bin", op, node, right)
        return node, p

    def pprod(p):
        node, p = punary(p)
        while peek(p) in ("*", "/", "%"):
            op = peek(p)
            right, p = punary(p + 1)
            node = ("bin", op, node, right)
        return node, p

    def punary(p):
        if peek(p) == "-":
            inner, p = punary(p + 1)
            return ("neg", inner), p
        return patom(p)

    def patom(p):
        if p >= len(toks):
            raise Fail("unexpected end")
        kind, text = toks[p]
        if kind == "num":
            return ("num", int(text)), p + 1
        if kind == "name":
            return ("var", text), p + 1
        if text == "(":
            inner, p = psum(p + 1)
            if peek(p) != ")":
                raise Fail("missing )")
            return inner, p + 1
        raise Fail("unexpected '{}'".format(text))

    node, p = psum(0)
    if p < len(toks):
        raise Fail("unexpected '{}'".format(toks[p][1]))
    return node


def trunc_div(a, b):
    q = abs(a) // abs(b)
    return q if (a < 0) == (b < 0) else -q


def evaluate(node, env):
    kind = node[0]
    if kind == "num":
        return node[1]
    if kind == "var":
        if node[1] not in env:
            raise Fail("unknown variable: " + node[1])
        return env[node[1]]
    if kind == "neg":
        return -evaluate(node[1], env)
    _, op, left, right = node
    a, b = evaluate(left, env), evaluate(right, env)
    if op == "+":
        return a + b
    if op == "-":
        return a - b
    if op == "*":
        return a * b
    if b == 0:
        raise Fail("division by zero")
    q = trunc_div(a, b)
    return q if op == "/" else a - b * q


env = {}
for line in sys.stdin.read().splitlines():
    line = line.strip()
    if not line:
        continue
    try:
        if line.startswith("let "):
            name, _, expr = line[4:].partition("=")
            name = name.strip()
            if not re.fullmatch(r"[a-z_][a-z_0-9]*", name):
                raise Fail("bad name")
            env[name] = evaluate(parse(expr.strip()), env)
            print("{} = {}".format(name, env[name]))
        else:
            print(evaluate(parse(line), env))
    except Fail as e:
        print("error: {}".format(e))
