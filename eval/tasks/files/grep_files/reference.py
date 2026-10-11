import os, re, sys
pat = re.compile(sys.stdin.read().strip(), re.I)
n, files = 0, 0
for name in sorted(os.listdir("notes")):
    if not name.endswith(".md"):
        continue
    hit = False
    for i, line in enumerate(open(os.path.join("notes", name)).read().splitlines(), 1):
        if pat.search(line):
            print(f"{name}:{i}: {line}")
            n += 1
            hit = True
    files += hit
print(f"{n} matches in {files} files")
