import sys
lines = sys.stdin.read().split("\n")
shift = int(lines[0])
text = lines[1]
def enc(t, k):
    out = []
    for ch in t:
        if "a" <= ch <= "z":
            out.append(chr((ord(ch) - 97 + k) % 26 + 97))
        elif "A" <= ch <= "Z":
            out.append(chr((ord(ch) - 65 + k) % 26 + 65))
        else:
            out.append(ch)
    return "".join(out)
e = enc(text, shift)
print(e)
print(enc(e, -shift))
