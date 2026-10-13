"""Deterministic deep input for nested_list_parser: prints deep.txt.

Run by the harness as `python3 gen_input.py deep.txt`.
"""
import sys


def deep(depth, k):
    # [k,[k+1,[ ... [k+depth-1] ... ],-1],-2]-like nesting, `depth` lists.
    s = ""
    for i in range(depth):
        s += "[" + str(k + i) + ","
    s = s[:-1]
    for i in range(depth):
        s += "]" if i % 3 else ",-1]"
    return s


def main():
    if (sys.argv[1:] or ["deep.txt"])[0] != "deep.txt":
        sys.exit("unknown file")
    lines = [deep(3000, 1), deep(1500, -700), "[" * 2500 + "]" * 2500,
             "[" * 2000 + "1" + "]" * 1999, deep(10, 5)]
    sys.stdout.write("\n".join(lines) + "\n")


main()
