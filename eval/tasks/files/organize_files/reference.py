import os, shutil
moves, folders = [], {}
for n in sorted(os.listdir("inbox")):
    ext = os.path.splitext(n)[1][1:] or "misc"
    os.makedirs(ext, exist_ok=True)
    shutil.move(os.path.join("inbox", n), os.path.join(ext, n))
    print(f"inbox/{n} -> {ext}/{n}")
    folders.setdefault(ext, []).append(n)
print("inbox empty: " + str(len(os.listdir("inbox")) == 0).lower())
for f in sorted(folders):
    print(f"{f}: {' '.join(sorted(os.listdir(f)))}")
