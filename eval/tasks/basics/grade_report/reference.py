students = [("Ada", 93), ("Brian", 78), ("Chen", 85), ("Dora", 61), ("Eli", 88), ("Fay", 54)]
def letter(s):
    if s >= 90: return "A"
    if s >= 80: return "B"
    if s >= 70: return "C"
    if s >= 60: return "D"
    return "F"
for n, s in students:
    print(f"{n}: {s} {letter(s)}")
avg = sum(s for _, s in students) / len(students)
print(f"average: {avg:.1f}")
