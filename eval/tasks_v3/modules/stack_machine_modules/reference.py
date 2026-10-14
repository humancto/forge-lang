import re
import sys

# Produces the expected output of the Forge modules driven by files/main.fg.
SIMPLE = {"pop", "add", "sub", "mul", "div", "dup", "swap", "over", "print", "halt"}
NEEDS = {"add": 2, "sub": 2, "mul": 2, "div": 2, "swap": 2, "over": 2,
         "pop": 1, "dup": 1, "jz": 1, "print": 1}


def clean(raw):
    return raw.split("#")[0].strip()


def parse(src):
    labels, count = {}, 0
    lines = src.split("\n")
    for n, raw in enumerate(lines, 1):
        line = clean(raw)
        if not line:
            continue
        if line.endswith(":"):
            name = line[:-1]
            if name in labels:
                return None, "line {}: duplicate label {}".format(n, name)
            labels[name] = count
        else:
            count += 1
    ops = []
    for n, raw in enumerate(lines, 1):
        line = clean(raw)
        if not line or line.endswith(":"):
            continue
        words = [w for w in line.split(" ") if w]
        op = words[0]
        if op in SIMPLE and len(words) == 1:
            ops.append((op, None))
        elif op == "push" and len(words) == 2:
            if not re.fullmatch(r"-?[0-9]+", words[1]):
                return None, "line {}: bad number {}".format(n, words[1])
            ops.append(("push", int(words[1])))
        elif op in ("jz", "jmp") and len(words) == 2:
            if words[1] not in labels:
                return None, "line {}: unknown label {}".format(n, words[1])
            ops.append((op, labels[words[1]]))
        else:
            return None, "line {}: unknown instruction {}".format(n, line)
    return ops, None


def run(ops, max_steps):
    s, out, pc, steps = [], [], 0, 0
    while pc < len(ops):
        steps += 1
        if steps > max_steps:
            return None, "step limit exceeded"
        op, arg = ops[pc]
        if len(s) < NEEDS.get(op, 0):
            return None, "stack underflow at {}".format(pc)
        pc += 1
        if op == "push":
            s.append(arg)
        elif op == "pop":
            s.pop()
        elif op == "dup":
            s.append(s[-1])
        elif op == "over":
            s.append(s[-2])
        elif op == "swap":
            s[-1], s[-2] = s[-2], s[-1]
        elif op == "print":
            out.append(s.pop())
        elif op == "jz":
            if s.pop() == 0:
                pc = arg
        elif op == "jmp":
            pc = arg
        elif op == "halt":
            return out, None
        else:
            b, a = s.pop(), s.pop()
            if op == "div" and b == 0:
                return None, "division by zero at {}".format(pc - 1)
            if op == "add":
                s.append(a + b)
            elif op == "sub":
                s.append(a - b)
            elif op == "mul":
                s.append(a * b)
            else:
                q = abs(a) // abs(b)
                s.append(q if (a >= 0) == (b >= 0) else -q)
    return out, None


ops, err = parse(sys.stdin.read())
if err:
    print("parse error: " + err)
else:
    print("{} instructions".format(len(ops)))
    out, err = run(ops, 10000)
    if err:
        print("runtime error: " + err)
    else:
        print("output: " + " ".join(str(n) for n in out))
