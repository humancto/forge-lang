import re
import sys

lines = sys.stdin.read().split("\n")
if lines and lines[-1] == "":
    lines.pop()
try:
    pattern = re.compile(lines[0])
except re.error:
    print("error: invalid pattern")
    sys.exit(2)
count = 0
for n, line in enumerate(lines[1:], 1):
    if pattern.search(line):
        count += 1
        print("{}:{}".format(n, line))
print("{} match(es)".format(count))
sys.exit(0 if count else 1)
