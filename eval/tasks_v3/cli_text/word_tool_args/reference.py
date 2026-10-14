import re
import sys

USAGE = "usage: wordtool [--top N] [--min-length N] [--ignore-case] [--]"


def usage(msg):
    sys.stderr.write("error: {}\n{}\n".format(msg, USAGE))
    sys.exit(2)


top, min_len, ignore_case = 5, 1, False
args = sys.argv[1:]
i = 0
while i < len(args):
    a = args[i]
    if a in ("--top", "--min-length"):
        if i + 1 >= len(args):
            usage("{} needs a value".format(a))
        v = args[i + 1]
        if not re.fullmatch(r"[0-9]+", v) or int(v) == 0:
            usage("{} needs a positive integer, got '{}'".format(a, v))
        if a == "--top":
            top = int(v)
        else:
            min_len = int(v)
        i += 2
    elif a == "--ignore-case":
        ignore_case = True
        i += 1
    else:
        usage("unknown option '{}'".format(a))

counts = {}
for w in re.findall(r"[A-Za-z']+", sys.stdin.read()):
    w = w.strip("'")
    if ignore_case:
        w = w.lower()
    if len(w) >= min_len and w:
        counts[w] = counts.get(w, 0) + 1
ranked = sorted(counts.items(), key=lambda kv: (-kv[1], kv[0]))[:top]
for w, c in ranked:
    print("{:>4} {}".format(c, w))
sys.stderr.write("{} distinct words\n".format(len(counts)))
