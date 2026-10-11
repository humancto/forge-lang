import sys
pairs = {")": "(", "]": "[", "}": "{"}
for line in sys.stdin.read().splitlines():
    if not line:
        continue
    stack, res = [], "ok"
    for i, c in enumerate(line, 1):
        if c in "([{":
            stack.append((c, i))
        elif c in pairs:
            if not stack or stack[-1][0] != pairs[c]:
                res = f"error at {i}: unexpected {c}"
                break
            stack.pop()
    else:
        if stack:
            c, i = stack[-1]
            res = f"error at {i}: unclosed {c}"
    print(f"{res} | {line}")
