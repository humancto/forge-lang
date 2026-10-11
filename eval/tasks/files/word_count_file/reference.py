text = open("poem.txt").read()
lines = text.split("\n")
print(f"lines={text.count(chr(10))} words={len(text.split())} chars={len(text)}")
print("longest: " + max(lines, key=len))
