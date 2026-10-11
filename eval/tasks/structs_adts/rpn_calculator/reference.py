import sys
def calc(tokens):
    st = []
    for t in tokens:
        if t in "+-*/" and len(t) == 1:
            if len(st) < 2:
                return "error: stack underflow"
            b, a = st.pop(), st.pop()
            if t == "+": st.append(a + b)
            elif t == "-": st.append(a - b)
            elif t == "*": st.append(a * b)
            else:
                if b == 0:
                    return "error: division by zero"
                q = abs(a) // abs(b)
                st.append(q if (a >= 0) == (b >= 0) else -q)
        else:
            st.append(int(t))
    if len(st) != 1:
        return "error: leftover operands"
    return str(st[0])
for line in sys.stdin:
    line = line.strip()
    if line:
        print(f"{line} = {calc(line.split())}")
